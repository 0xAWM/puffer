use super::append_tool_invocations;
use super::common::open_text_file_in_editor;
use super::emit_system;
use crate::plans::{plan_file_path, plan_has_user_content, read_plan_text};
use crate::runtime::RequestToolFilter;
use crate::{AppState, MessageRole};
use anyhow::Result;
use puffer_provider_registry::{AuthStore, ProviderRegistry};
use puffer_resources::LoadedResources;
use puffer_session_store::{SessionStore, TranscriptEvent};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

/// Describes how a prompt command should be handled after specialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PromptCommandPreparation {
    /// The command already produced local output and should skip provider execution.
    HandledLocally,
    /// The command should execute through the provider with a custom prompt body.
    PromptOverride(String),
    /// The command should submit the provided text as a normal user prompt.
    DirectPrompt(String),
    /// The command should execute as a one-off side question outside the main transcript.
    SideQuestion(String),
    /// The command should render its resource prompt with extra computed variables.
    VariableOverrides(BTreeMap<String, String>),
}

/// Returns any specialized handling required for prompt commands with local semantics.
pub(crate) fn prepare_prompt_command_specialization(
    state: &mut AppState,
    session_store: &SessionStore,
    command_name: &str,
    args: &str,
) -> Result<Option<PromptCommandPreparation>> {
    match command_name {
        "btw" => Ok(Some(prepare_btw_prompt_command(
            state,
            session_store,
            args,
        )?)),
        "compact" => Ok(Some(prepare_compact_prompt_command(
            state,
            session_store,
            args,
        )?)),
        "pr-comments" => Ok(Some(prepare_pr_comments_prompt_command(args))),
        "security-review" => Ok(Some(prepare_security_review_prompt_command(state, args)?)),
        "statusline" => Ok(Some(prepare_statusline_prompt_command(args)?)),
        _ => Ok(None),
    }
}

/// Prepares `/btw` side-question handling without appending a user prompt to the main transcript.
pub(crate) fn prepare_btw_prompt_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<PromptCommandPreparation> {
    let question = args.trim();
    if question.is_empty() {
        emit_system(
            state,
            session_store,
            "Usage: /btw <your question>".to_string(),
        )?;
        return Ok(PromptCommandPreparation::HandledLocally);
    }
    Ok(PromptCommandPreparation::SideQuestion(question.to_string()))
}

/// Prepares `/compact` by generating a provider-driven compaction prompt override.
pub(crate) fn prepare_compact_prompt_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<PromptCommandPreparation> {
    if state.transcript.is_empty() {
        emit_system(
            state,
            session_store,
            "No messages are available to compact.".to_string(),
        )?;
        return Ok(PromptCommandPreparation::HandledLocally);
    }
    Ok(PromptCommandPreparation::PromptOverride(
        build_compact_prompt_override(state, args),
    ))
}

/// Handles `/plan` local behaviors using Claude-style plan-mode semantics.
pub(crate) fn prepare_plan_prompt_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<PromptCommandPreparation> {
    let plan_path = plan_file_path(state)?;
    let trimmed = args.trim();

    if !state.plan_mode {
        state.plan_mode = true;
        emit_system(state, session_store, "Enabled plan mode".to_string())?;
        if trimmed.is_empty() || trimmed == "open" {
            return Ok(PromptCommandPreparation::HandledLocally);
        }
        return Ok(PromptCommandPreparation::DirectPrompt(trimmed.to_string()));
    }

    let Some(plan_body) = read_plan_text(state)?.filter(|text| plan_has_user_content(text)) else {
        emit_system(
            state,
            session_store,
            "Already in plan mode. No plan written yet.".to_string(),
        )?;
        return Ok(PromptCommandPreparation::HandledLocally);
    };

    if trimmed.split_whitespace().next() == Some("open") {
        let status = match open_text_file_in_editor(&plan_path) {
            Ok(_) => format!("Opened plan in editor: {}", plan_path.display()),
            Err(error) => format!("Failed to open plan in editor: {error}"),
        };
        emit_system(state, session_store, status)?;
        return Ok(PromptCommandPreparation::HandledLocally);
    }
    emit_system(
        state,
        session_store,
        render_current_plan_message(&plan_path, &plan_body),
    )?;
    Ok(PromptCommandPreparation::HandledLocally)
}

