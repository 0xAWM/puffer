use super::*;

#[test]
fn try_open_overlay_builds_tag_removal_picker_for_existing_tag() {
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
        "/tag review",
    )
    .unwrap();

    assert!(opened);
    match tui.overlay {
        Some(OverlayState::CommandPicker {
            title,
            entries,
            selection,
        }) => {
            assert_eq!(title, "Remove Tag?");
            assert_eq!(selection, 0);
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].selector, "Yes, remove tag");
            assert_eq!(entries[0].description, "Current tag: #review");
            assert_eq!(
                entries[0].command.as_deref(),
                Some("/tag --confirm-remove review")
            );
            assert_eq!(entries[1].selector, "No, keep tag");
            assert_eq!(entries[1].command.as_deref(), Some("/tag --keep review"));
        }
        other => panic!("expected tag removal picker, got {other:?}"),
    }
}

#[test]
fn command_picker_selected_command_prefers_explicit_command() {
    let overlay = OverlayState::CommandPicker {
        title: "Remove Tag?".to_string(),
        entries: vec![ModelPickerEntry {
            selector: "Yes, remove tag".to_string(),
            description: "Current tag: #review".to_string(),
            command: Some("/tag --confirm-remove review".to_string()),
        }],
        selection: 0,
    };

    assert_eq!(
        overlay.selected_command().as_deref(),
        Some("/tag --confirm-remove review")
    );
}
