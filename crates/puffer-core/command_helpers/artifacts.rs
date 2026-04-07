use super::emit_system;
use crate::{AppState, MessageRole, RenderedMessage};
use anyhow::{Context, Result};
use arboard::Clipboard;
use puffer_session_store::SessionStore;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

const COPY_RESPONSE_FILENAME: &str = "response.md";

/// Describes the selected assistant message for `/copy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CopySelection {
    pub(crate) text: String,
    pub(crate) age: usize,
    pub(crate) total: usize,
}

/// Handles `/copy`, including Claude-style `/copy N` history selection.
pub(crate) fn handle_copy_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let selection = match select_copy_target(&state.transcript, args) {
        Ok(selection) => selection,
        Err(error) => return emit_system(state, session_store, error.to_string()),
    };
    let fallback_path = write_temp_artifact(&selection.text, COPY_RESPONSE_FILENAME).ok();
    let summary = clipboard_summary(
        &selection.text,
        "assistant response",
        selection.age,
        selection.total,
        fallback_path.as_deref(),
    );
    emit_system(state, session_store, summary)
}

/// Handles `/export` by rendering a plain-text conversation transcript.
pub(crate) fn handle_export_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let content = render_export_transcript(state);
    let trimmed = args.trim();
    if trimmed.eq_ignore_ascii_case("clipboard") {
        let fallback_path = write_temp_artifact(&content, &default_export_filename(state)).ok();
        let summary = clipboard_summary(
            &content,
            "conversation export",
            0,
            1,
            fallback_path.as_deref(),
        );
        return emit_system(state, session_store, summary);
    }

    let target = export_target_path(state, trimmed);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&target, &content)
        .with_context(|| format!("failed to write {}", target.display()))?;

    let clipboard_message = match try_copy_to_clipboard(&content) {
        Ok(()) => " Also copied to clipboard.",
        Err(_) => "",
    };
    emit_system(
        state,
        session_store,
        format!(
            "Conversation exported to {}.{}",
            target.display(),
            clipboard_message
        ),
    )
}

/// Selects the assistant message to copy for `/copy [N]`.
pub(crate) fn select_copy_target(
    transcript: &[RenderedMessage],
    args: &str,
) -> Result<CopySelection> {
    let recent = recent_assistant_texts(transcript);
    if recent.is_empty() {
        anyhow::bail!("No assistant message is available to copy.");
    }

    let trimmed = args.trim();
    let age = if trimmed.is_empty() {
        0
    } else {
        let n = trimmed.parse::<usize>().with_context(|| {
            format!("Usage: /copy [N] where N is 1 (latest), 2, 3, ... Got: {trimmed}")
        })?;
        if n == 0 {
            anyhow::bail!("Usage: /copy [N] where N is 1 (latest), 2, 3, ... Got: {trimmed}");
        }
        if n > recent.len() {
            anyhow::bail!(
                "Only {} assistant {} available to copy.",
                recent.len(),
                if recent.len() == 1 {
                    "message"
                } else {
                    "messages"
                }
            );
        }
        n - 1
    };

    Ok(CopySelection {
        text: recent[age].clone(),
        age,
        total: recent.len(),
    })
}

/// Renders the current transcript as a plain-text conversation export.
pub(crate) fn render_export_transcript(state: &AppState) -> String {
    let mut text = String::new();
    let _ = writeln!(&mut text, "Puffer Code Conversation Export");
    let _ = writeln!(&mut text, "session_id={}", state.session.id);
    let _ = writeln!(
        &mut text,
        "display_name={}",
        state.session.display_name.as_deref().unwrap_or("<unnamed>")
    );
    let _ = writeln!(&mut text, "cwd={}", state.cwd.display());
    let _ = writeln!(
        &mut text,
        "provider={}",
        state.current_provider.as_deref().unwrap_or("<unset>")
    );
    let _ = writeln!(
        &mut text,
        "model={}",
        state.current_model.as_deref().unwrap_or("<unset>")
    );
    let _ = writeln!(&mut text, "exported_at={}", export_timestamp());

    for message in &state.transcript {
        let _ = writeln!(&mut text, "\n## {}", role_label(&message.role));
        let _ = writeln!(&mut text, "{}", message.text.trim_end());
    }

    text.trim_end().to_string()
}