/// Handles `/plan` from the local command path while preserving its direct-prompt behavior.
pub(crate) fn handle_plan_command(
    state: &mut AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    auth_store: &mut AuthStore,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    match prepare_plan_prompt_command(state, session_store, args)? {
        PromptCommandPreparation::HandledLocally => Ok(()),
        PromptCommandPreparation::DirectPrompt(prompt) => {
            record_specialized_prompt_request(state, session_store, &prompt)?;
            match crate::runtime::execute_user_prompt(
                state, resources, providers, auth_store, &prompt,
            ) {
                Ok(turn) => {
                    append_tool_invocations(state, session_store, &turn.tool_invocations)?;
                    state.push_message(MessageRole::Assistant, turn.assistant_text.clone());
                    session_store.append_event(
                        state.session.id,
                        TranscriptEvent::AssistantMessage {
                            text: turn.assistant_text,
                        },
                    )?;
                    Ok(())
                }
                Err(error) => emit_system(
                    state,
                    session_store,
                    format!("Plan mode query failed: {error}"),
                ),
            }
        }
        PromptCommandPreparation::PromptOverride(_)
        | PromptCommandPreparation::SideQuestion(_)
        | PromptCommandPreparation::VariableOverrides(_) => {
            unreachable!("/plan only uses local and direct-prompt branches")
        }
    }
}

/// Supplies the optional user-input block used by the declarative `/pr-comments` prompt.
pub(crate) fn prepare_pr_comments_prompt_command(args: &str) -> PromptCommandPreparation {
    PromptCommandPreparation::VariableOverrides(build_pr_comments_prompt_variables(args))
}

/// Computes git-aware context variables for `/security-review`.
pub(crate) fn prepare_security_review_prompt_command(
    state: &AppState,
    args: &str,
) -> Result<PromptCommandPreparation> {
    Ok(PromptCommandPreparation::VariableOverrides(
        build_security_review_prompt_variables(&state.cwd, args),
    ))
}

/// Builds the Claude-style `/statusline` setup variables.
pub(crate) fn prepare_statusline_prompt_command(args: &str) -> Result<PromptCommandPreparation> {
    Ok(PromptCommandPreparation::VariableOverrides(
        build_statusline_prompt_variables(args)?,
    ))
}

/// Executes the provider-backed `/compact` prompt and persists the compacted transcript.
pub(crate) fn execute_compact_prompt_command(
    state: &mut AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    auth_store: &mut AuthStore,
    session_store: &SessionStore,
    rendered: &str,
    tool_filter: Option<&RequestToolFilter>,
) -> Result<()> {
    record_specialized_prompt_request(state, session_store, rendered)?;
    match crate::runtime::execute_user_prompt_with_tool_filter(
        state,
        resources,
        providers,
        auth_store,
        rendered,
        tool_filter,
    ) {
        Ok(turn) => {
            append_tool_invocations(state, session_store, &turn.tool_invocations)?;
            finalize_compact_prompt_command(state, session_store, &turn.assistant_text)
        }
        Err(error) => emit_system(
            state,
            session_store,
            format!("Prompt command /compact failed: {error}"),
        ),
    }
}

fn record_specialized_prompt_request(
    state: &mut AppState,
    session_store: &SessionStore,
    rendered: &str,
) -> Result<()> {
    state.push_message(MessageRole::User, rendered.to_string());
    session_store.append_event(
        state.session.id,
        TranscriptEvent::UserMessage {
            text: rendered.to_string(),
        },
    )?;
    Ok(())
}

/// Applies a provider-generated compaction summary and persists the transcript rewrite.
pub(crate) fn finalize_compact_prompt_command(
    state: &mut AppState,
    session_store: &SessionStore,
    summary: &str,
) -> Result<()> {
    session_store.append_transcript_clear(state.session.id)?;
    state.apply_transcript_rewrite(&puffer_session_store::TranscriptRewrite::Clear);
    emit_system(
        state,
        session_store,
        format!("Compacted conversation summary:\n{}", summary.trim_end()),
    )
}

