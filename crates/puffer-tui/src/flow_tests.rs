use super::*;
use puffer_config::{ensure_workspace_dirs, ConfigPaths, PufferConfig};
use puffer_session_store::SessionMetadata;
use tempfile::tempdir;

fn sample_state(session: SessionMetadata, cwd: &Path) -> AppState {
    AppState::new(PufferConfig::default(), cwd.to_path_buf(), session)
}

#[test]
fn provider_prompt_detection_matches_interactive_surface() {
    assert!(is_provider_prompt_input("henlo"));
    assert!(is_provider_prompt_input(" review this diff "));
    assert!(!is_provider_prompt_input(""));
    assert!(!is_provider_prompt_input("/help"));
    assert!(!is_provider_prompt_input("!pwd"));
    assert!(!is_provider_prompt_input("login openai"));
    assert!(!is_provider_prompt_input("/logout"));
}

#[test]
fn handle_prompt_submit_starts_async_provider_turn_and_polls_result() {
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let session = session_store
        .create_session(tempdir.path().to_path_buf())
        .unwrap();
    let mut state = sample_state(session, tempdir.path());
    let mut resources = LoadedResources::default();
    let mut providers = ProviderRegistry::new();
    let auth_path = paths.user_config_dir.join("auth.json");
    let mut auth_store = AuthStore::default();
    let mut tui = TuiState::default();

    handle_prompt_submit(
        &mut state,
        &mut resources,
        &mut providers,
        &mut auth_store,
        &auth_path,
        &session_store,
        &mut tui,
        "henlo".to_string(),
        true,
    )
    .unwrap();

    assert!(tui.has_pending_submit());
    assert!(matches!(state.transcript.first(), Some(message) if message.text == "henlo"));

    let mut completed = false;
    for _ in 0..20 {
        if poll_pending_submit(
            &mut state,
            &mut auth_store,
            &auth_path,
            &session_store,
            &mut tui,
        )
        .unwrap()
        {
            completed = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert!(completed);
    assert!(!tui.has_pending_submit());
    assert!(state.transcript.iter().any(|message| {
        message.role == MessageRole::System && message.text.starts_with("Provider request failed:")
    }));
}

#[test]
fn handle_prompt_submit_queues_prompt_while_turn_is_running() {
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let session = session_store
        .create_session(tempdir.path().to_path_buf())
        .unwrap();
    let mut state = sample_state(session, tempdir.path());
    let mut resources = LoadedResources::default();
    let mut providers = ProviderRegistry::new();
    let auth_path = paths.user_config_dir.join("auth.json");
    let mut auth_store = AuthStore::default();
    let mut tui = TuiState::default();

    handle_prompt_submit(
        &mut state,
        &mut resources,
        &mut providers,
        &mut auth_store,
        &auth_path,
        &session_store,
        &mut tui,
        "first".to_string(),
        true,
    )
    .unwrap();
    handle_prompt_submit(
        &mut state,
        &mut resources,
        &mut providers,
        &mut auth_store,
        &auth_path,
        &session_store,
        &mut tui,
        "second".to_string(),
        true,
    )
    .unwrap();

    assert!(tui.has_pending_submit());
    assert_eq!(tui.queued_prompts.len(), 1);
    assert_eq!(
        tui.queued_prompts.front().map(String::as_str),
        Some("second")
    );
    assert!(matches!(state.transcript.first(), Some(message) if message.text == "first"));
}

#[test]
fn cancel_pending_submit_records_interrupt_and_starts_next_queued_prompt() {
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let session = session_store
        .create_session(tempdir.path().to_path_buf())
        .unwrap();
    let mut state = sample_state(session, tempdir.path());
    let mut resources = LoadedResources::default();
    let mut providers = ProviderRegistry::new();
    let auth_path = paths.user_config_dir.join("auth.json");
    let mut auth_store = AuthStore::default();
    let mut tui = TuiState::default();

    handle_prompt_submit(
        &mut state,
        &mut resources,
        &mut providers,
        &mut auth_store,
        &auth_path,
        &session_store,
        &mut tui,
        "first".to_string(),
        true,
    )
    .unwrap();
    handle_prompt_submit(
        &mut state,
        &mut resources,
        &mut providers,
        &mut auth_store,
        &auth_path,
        &session_store,
        &mut tui,
        "second".to_string(),
        true,
    )
    .unwrap();

    assert!(cancel_pending_submit(&mut state, &session_store, &mut tui).unwrap());
    assert!(!tui.has_pending_submit());
    assert!(state.transcript.iter().any(|message| {
        message.role == MessageRole::System && message.text == "Interrupted by user."
    }));

    assert!(submit_next_queued_prompt(
        &mut state,
        &mut resources,
        &mut providers,
        &mut auth_store,
        &auth_path,
        &session_store,
        &mut tui,
        true,
    )
    .unwrap());
    assert!(tui.has_pending_submit());
    assert!(tui.queued_prompts.is_empty());
    assert!(state
        .transcript
        .iter()
        .any(|message| { message.role == MessageRole::User && message.text == "second" }));
}