fn recent_assistant_texts(transcript: &[RenderedMessage]) -> Vec<String> {
    transcript
        .iter()
        .rev()
        .filter(|message| message.role == MessageRole::Assistant)
        .map(|message| message.text.trim().to_string())
        .filter(|text| !text.is_empty())
        .take(20)
        .collect()
}

fn clipboard_summary(
    text: &str,
    label: &str,
    age: usize,
    total: usize,
    fallback_path: Option<&Path>,
) -> String {
    let age_label = if total > 1 {
        format!(" {} of {}", age + 1, total)
    } else {
        String::new()
    };
    match try_copy_to_clipboard(text) {
        Ok(()) => {
            let mut message = format!(
                "Copied {label}{age_label} to clipboard ({} characters, {} lines).",
                text.len(),
                line_count(text)
            );
            if let Some(path) = fallback_path {
                let _ = write!(&mut message, "\nAlso written to {}.", path.display());
            }
            message
        }
        Err(_) => {
            if let Some(path) = fallback_path {
                format!(
                    "Clipboard copy unavailable. Wrote {label}{age_label} to {}.",
                    path.display()
                )
            } else {
                format!("{label}{age_label}:\n{text}")
            }
        }
    }
}

fn try_copy_to_clipboard(text: &str) -> Result<()> {
    Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text.to_string()))
        .context("clipboard unavailable")
}

fn write_temp_artifact(text: &str, filename: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join("puffer");
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let path = dir.join(filename);
    fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn export_target_path(state: &AppState, args: &str) -> PathBuf {
    let filename = if args.is_empty() {
        default_export_filename(state)
    } else {
        args.to_string()
    };
    let mut path = PathBuf::from(filename);
    if !path.is_absolute() {
        path = state.cwd.join(path);
    }
    if path.extension().and_then(|value| value.to_str()) != Some("txt") {
        path.set_extension("txt");
    }
    path
}

fn default_export_filename(state: &AppState) -> String {
    let timestamp = export_timestamp();
    let prompt = first_prompt_text(&state.transcript);
    if prompt.is_empty() {
        return format!("conversation-{timestamp}.txt");
    }
    let sanitized = sanitize_filename(&prompt);
    if sanitized.is_empty() {
        format!("conversation-{timestamp}.txt")
    } else {
        format!("{timestamp}-{sanitized}.txt")
    }
}

fn first_prompt_text(transcript: &[RenderedMessage]) -> String {
    let Some(message) = transcript
        .iter()
        .find(|message| message.role == MessageRole::User)
    else {
        return String::new();
    };
    let mut text = message
        .text
        .trim()
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    if text.chars().count() > 50 {
        text = text.chars().take(49).collect::<String>() + "...";
    }
    text
}

fn sanitize_filename(text: &str) -> String {
    let mut sanitized = String::new();
    let mut last_dash = false;
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            sanitized.push(ch);
            last_dash = false;
        } else if (ch.is_ascii_whitespace() || ch == '-') && !last_dash && !sanitized.is_empty() {
            sanitized.push('-');
            last_dash = true;
        }
    }
    sanitized.trim_matches('-').to_string()
}

fn export_timestamp() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    format!(
        "{:04}-{:02}-{:02}-{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

fn role_label(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::User => "User",
        MessageRole::Assistant => "Assistant",
        MessageRole::System => "System",
    }
}

fn line_count(text: &str) -> usize {
    text.chars().filter(|ch| *ch == '\n').count() + 1
}
