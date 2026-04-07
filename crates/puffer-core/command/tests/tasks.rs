use super::*;

#[test]
fn tasks_command_reports_recorded_runtime_tasks() {
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
    state.record_task("bash", "printf hi", true);

    dispatch_command(
        &mut state,
        &supported_commands(),
        &LoadedResources::default(),
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/tasks",
    )
    .unwrap();

    assert!(matches!(
        state.transcript.last(),
        Some(RenderedMessage {
            role: MessageRole::System,
            text,
        }) if text.contains("bash") && text.contains("completed")
    ));
}

#[test]
fn tasks_command_reports_workflow_tasks_and_todos() {
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

    let cwd = state.cwd.clone();
    crate::runtime::claude_tools::workflow::task_create::execute_task_create(
        &mut state,
        &cwd,
        serde_json::json!({
            "subject": "Audit slash command parity",
            "description": "Check missing task surfaces"
        }),
    )
    .unwrap();
    crate::runtime::claude_tools::workflow::todo_write::execute_todo_write(
        &mut state,
        &cwd,
        serde_json::json!({
            "todos": [
                {
                    "content": "Wire /tasks to workflow state",
                    "status": "in_progress",
                    "activeForm": "Wiring /tasks to workflow state"
                }
            ]
        }),
    )
    .unwrap();

    dispatch_command(
        &mut state,
        &supported_commands(),
        &LoadedResources::default(),
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/tasks",
    )
    .unwrap();

    assert!(matches!(
        state.transcript.last(),
        Some(RenderedMessage {
            role: MessageRole::System,
            text,
        }) if text.contains("Task list:")
            && text.contains("Audit slash command parity")
            && text.contains("Todos:")
            && text.contains("Wire /tasks to workflow state")
    ));
}
