use crate::{run_command_capture, CommandOutput};
use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Describes tmux availability and optional version metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxInfo {
    pub available: bool,
    pub version: Option<String>,
}

/// Owns one temporary tmux session created for tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxSession {
    pub name: String,
}

/// Returns true when tmux appears to be installed and runnable.
pub fn tmux_available() -> bool {
    detect_tmux().available
}

/// Probes the local system for tmux and captures its version when available.
pub fn detect_tmux() -> TmuxInfo {
    match run_command_capture("tmux", &["-V"], None) {
        Ok(output) if output.status_code == 0 => TmuxInfo {
            available: true,
            version: Some(output.stdout.trim().to_string()),
        },
        _ => TmuxInfo {
            available: false,
            version: None,
        },
    }
}

/// Starts a detached tmux session that runs the provided program and arguments.
pub fn start_tmux_command(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<TmuxSession> {
    let session = TmuxSession {
        name: format!("puffer-{}", Uuid::new_v4().simple()),
    };
    let mut command = Command::new("tmux");
    command
        .arg("new-session")
        .arg("-d")
        .arg("-s")
        .arg(&session.name);
    if let Some(cwd) = cwd {
        command.arg("-c").arg(cwd);
    }
    command.arg(shell_command(program, args));
    let status = command.status()?;
    if !status.success() {
        return Err(anyhow!("failed to start tmux session {}", session.name));
    }
    let output = run_tmux_command(&["set-option", "-t", &session.name, "remain-on-exit", "on"])?;
    if output.status_code != 0 {
        return Err(anyhow!(
            "failed to set remain-on-exit for {}: {}",
            session.name,
            output.stderr.trim()
        ));
    }
    Ok(session)
}

/// Captures the current pane contents for a tmux session.
pub fn capture_tmux_pane(session: &TmuxSession) -> Result<String> {
    let target = format!("{}:0.0", session.name);
    let output = run_tmux_command(&["capture-pane", "-p", "-S", "-", "-t", &target])?;
    if output.status_code != 0 {
        return Err(anyhow!(
            "failed to capture tmux pane for {}: {}",
            session.name,
            output.stderr.trim()
        ));
    }
    Ok(output.stdout)
}

/// Sends tmux key presses to the target session pane.
pub fn send_tmux_keys(session: &TmuxSession, keys: &[&str]) -> Result<()> {
    let mut args = vec!["send-keys", "-t", session.name.as_str()];
    args.extend(keys.iter().copied());
    let output = run_tmux_command(&args)?;
    if output.status_code != 0 {
        return Err(anyhow!(
            "failed to send tmux keys to {}: {}",
            session.name,
            output.stderr.trim()
        ));
    }
    Ok(())
}

/// Waits until the tmux pane contains the requested text or the timeout expires.
pub fn wait_for_tmux_text(
    session: &TmuxSession,
    needle: &str,
    timeout: Duration,
) -> Result<String> {
    let deadline = Instant::now() + timeout;
    loop {
        let capture = capture_tmux_pane(session)?;
        if capture.contains(needle) {
            return Ok(capture);
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "timed out waiting for `{needle}` in tmux session {}",
                session.name
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Kills a tmux session created for testing.
pub fn kill_tmux_session(session: &TmuxSession) -> Result<()> {
    let output = run_tmux_command(&["kill-session", "-t", &session.name])?;
    if output.status_code != 0 {
        return Err(anyhow!(
            "failed to kill tmux session {}: {}",
            session.name,
            output.stderr.trim()
        ));
    }
    Ok(())
}

impl Drop for TmuxSession {
    fn drop(&mut self) {
        let _ = run_tmux_command(&["kill-session", "-t", &self.name]);
    }
}

fn run_tmux_command(args: &[&str]) -> Result<CommandOutput> {
    run_command_capture("tmux", args, None)
}

fn shell_command(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .map(shell_escape)
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_tmux_never_panics() {
        let _ = detect_tmux();
    }

    #[test]
    fn tmux_command_round_trips_output_when_available() {
        if !tmux_available() {
            return;
        }
        let session =
            start_tmux_command("sh", &["-lc", "printf 'hello from tmux'; sleep 2"], None).unwrap();
        let capture =
            wait_for_tmux_text(&session, "hello from tmux", Duration::from_secs(3)).unwrap();
        assert!(capture.contains("hello from tmux"));
    }
}
