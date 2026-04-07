use super::*;

#[test]
fn copy_selection_reaches_back_to_older_assistant_messages() {
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
    state.push_message(MessageRole::Assistant, "oldest");
    state.push_message(MessageRole::Assistant, "older");
    state.push_message(MessageRole::Assistant, "latest");

    let selection =
        crate::command_helpers::artifacts::select_copy_target(&state.transcript, "2").unwrap();

    assert_eq!(selection.text, "older");
    assert_eq!(selection.age, 1);
    assert_eq!(selection.total, 3);
}

#[test]
fn copy_command_reports_invalid_history_index() {
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
    state.push_message(MessageRole::Assistant, "latest");

    dispatch_command(
        &mut state,
        &supported_commands(),
        &LoadedResources::default(),
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/copy 0",
    )
    .unwrap();

    assert!(matches!(
        state.transcript.last(),
        Some(RenderedMessage {
            role: MessageRole::System,
            text,
        }) if text.contains("Usage: /copy [N]")
    ));
}

#[test]
fn export_command_writes_plain_text_transcript_to_txt_file() {
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
    state.push_message(MessageRole::User, "Review current diff");
    state.push_message(MessageRole::Assistant, "The diff is clean.");

    dispatch_command(
        &mut state,
        &supported_commands(),
        &LoadedResources::default(),
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/export ship-notes.md",
    )
    .unwrap();

    let target = tempdir.path().join("ship-notes.txt");
    let contents = std::fs::read_to_string(&target).unwrap();
    assert!(contents.contains("Puffer Code Conversation Export"));
    assert!(contents.contains("## User"));
    assert!(contents.contains("Review current diff"));
    assert!(contents.contains("## Assistant"));
    assert!(contents.contains("The diff is clean."));
    assert!(matches!(
        state.transcript.last(),
        Some(RenderedMessage {
            role: MessageRole::System,
            text,
        }) if text.contains("Conversation exported to")
            && text.contains("ship-notes.txt")
    ));
}
