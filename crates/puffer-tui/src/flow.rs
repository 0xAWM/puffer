use anyhow::Result;
use puffer_config::{save_user_config, ConfigPaths};
use puffer_core::{
    dispatch_command, execute_user_turn, reload_runtime_resources, run_resource_hooks,
    supported_commands, AppState, MessageRole, ToolInvocation,
};
use puffer_provider_registry::{AuthStore, ProviderRegistry};
use puffer_resources::LoadedResources;
use puffer_session_store::{SessionStore, TranscriptEvent};
use puffer_tools::{ToolInput, ToolRegistry};
use std::io;
use std::path::Path;
use std::process::Command;

use crate::onboarding;
use crate::{OverlayState, TuiState};

/// Opens a TUI overlay for slash commands that map to picker UI.
pub(crate) fn try_open_overlay(
    state: &AppState,
    providers: &mut ProviderRegistry,
    auth_store: &AuthStore,
    session_store: &SessionStore,
    tui: &mut TuiState,
    submitted: &str,
) -> Result<bool> {
    if let Some(overlay) =
        onboarding::overlay_from_command(state, providers, auth_store, session_store, submitted)?
    {
        set_overlay_state(tui, Some(overlay));
        return Ok(true);
    }
    Ok(false)
}

/// Replaces the active overlay and clears the overlay query buffer.
pub(crate) fn set_overlay_state(tui: &mut TuiState, overlay: Option<OverlayState>) {
    tui.overlay = overlay;
    tui.input.clear();
    tui.cursor = 0;
    tui.slash_selection = 0;
}

/// Submits prompt/auth/shell input from the TUI prompt.
pub(crate) fn handle_submit(
    state: &mut AppState,
    resources: &mut LoadedResources,
    providers: &mut ProviderRegistry,
    auth_store: &mut AuthStore,
    auth_path: &Path,
    session_store: &SessionStore,
    submitted: String,
    no_alt_screen: bool,
) -> Result<()> {
    let submitted = submitted.trim().to_string();
    if submitted.is_empty() {
        return Ok(());
    }

    if handle_auth_command(
        state,
        auth_store,
        auth_path,
        session_store,
        &submitted,
        no_alt_screen,
    )? {
        return Ok(());
    }

    if submitted.starts_with('/') {
        let previous_auth_store = auth_store.clone();
        dispatch_command(
            state,
            &supported_commands(),
            resources,
            providers,
            auth_store,
            session_store,
            &submitted,
        )?;
        if *auth_store != previous_auth_store {
            auth_store.save(auth_path)?;
        }
        maybe_apply_requested_reload(state, resources, providers, auth_store, session_store)?;
        return Ok(());
    }

    if let Some(shell_command) = parse_shell_shortcut(&submitted) {
        execute_shell_shortcut(state, resources, session_store, shell_command)?;
        return Ok(());
    }

    state.push_message(MessageRole::User, submitted.clone());
    session_store.append_event(
        state.session.id,
        TranscriptEvent::UserMessage {
            text: submitted.clone(),
        },
    )?;

    let previous_auth_store = auth_store.clone();
    match execute_user_turn(state, resources, providers, auth_store, &submitted) {
        Ok(turn) => {
            append_tool_messages(state, session_store, &turn.tool_invocations)?;
            state.push_message(MessageRole::Assistant, turn.assistant_text.clone());
            session_store.append_event(
                state.session.id,
                TranscriptEvent::AssistantMessage {
                    text: turn.assistant_text,
                },
            )?;
        }
        Err(error) => {
            let message = format!("Provider request failed: {error}");
            state.push_message(MessageRole::System, message.clone());
            session_store.append_event(
                state.session.id,
                TranscriptEvent::SystemMessage { text: message },
            )?;
        }
    }
    if *auth_store != previous_auth_store {
        auth_store.save(auth_path)?;
    }

    Ok(())
}

/// Persists the selected provider and clears any selected model until the user chooses one.
pub(crate) fn apply_selected_provider(state: &mut AppState, provider_id: &str) -> Result<()> {
    state.current_provider = Some(provider_id.to_string());
    state.current_model = None;
    state.config.default_provider = Some(provider_id.to_string());
    state.config.default_model = None;
    persist_user_config(state)
}

/// Persists the current user config to `~/.puffer/config.toml`.
pub(crate) fn persist_user_config(state: &AppState) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    save_user_config(&paths, &state.config)
}

/// Returns the builtin OpenAI base URL from loaded provider resources.
pub(crate) fn builtin_openai_base_url(resources: &LoadedResources) -> Option<String> {
    resources
        .providers
        .iter()
        .find(|provider| provider.value.id == "openai")
        .map(|provider| provider.value.base_url.clone())
}