/// Renders the active plan-mode context block for provider requests.
pub(crate) fn plan_mode_context_message(state: &AppState) -> Result<Option<String>> {
    if !state.plan_mode {
        return Ok(None);
    }
    let plan_path = plan_file_path(state)?;
    let plan_text = read_plan_text(state)?.unwrap_or_default();
    let plan_body = if plan_text.trim().is_empty() {
        "<empty>"
    } else {
        plan_text.trim_end()
    };
    Ok(Some(format!(
        "Plan mode is active. The user wants planning only right now.\n\
Do not edit files other than the plan file, and do not implement code until plan mode is exited.\n\
Update the plan file as you learn more, use AskUserQuestion for clarifications, and use ExitPlanMode when the plan is ready for approval.\n\
The active plan file is: {}\n\
\n\
Current plan contents:\n{}",
        plan_path.display(),
        plan_body
    )))
}

fn build_compact_prompt_override(state: &AppState, args: &str) -> String {
    let trimmed_instruction = args.trim();
    let mut user_messages = 0usize;
    let mut assistant_messages = 0usize;
    let mut system_messages = 0usize;
    let mut highlights = Vec::new();

    for message in state.transcript.iter().rev() {
        match message.role {
            MessageRole::User => user_messages += 1,
            MessageRole::Assistant => assistant_messages += 1,
            MessageRole::System => system_messages += 1,
        }
        if highlights.len() >= 8 {
            continue;
        }
        let compact_line = single_line_excerpt(&message.text);
        if compact_line.is_empty() {
            continue;
        }
        let role = match message.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
        };
        highlights.push(format!("- {role}: {compact_line}"));
    }
    highlights.reverse();

    let mut text = String::from(
        "Summarize the current conversation so work can continue with a compact preserved context.\n",
    );
    let _ = writeln!(
        &mut text,
        "messages: user={} assistant={} system={}",
        user_messages, assistant_messages, system_messages
    );
    if !trimmed_instruction.is_empty() {
        let _ = writeln!(&mut text, "custom_instruction: {trimmed_instruction}");
    }
    text.push_str("Return only the compact summary that should remain in context.\n");
    if highlights.is_empty() {
        text.push_str("highlights:\n- <no non-empty messages>");
    } else {
        text.push_str("highlights:\n");
        text.push_str(&highlights.join("\n"));
    }
    text
}

fn build_pr_comments_prompt_variables(args: &str) -> BTreeMap<String, String> {
    let trimmed = args.trim();
    BTreeMap::from([(
        "ADDITIONAL_USER_INPUT_BLOCK".to_string(),
        if trimmed.is_empty() {
            String::new()
        } else {
            format!("Additional user input: {trimmed}")
        },
    )])
}

fn build_statusline_prompt_variables(args: &str) -> Result<BTreeMap<String, String>> {
    let prompt = if args.trim().is_empty() {
        "Configure my statusLine from my shell PS1 configuration".to_string()
    } else {
        args.trim().to_string()
    };
    Ok(BTreeMap::from([(
        "STATUSLINE_PROMPT_JSON".to_string(),
        serde_json::to_string(&prompt)?,
    )]))
}

fn build_security_review_prompt_variables(cwd: &Path, args: &str) -> BTreeMap<String, String> {
    let trimmed = args.trim();
    BTreeMap::from([
        (
            "GIT_STATUS".to_string(),
            run_git_with_fallbacks(cwd, &[&["status"]]),
        ),
        (
            "FILES_MODIFIED".to_string(),
            run_git_with_fallbacks(
                cwd,
                &[
                    &["diff", "--name-only", "origin/HEAD..."],
                    &["diff", "--name-only"],
                ],
            ),
        ),
        (
            "COMMITS".to_string(),
            run_git_with_fallbacks(
                cwd,
                &[
                    &["log", "--no-decorate", "origin/HEAD..."],
                    &["log", "--no-decorate", "-n", "10"],
                ],
            ),
        ),
        (
            "DIFF_CONTENT".to_string(),
            run_git_with_fallbacks(cwd, &[&["diff", "origin/HEAD..."], &["diff"]]),
        ),
        (
            "ADDITIONAL_USER_INPUT_BLOCK".to_string(),
            if trimmed.is_empty() {
                String::new()
            } else {
                format!("Additional user input: {trimmed}")
            },
        ),
    ])
}

