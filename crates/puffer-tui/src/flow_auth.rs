use crate::flow::persist_user_config;
use anyhow::Result;
use puffer_core::{AppState, MessageRole};
use puffer_provider_registry::AuthStore;
use puffer_session_store::{SessionStore, TranscriptEvent};
use std::io;
use std::path::Path;
use std::process::Command;

/// Handles embedded login and logout commands from the TUI.
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
        if args.is_empty() {
            return Ok(false);
        }
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
    state
        .current_model
        .as_deref()
        .and_then(|selector| selector.split_once('/'))
        .map(|(provider, _)| provider == provider_id)
        .unwrap_or(false)
}

/// Runs the external `puffer auth login` flow and reloads stored credentials.
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