/// Returns builtin OpenAI headers from loaded provider resources.
pub(crate) fn builtin_openai_headers(
    resources: &LoadedResources,
) -> indexmap::IndexMap<String, String> {
    resources
        .providers
        .iter()
        .find(|provider| provider.value.id == "openai")
        .map(|provider| provider.value.headers.clone())
        .unwrap_or_default()
}

/// Returns builtin OpenAI query params from loaded provider resources.
pub(crate) fn builtin_openai_query_params(
    resources: &LoadedResources,
) -> indexmap::IndexMap<String, String> {
    resources
        .providers
        .iter()
        .find(|provider| provider.value.id == "openai")
        .map(|provider| provider.value.query_params.clone())
        .unwrap_or_default()
}

/// Re-enters onboarding when needed or submits any queued prompt once setup is complete.
pub(crate) fn submit_queued_prompt_if_ready(
    state: &mut AppState,
    resources: &mut LoadedResources,
    providers: &mut ProviderRegistry,
    auth_store: &mut AuthStore,
    auth_path: &Path,
    session_store: &SessionStore,
    tui: &mut TuiState,
    no_alt_screen: bool,
) -> Result<()> {
    if tui
        .deferred_prompt
        .as_deref()
        .map(str::trim)
        .is_some_and(|prompt| prompt == "/help" || prompt == "/?")
    {
        if let Some(prompt) = tui.take_deferred_prompt() {
            handle_submit(
                state,
                resources,
                providers,
                auth_store,
                auth_path,
                session_store,
                prompt,
                no_alt_screen,
            )?;
        }
        return Ok(());
    }
    if tui.overlay.is_some() {
        return Ok(());
    }
    if let Some(overlay) = onboarding::initial_overlay(state, providers, auth_store)? {
        tui.overlay = Some(overlay);
        return Ok(());
    }
    if let Some(prompt) = tui.take_deferred_prompt() {
        handle_submit(
            state,
            resources,
            providers,
            auth_store,
            auth_path,
            session_store,
            prompt,
            no_alt_screen,
        )?;
    }
    Ok(())
}

/// Reloads runtime resources when commands request it and emits the reload summary.
pub(crate) fn maybe_apply_requested_reload(
    state: &mut AppState,
    resources: &mut LoadedResources,
    providers: &mut ProviderRegistry,
    auth_store: &AuthStore,
    session_store: &SessionStore,
) -> Result<()> {
    if !state.reload_resources_requested {
        return Ok(());
    }
    state.reload_resources_requested = false;
    let summary = reload_runtime_resources(state, resources, providers, auth_store)?;
    emit_system_message(state, session_store, summary)
}

/// Appends one system message to the in-memory transcript and persisted session log.
pub(crate) fn emit_system_message(
    state: &mut AppState,
    session_store: &SessionStore,
    text: String,
) -> Result<()> {
    state.push_message(MessageRole::System, text.clone());
    session_store.append_event(state.session.id, TranscriptEvent::SystemMessage { text })?;
    Ok(())
}

/// Handles embedded login/logout commands from the TUI.
pub(crate) fn handle_auth_command(
    state: &mut AppState,
    auth_store: &mut AuthStore,
    auth_path: &Path,
    session_store: &SessionStore,
    submitted: &str,
    no_alt_screen: bool,
) -> Result<bool> {
    let without_slash = submitted.trim_start_matches('/');
    let (name, args) = without_slash
        .split_once(' ')
        .map(|(name, args)| (name, args.trim()))
        .unwrap_or((without_slash, ""));
    if name == "login" {
        let provider = if args.is_empty() {
            state.current_provider.as_deref().unwrap_or("anthropic")
        } else {
            args
        };
        run_embedded_auth_login(provider, auth_store, auth_path, no_alt_screen)?;
        let message = format!("Completed login flow for {provider}.");
        state.push_message(MessageRole::System, message.clone());
        session_store.append_event(
            state.session.id,
            TranscriptEvent::SystemMessage { text: message },
        )?;
        return Ok(true);
    }

    if name != "logout" {
        return Ok(false);
    }

    let provider = if args.is_empty() {
        state.current_provider.as_deref().unwrap_or("anthropic")
    } else {
        args
    }
    .to_string();
    let removed = auth_store.remove(&provider);
    let cleared_active_provider = active_selection_uses_provider(state, provider.as_str());
    if cleared_active_provider {
        state.current_provider = None;
        state.current_model = None;
        state.config.default_provider = None;
        state.config.default_model = None;
        persist_user_config(state)?;
    }
    let message = if removed.is_some() {
        auth_store.save(auth_path)?;
        if cleared_active_provider {
            format!("Removed stored credentials for {provider} and cleared the active selection.")
        } else {
            format!("Removed stored credentials for {provider}.")
        }
    } else if cleared_active_provider {
        format!("No stored credentials exist for {provider}; cleared the active selection.")
    } else {
        format!("No stored credentials exist for {provider}.")
    };
    state.push_message(MessageRole::System, message.clone());
    session_store.append_event(
        state.session.id,
        TranscriptEvent::SystemMessage { text: message },
    )?;
    Ok(true)
}

