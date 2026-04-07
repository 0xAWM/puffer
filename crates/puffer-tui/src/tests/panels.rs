use super::*;

fn open_panel(command: &str) -> OverlayState {
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();

    let mut state = sample_state();
    state.cwd = tempdir.path().to_path_buf();
    state.session.cwd = tempdir.path().to_path_buf();
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
        command,
    )
    .unwrap();
    assert!(opened);
    tui.overlay.expect("panel overlay")
}

#[test]
fn try_open_overlay_builds_config_panel() {
    assert!(matches!(open_panel("/config"), OverlayState::Text(..)));
}

#[test]
fn try_open_overlay_builds_permissions_panel() {
    assert!(matches!(open_panel("/permissions"), OverlayState::Text(..)));
}

#[test]
fn try_open_overlay_builds_hooks_panel() {
    assert!(matches!(open_panel("/hooks"), OverlayState::Text(..)));
}

#[test]
fn try_open_overlay_builds_mcp_panel() {
    assert!(matches!(
        open_panel("/mcp"),
        OverlayState::CommandPicker { .. }
    ));
}

#[test]
fn try_open_overlay_builds_plugin_panel() {
    assert!(matches!(
        open_panel("/plugin"),
        OverlayState::CommandPicker { .. }
    ));
}

#[test]
fn try_open_overlay_builds_memory_panel() {
    assert!(matches!(
        open_panel("/memory"),
        OverlayState::CommandPicker { .. }
    ));
}

#[test]
fn try_open_overlay_builds_session_panel() {
    assert!(matches!(open_panel("/session"), OverlayState::Session(..)));
}

#[test]
fn try_open_overlay_builds_rewind_picker() {
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();

    let mut state = sample_state();
    state.cwd = tempdir.path().to_path_buf();
    state.session.cwd = tempdir.path().to_path_buf();
    state.push_message(MessageRole::User, "first");
    state.push_message(MessageRole::Assistant, "reply");
    state.push_message(MessageRole::User, "second");
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
        "/rewind",
    )
    .unwrap();

    assert!(opened);
    assert!(matches!(
        tui.overlay,
        Some(OverlayState::CommandPicker { .. })
    ));
}
