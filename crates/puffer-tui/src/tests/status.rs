use super::*;

#[test]
fn try_open_overlay_builds_status_overlay() {
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();

    let state = sample_state();
    let resources = sample_resources();
    let mut providers = sample_providers();
    let auth_store = sample_auth_store();
    let mut tui = TuiState::default();
    let opened = try_open_overlay(
        &state,
        &resources,
        &mut providers,
        &auth_store,
        &session_store,
        &mut tui,
        "/status",
    )
    .unwrap();

    assert!(opened);
    assert!(matches!(tui.overlay, Some(OverlayState::Status(..))));
}

#[test]
fn try_open_overlay_builds_usage_overlay() {
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();

    let state = sample_state();
    let resources = sample_resources();
    let mut providers = sample_providers();
    let auth_store = sample_auth_store();
    let mut tui = TuiState::default();
    let opened = try_open_overlay(
        &state,
        &resources,
        &mut providers,
        &auth_store,
        &session_store,
        &mut tui,
        "/usage",
    )
    .unwrap();

    assert!(opened);
    assert!(matches!(tui.overlay, Some(OverlayState::Usage(..))));
}