fn run_git_with_fallbacks(cwd: &Path, candidates: &[&[&str]]) -> String {
    let mut last_failure = String::new();
    for candidate in candidates {
        match run_git_command(cwd, candidate) {
            Ok(output) => return output,
            Err(error) => last_failure = error,
        }
    }
    last_failure
}

fn run_git_command(cwd: &Path, args: &[&str]) -> std::result::Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .map_err(|error| format!("Failed to run `git {}`: {error}", args.join(" ")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        if stdout.is_empty() {
            Ok("<no output>".to_string())
        } else {
            Ok(stdout)
        }
    } else {
        let exit = output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".to_string());
        Err(format!(
            "Command `git {}` failed with exit code {exit}.\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            if stdout.is_empty() {
                "<no output>"
            } else {
                &stdout
            },
            if stderr.is_empty() {
                "<no output>"
            } else {
                &stderr
            }
        ))
    }
}

fn single_line_excerpt(text: &str) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("");
    if line.chars().count() <= 120 {
        line.to_string()
    } else {
        let mut shortened = String::new();
        for ch in line.chars().take(117) {
            shortened.push(ch);
        }
        shortened.push_str("...");
        shortened
    }
}
fn render_current_plan_message(plan_path: &Path, plan_body: &str) -> String {
    let mut message = format!("Current Plan\n{}", plan_path.display());
    if !plan_body.is_empty() {
        let _ = write!(&mut message, "\n\n{}", plan_body.trim_end());
    }
    if let Some(editor_name) = configured_editor_display_name() {
        let _ = writeln!(
            &mut message,
            "\n\n\"/plan open\" to edit this plan in {}",
            editor_name
        );
        return message.trim_end().to_string();
    }
    message
}