/// Returns true when the active selection belongs to the provider being logged out.
pub(crate) fn active_selection_uses_provider(state: &AppState, provider_id: &str) -> bool {
    if state.current_provider.as_deref() == Some(provider_id) {
        return true;
    }
    state.current_model
        .as_deref()
        .and_then(|selector| selector.split_once('/'))
        .map(|(provider, _)| provider == provider_id)
        .unwrap_or(false)
}

pub(crate) fn run_embedded_auth_login(
    provider: &str,
    auth_store: &mut AuthStore,
    auth_path: &Path,
    no_alt_screen: bool,
) -> Result<()> {
    if !no_alt_screen {
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
    }

    let status = Command::new(std::env::current_exe()?)
        .arg("auth")
        .arg("login")
        .arg(provider)
        .status()?;

    if !no_alt_screen {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
    }

    if !status.success() {
        anyhow::bail!("login flow for {provider} exited with {}", status);
    }

    *auth_store = AuthStore::load(auth_path)?;
    Ok(())
}

/// Records tool invocations into transcript/task/session state.
pub(crate) fn append_tool_messages(
    state: &mut AppState,
    session_store: &SessionStore,
    invocations: &[ToolInvocation],
) -> Result<()> {
    for invocation in invocations {
        state.record_task(
            invocation.tool_id.clone(),
            invocation.input.clone(),
            invocation.success,
        );
        let rendered = render_tool_invocation(invocation);
        state.push_message(MessageRole::System, rendered.clone());
        session_store.append_event(
            state.session.id,
            TranscriptEvent::SystemMessage { text: rendered },
        )?;
    }
    Ok(())
}

fn render_tool_invocation(invocation: &ToolInvocation) -> String {
    let status = if invocation.success { "ok" } else { "error" };
    let output = invocation.output.trim();
    if output.is_empty() {
        format!(
            "Tool {} [{}]\ninput: {}",
            invocation.tool_id, status, invocation.input
        )
    } else {
        format!(
            "Tool {} [{}]\ninput: {}\n{}",
            invocation.tool_id, status, invocation.input, output
        )
    }
}

/// Executes a `!cmd` shell shortcut and records the result into the transcript.
pub(crate) fn execute_shell_shortcut(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    shell_command: &str,
) -> Result<()> {
    let rendered_command = format!("!{shell_command}");
    state.push_message(MessageRole::User, rendered_command.clone());
    session_store.append_event(
        state.session.id,
        TranscriptEvent::UserMessage {
            text: rendered_command,
        },
    )?;

    let registry = ToolRegistry::from_resources(resources);
    let result = registry.execute(
        "bash",
        &state.cwd,
        ToolInput::Bash {
            command: shell_command.to_string(),
            timeout: None,
            run_in_background: false,
            dangerously_disable_sandbox: false,
        },
    )?;
    state.record_task("bash", shell_command.to_string(), result.success);
    run_resource_hooks(
        resources,
        &state.cwd,
        "tool_end",
        &[
            ("PUFFER_TOOL_ID", "bash".to_string()),
            (
                "PUFFER_TOOL_INPUT",
                format!("{{\"command\":\"{}\"}}", shell_command.replace('"', "\\\"")),
            ),
            (
                "PUFFER_TOOL_SUCCESS",
                if result.success { "true" } else { "false" }.to_string(),
            ),
            ("PUFFER_TOOL_STDOUT", result.output.stdout.clone()),
            ("PUFFER_TOOL_STDERR", result.output.stderr.clone()),
        ],
    );

    let reply = if result.output.stderr.is_empty() {
        result.output.stdout
    } else if result.output.stdout.is_empty() {
        result.output.stderr
    } else {
        format!("{}\n{}", result.output.stdout, result.output.stderr)
    };
    let role = if result.success {
        MessageRole::Assistant
    } else {
        MessageRole::System
    };
    state.push_message(role, reply.clone());
    session_store.append_event(
        state.session.id,
        TranscriptEvent::AssistantMessage { text: reply },
    )?;
    Ok(())
}

/// Parses the `!cmd` shell shortcut form used by Claude/Codex-style CLIs.
pub(crate) fn parse_shell_shortcut(input: &str) -> Option<&str> {
    let command = input
        .strip_prefix("!!")
        .or_else(|| input.strip_prefix('!'))?;
    let trimmed = command.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Returns true for slash commands that should bypass startup onboarding.
pub(crate) fn allow_prompt_before_onboarding(prompt: &str) -> bool {
    matches!(
        prompt.trim(),
        "/help" | "/theme" | "/doctor" | "/status" | "/usage"
    )
}
