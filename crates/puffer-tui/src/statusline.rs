use anyhow::{Context, Result};
use puffer_core::{AppState, MessageRole};
use serde_json::json;
use std::io::Write as _;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const STATUS_LINE_TIMEOUT_MS: u64 = 500;

/// Refreshes the configured status line command output when the input snapshot changes.
pub(crate) fn refresh_status_line(state: &mut AppState) -> Result<()> {
    let Some(config) = state
        .config
        .ui
        .status_line
        .as_ref()
        .filter(|config| !config.command.trim().is_empty())
    else {
        state.status_line_text = None;
        state.set_status_line_signature(None);
        return Ok(());
    };
    let command = config.command.clone();

    let input = build_status_line_input(state);
    let signature = format!("{}\0{}", command, input);
    if state.status_line_signature() == Some(signature.as_str()) {
        return Ok(());
    }

    state.set_status_line_signature(Some(signature));
    state.status_line_text =
        run_status_line_command(&state.cwd, &command, &input).unwrap_or_default();
    Ok(())
}

fn build_status_line_input(state: &AppState) -> String {
    let user_messages = state
        .transcript
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .count();
    let assistant_messages = state
        .transcript
        .iter()
        .filter(|message| message.role == MessageRole::Assistant)
        .count();
    let system_messages = state
        .transcript
        .iter()
        .filter(|message| message.role == MessageRole::System)
        .count();
    serde_json::to_string_pretty(&json!({
        "session_id": state.session.id,
        "session_name": state.session.display_name,
        "cwd": state.cwd,
        "provider": state.current_provider,
        "model": {
            "id": state.current_model,
            "display_name": model_display_name(state.current_model.as_deref()),
        },
        "workspace": {
            "current_dir": state.cwd,
            "project_dir": state.session.cwd,
            "added_dirs": state.working_dirs,
        },
        "app": {
            "version": env!("CARGO_PKG_VERSION"),
        },
        "ui": {
            "theme": state.config.theme,
            "fast_mode": state.fast_mode,
            "plan_mode": state.plan_mode,
            "sandbox_mode": state.sandbox_mode,
            "vim_mode": state.vim_mode,
        },
        "transcript": {
            "message_count": state.transcript.len(),
            "user_messages": user_messages,
            "assistant_messages": assistant_messages,
            "system_messages": system_messages,
        },
        "remote": {
            "name": state.remote_name,
            "environment": state.remote_environment,
            "session_id": state.remote_session_id,
            "status": state.remote_session_status,
            "url": state.remote_session_url,
        }
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

fn model_display_name(model_id: Option<&str>) -> String {
    model_id
        .and_then(|model_id| model_id.rsplit('/').next())
        .unwrap_or("<unset>")
        .to_string()
}

fn run_status_line_command(
    cwd: &std::path::Path,
    command: &str,
    input: &str,
) -> Result<Option<String>> {
    let mut child = Command::new("bash")
        .arg("-lc")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to start status line command in {}", cwd.display()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }
    let output = wait_for_output(child, STATUS_LINE_TIMEOUT_MS)?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let rendered = stdout
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if rendered.is_empty() {
        Ok(None)
    } else {
        Ok(Some(rendered))
    }
}

fn wait_for_output(mut child: std::process::Child, timeout_ms: u64) -> Result<Output> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if child.try_wait()?.is_some() {
            return child
                .wait_with_output()
                .context("failed to read status line output");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            return child
                .wait_with_output()
                .context("failed to read timed out status line output");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::refresh_status_line;
    use puffer_config::{PufferConfig, StatusLineConfig};
    use puffer_core::AppState;
    use puffer_session_store::SessionMetadata;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn refresh_status_line_executes_configured_command() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut config = PufferConfig::default();
        config.ui.status_line = Some(StatusLineConfig {
            command: r#"cat >/dev/null; printf 'openai gpt-5'"#.to_string(),
            padding: 0,
        });
        let mut state = AppState::new(
            config,
            tempdir.path().to_path_buf(),
            SessionMetadata {
                id: Uuid::new_v4(),
                display_name: Some("dockyard".to_string()),
                cwd: tempdir.path().to_path_buf(),
                created_at_ms: 0,
                updated_at_ms: 0,
                parent_session_id: None,
                slug: None,
                tags: Vec::new(),
                note: None,
            },
        );
        state.current_provider = Some("openai".to_string());
        state.current_model = Some("openai/gpt-5".to_string());

        refresh_status_line(&mut state).unwrap();

        assert_eq!(state.status_line_text.as_deref(), Some("openai gpt-5"));
        assert!(state.status_line_signature().is_some());
    }

    #[test]
    fn refresh_status_line_clears_output_when_not_configured() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut state = AppState::new(
            PufferConfig::default(),
            tempdir.path().to_path_buf(),
            SessionMetadata {
                id: Uuid::new_v4(),
                display_name: None,
                cwd: PathBuf::from(tempdir.path()),
                created_at_ms: 0,
                updated_at_ms: 0,
                parent_session_id: None,
                slug: None,
                tags: Vec::new(),
                note: None,
            },
        );
        state.status_line_text = Some("stale".to_string());
        state.set_status_line_signature(Some("sig".to_string()));

        refresh_status_line(&mut state).unwrap();

        assert!(state.status_line_text.is_none());
        assert!(state.status_line_signature().is_none());
    }
}