fn configured_editor_display_name() -> Option<String> {
    std::env::var("VISUAL")
        .ok()
        .or_else(|| std::env::var("EDITOR").ok())
        .and_then(|command| {
            let binary = command.split_whitespace().next()?;
            let basename = std::path::Path::new(binary)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(binary)
                .to_ascii_lowercase();
            let display = match basename.as_str() {
                "code" => "VS Code",
                "cursor" => "Cursor",
                "windsurf" => "Windsurf",
                "codium" => "VSCodium",
                "nvim" => "Neovim",
                "vim" => "Vim",
                "vi" => "vi",
                "nano" => "nano",
                _ => binary,
            };
            Some(display.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::{
        handle_plan_command, plan_mode_context_message, prepare_btw_prompt_command,
        prepare_compact_prompt_command, prepare_plan_prompt_command,
        prepare_pr_comments_prompt_command, prepare_prompt_command_specialization,
        prepare_security_review_prompt_command, prepare_statusline_prompt_command,
        PromptCommandPreparation,
    };
    use crate::plans::{ensure_plan_file, persist_plan_output, plan_file_path};
    use crate::{AppState, MessageRole};
    use puffer_config::{ensure_workspace_dirs, ConfigPaths, PufferConfig};
    use puffer_provider_registry::{AuthStore, ProviderRegistry};
    use puffer_resources::LoadedResources;
    use puffer_session_store::SessionStore;
    use tempfile::tempdir;
    use tempfile::TempDir;

    #[test]
    fn compact_specialization_returns_prompt_override() {
        let fixture = sample_state();
        let mut state = fixture.state;
        let session_store = fixture.session_store;
        state.push_message(MessageRole::User, "Ship this change.");
        state.push_message(MessageRole::Assistant, "Implemented and tested.");

        let outcome =
            prepare_compact_prompt_command(&mut state, &session_store, "focus on tests").unwrap();

        match outcome {
            PromptCommandPreparation::PromptOverride(prompt) => {
                assert!(prompt.contains("Summarize the current conversation"));
                assert!(prompt.contains("custom_instruction: focus on tests"));
            }
            PromptCommandPreparation::DirectPrompt(_)
            | PromptCommandPreparation::HandledLocally
            | PromptCommandPreparation::SideQuestion(_)
            | PromptCommandPreparation::VariableOverrides(_) => {
                panic!("expected compact prompt override")
            }
        }
    }

    #[test]
    fn btw_specialization_requires_a_question() {
        let fixture = sample_state();
        let mut state = fixture.state;
        let session_store = fixture.session_store;

        let outcome = prepare_btw_prompt_command(&mut state, &session_store, "").unwrap();

        assert_eq!(outcome, PromptCommandPreparation::HandledLocally);
        assert!(state
            .transcript
            .last()
            .unwrap()
            .text
            .contains("Usage: /btw <your question>"));
    }

    #[test]
    fn btw_specialization_uses_side_question_variant() {
        let fixture = sample_state();
        let mut state = fixture.state;
        let session_store = fixture.session_store;

        let outcome =
            prepare_btw_prompt_command(&mut state, &session_store, "what changed?").unwrap();

        assert_eq!(
            outcome,
            PromptCommandPreparation::SideQuestion("what changed?".to_string())
        );
    }

    #[test]
    fn plan_specialization_enables_mode_without_creating_a_plan_file() {
        let fixture = sample_state();
        let mut state = fixture.state;
        let session_store = fixture.session_store;
        let plan_path = plan_file_path(&state).unwrap();

        let show_outcome = prepare_plan_prompt_command(&mut state, &session_store, "").unwrap();
        assert_eq!(show_outcome, PromptCommandPreparation::HandledLocally);
        assert!(state.plan_mode);
        assert!(!plan_path.exists());
        assert!(state
            .transcript
            .last()
            .unwrap()
            .text
            .contains("Enabled plan mode"));
    }

    #[test]
    fn plan_specialization_with_description_submits_raw_prompt_after_enabling_mode() {
        let fixture = sample_state();
        let mut state = fixture.state;
        let session_store = fixture.session_store;
        let plan_path = plan_file_path(&state).unwrap();
        let outcome = prepare_plan_prompt_command(
            &mut state,
            &session_store,
            "stabilize slash-command parity",
        )
        .unwrap();

        match outcome {
            PromptCommandPreparation::DirectPrompt(prompt) => {
                assert_eq!(prompt, "stabilize slash-command parity");
                assert!(state.plan_mode);
                assert!(!plan_path.exists());
                assert!(state
                    .transcript
                    .last()
                    .unwrap()
                    .text
                    .contains("Enabled plan mode"));
            }
            PromptCommandPreparation::PromptOverride(_)
            | PromptCommandPreparation::HandledLocally
            | PromptCommandPreparation::SideQuestion(_)
            | PromptCommandPreparation::VariableOverrides(_) => {
                panic!("expected direct prompt for non-empty plan arguments")
            }
        }
    }

    #[test]
    fn plan_specialization_shows_existing_plan_when_already_active() {
        let fixture = sample_state();
        let mut state = fixture.state;
        let session_store = fixture.session_store;
        state.plan_mode = true;
        persist_plan_output(&state, "# Current Plan\n\n1. Verify tooling\n").unwrap();

        let outcome =
            prepare_plan_prompt_command(&mut state, &session_store, "next-step ignored").unwrap();

        assert_eq!(outcome, PromptCommandPreparation::HandledLocally);
        assert!(state
            .transcript
            .last()
            .unwrap()
            .text
            .contains("Current Plan"));
        assert!(state
            .transcript
            .last()
            .unwrap()
            .text
            .contains("Verify tooling"));
    }

    #[test]
    fn plan_open_reports_missing_plan_when_no_plan_exists() {
        let fixture = sample_state();
        let mut state = fixture.state;
        let session_store = fixture.session_store;
        state.plan_mode = true;

        let outcome = prepare_plan_prompt_command(&mut state, &session_store, "open").unwrap();

        assert_eq!(outcome, PromptCommandPreparation::HandledLocally);
        assert!(!plan_file_path(&state).unwrap().exists());
        assert!(state
            .transcript
            .last()
            .unwrap()
            .text
            .contains("Already in plan mode. No plan written yet."));
    }

    #[test]
    fn plan_specialization_reports_missing_plan_when_already_active() {
        let fixture = sample_state();
        let mut state = fixture.state;
        let session_store = fixture.session_store;
        state.plan_mode = true;

        let outcome = prepare_plan_prompt_command(&mut state, &session_store, "").unwrap();

        assert_eq!(outcome, PromptCommandPreparation::HandledLocally);
        assert!(state
            .transcript
            .last()
            .unwrap()
            .text
            .contains("Already in plan mode. No plan written yet."));
    }

    #[test]
    fn plan_specialization_treats_default_scaffold_as_missing_plan() {
        let fixture = sample_state();
        let mut state = fixture.state;
        let session_store = fixture.session_store;
        state.plan_mode = true;
        ensure_plan_file(&state).unwrap();

        let outcome = prepare_plan_prompt_command(&mut state, &session_store, "").unwrap();

        assert_eq!(outcome, PromptCommandPreparation::HandledLocally);
        assert!(state
            .transcript
            .last()
            .unwrap()
            .text
            .contains("Already in plan mode. No plan written yet."));
    }

    #[test]
    fn handle_plan_command_executes_direct_prompt_after_entering_plan_mode() {
        let fixture = sample_state();
        let mut state = fixture.state;
        let session_store = fixture.session_store;

        handle_plan_command(
            &mut state,
            &LoadedResources::default(),
            &ProviderRegistry::new(),
            &mut AuthStore::default(),
            &session_store,
            "stabilize slash-command parity",
        )
        .unwrap();

        assert!(state.plan_mode);
        assert!(state
            .transcript
            .iter()
            .any(|message| message.text == "Enabled plan mode"));
        assert!(state.transcript.iter().any(|message| {
            message.role == MessageRole::User && message.text == "stabilize slash-command parity"
        }));
        assert!(
            state
                .transcript
                .iter()
                .any(|message| message.text.starts_with("Plan mode query failed:")),
            "{:?}",
            state.transcript
        );
    }

    #[test]
    fn pr_comments_specialization_supplies_optional_input_block() {
        let empty = prepare_pr_comments_prompt_command("");
        let targeted = prepare_pr_comments_prompt_command("123");

        match empty {
            PromptCommandPreparation::VariableOverrides(variables) => {
                assert_eq!(
                    variables.get("ADDITIONAL_USER_INPUT_BLOCK"),
                    Some(&String::new())
                );
            }
            PromptCommandPreparation::DirectPrompt(_)
            | PromptCommandPreparation::HandledLocally
            | PromptCommandPreparation::SideQuestion(_)
            | PromptCommandPreparation::PromptOverride(_) => {
                panic!("expected variable overrides")
            }
        }
        match targeted {
            PromptCommandPreparation::VariableOverrides(variables) => {
                assert_eq!(
                    variables.get("ADDITIONAL_USER_INPUT_BLOCK"),
                    Some(&"Additional user input: 123".to_string())
                );
            }
            PromptCommandPreparation::DirectPrompt(_)
            | PromptCommandPreparation::HandledLocally
            | PromptCommandPreparation::SideQuestion(_)
            | PromptCommandPreparation::PromptOverride(_) => {
                panic!("expected variable overrides")
            }
        }
    }

    #[test]
    fn security_review_specialization_collects_git_context() {
        let fixture = sample_state();
        let state = fixture.state;
        let outcome = prepare_security_review_prompt_command(&state, "").unwrap();

        match outcome {
            PromptCommandPreparation::VariableOverrides(variables) => {
                assert!(variables.contains_key("GIT_STATUS"));
                assert!(variables.contains_key("FILES_MODIFIED"));
                assert!(variables.contains_key("COMMITS"));
                assert!(variables.contains_key("DIFF_CONTENT"));
            }
            PromptCommandPreparation::DirectPrompt(_)
            | PromptCommandPreparation::HandledLocally
            | PromptCommandPreparation::SideQuestion(_)
            | PromptCommandPreparation::PromptOverride(_) => {
                panic!("expected variable overrides")
            }
        }
    }

    #[test]
    fn statusline_specialization_uses_agent_setup_prompt() {
        let outcome = prepare_statusline_prompt_command("").unwrap();
        match outcome {
            PromptCommandPreparation::VariableOverrides(variables) => {
                assert_eq!(
                    variables.get("STATUSLINE_PROMPT_JSON"),
                    Some(
                        &"\"Configure my statusLine from my shell PS1 configuration\"".to_string()
                    )
                );
            }
            PromptCommandPreparation::DirectPrompt(_)
            | PromptCommandPreparation::HandledLocally
            | PromptCommandPreparation::SideQuestion(_)
            | PromptCommandPreparation::PromptOverride(_) => panic!("expected variable overrides"),
        }
    }

    #[test]
    fn dispatcher_helper_routes_known_prompt_specializations() {
        let fixture = sample_state();
        let mut state = fixture.state;
        let session_store = fixture.session_store;
        state.push_message(MessageRole::User, "summarize this");
        let compact =
            prepare_prompt_command_specialization(&mut state, &session_store, "compact", "")
                .unwrap();
        match compact {
            Some(PromptCommandPreparation::PromptOverride(prompt)) => {
                assert!(prompt.contains("Summarize the current conversation"));
            }
            _ => panic!("expected compact prompt override"),
        }

        let pr_comments =
            prepare_prompt_command_specialization(&mut state, &session_store, "pr-comments", "")
                .unwrap();
        match pr_comments {
            Some(PromptCommandPreparation::VariableOverrides(variables)) => {
                assert!(variables.contains_key("ADDITIONAL_USER_INPUT_BLOCK"));
            }
            _ => panic!("expected pr-comments prompt variable overrides"),
        }

        let statusline =
            prepare_prompt_command_specialization(&mut state, &session_store, "statusline", "")
                .unwrap();
        match statusline {
            Some(PromptCommandPreparation::VariableOverrides(variables)) => {
                assert!(variables.contains_key("STATUSLINE_PROMPT_JSON"));
            }
            _ => panic!("expected statusline prompt variable overrides"),
        }

        let security_review = prepare_prompt_command_specialization(
            &mut state,
            &session_store,
            "security-review",
            "",
        )
        .unwrap();
        match security_review {
            Some(PromptCommandPreparation::VariableOverrides(variables)) => {
                assert!(variables.contains_key("DIFF_CONTENT"));
            }
            _ => panic!("expected security-review variable overrides"),
        }

        let plan =
            prepare_prompt_command_specialization(&mut state, &session_store, "plan", "").unwrap();
        assert!(plan.is_none());

        let none = prepare_prompt_command_specialization(&mut state, &session_store, "review", "")
            .unwrap();
        assert!(none.is_none());
    }

    #[test]
    fn plan_mode_context_does_not_create_a_default_plan_file() {
        let fixture = sample_state();
        let mut state = fixture.state;
        state.plan_mode = true;

        let context = plan_mode_context_message(&state)
            .unwrap()
            .expect("plan mode context");

        assert!(context.contains("Current plan contents:\n<empty>"));
        assert!(!plan_file_path(&state).unwrap().exists());
    }

    struct TestFixture {
        #[allow(dead_code)]
        tempdir: TempDir,
        state: AppState,
        session_store: SessionStore,
    }

    fn sample_state() -> TestFixture {
        let tempdir = tempdir().unwrap();
        let paths = ConfigPaths::discover(tempdir.path());
        ensure_workspace_dirs(&paths).unwrap();
        let session_store = SessionStore::from_paths(&paths).unwrap();
        let session = session_store
            .create_session(tempdir.path().to_path_buf())
            .unwrap();
        let state = AppState::new(
            PufferConfig::default(),
            tempdir.path().to_path_buf(),
            session,
        );
        TestFixture {
            tempdir,
            state,
            session_store,
        }
    }
}
