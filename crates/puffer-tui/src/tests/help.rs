use super::*;
use puffer_session_store::{TranscriptEvent, TranscriptRewrite};

#[test]
fn esc_closes_help_pane_by_rewinding_help_message() {
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let session = session_store
        .create_session(tempdir.path().to_path_buf())
        .unwrap();
    let mut state = AppState::new(
        PufferConfig::default(),
        tempdir.path().to_path_buf(),
        session,
    );
    state.push_message(
        MessageRole::System,
        "Supported commands:\n/help Show help and available commands".to_string(),
    );
    let mut resources = sample_resources();
    let mut providers = sample_providers();
    let mut auth_store = sample_auth_store();
    let auth_path = paths.user_config_dir.join("auth.json");
    let commands = supported_commands();
    let mut tui = TuiState::default();

    handle_key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &mut state,
        &mut resources,
        &mut providers,
        &mut auth_store,
        &auth_path,
        &session_store,
        &commands,
        &mut tui,
        true,
    )
    .unwrap();

    assert!(state.transcript.is_empty());
    let record = session_store.load_session(state.session.id).unwrap();
    assert!(matches!(
        record.events.last(),
        Some(TranscriptEvent::TranscriptRewritten {
            rewrite: TranscriptRewrite::PopLast { count: 1 }
        })
    ));
}
