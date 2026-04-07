use super::*;
use puffer_session_store::TranscriptEvent;

#[test]
fn esc_closes_help_overlay_without_rewriting_transcript() {
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
    let mut resources = sample_resources();
    let mut providers = sample_providers();
    let mut auth_store = sample_auth_store();
    let auth_path = paths.user_config_dir.join("auth.json");
    let commands = supported_commands();
    let mut tui = TuiState {
        overlay: Some(OverlayState::Help),
        ..TuiState::default()
    };

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
    assert!(tui.overlay.is_none());
    let record = session_store.load_session(state.session.id).unwrap();
    assert!(!matches!(
        record.events.last(),
        Some(TranscriptEvent::TranscriptRewritten { .. })
    ));
}

#[test]
fn help_alias_question_mark_opens_help_overlay() {
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
    let resources = sample_resources();
    let mut providers = sample_providers();
    let auth_store = sample_auth_store();
    let mut tui = TuiState::default();

    assert!(try_open_overlay(
        &state,
        &resources,
        &mut providers,
        &auth_store,
        &session_store,
        &mut tui,
        "/?",
    )
    .unwrap());
    assert!(matches!(tui.overlay, Some(OverlayState::Help)));
}
