use super::*;

#[test]
fn config_tool_supports_camel_case_aliases_and_status_line_settings() {
    let mut state = temp_state();
    let cwd = state.cwd.clone();
    let output = super::super::claude_tools::workflow::config::execute_config(
        &mut state,
        &cwd,
        json!({
            "setting": "statusLineCommand",
            "value": "echo status"
        }),
    )
    .unwrap();
    let parsed: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["persisted"], true);
    assert_eq!(parsed["value"], "echo status");
    assert_eq!(
        state
            .config
            .ui
            .status_line
            .as_ref()
            .map(|status_line| status_line.command.as_str()),
        Some("echo status")
    );
}

#[test]
fn config_tool_supports_session_only_settings_without_persisting() {
    let mut state = temp_state();
    let cwd = state.cwd.clone();
    let output = super::super::claude_tools::workflow::config::execute_config(
        &mut state,
        &cwd,
        json!({
            "setting": "fastMode",
            "value": true
        }),
    )
    .unwrap();
    let parsed: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["persisted"], false);
    assert_eq!(parsed["path"], Value::Null);
    assert!(state.fast_mode);
}

#[test]
fn config_tool_allows_null_to_clear_model_override() {
    let mut state = temp_state();
    state.current_model = Some("openai/gpt-5".to_string());
    state.current_provider = Some("openai".to_string());
    let cwd = state.cwd.clone();
    let output = super::super::claude_tools::workflow::config::execute_config(
        &mut state,
        &cwd,
        json!({
            "setting": "model",
            "value": null
        }),
    )
    .unwrap();
    let parsed: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["value"], Value::Null);
    assert_eq!(state.current_model, None);
}

#[test]
fn ask_user_question_rejects_duplicate_question_text() {
    let mut state = temp_state();
    let cwd = state.cwd.clone();
    let error = super::super::claude_tools::workflow::ask_user_question::execute_ask_user_question(
        &mut state,
        &cwd,
        json!({
            "questions": [
                {
                    "question": "Pick one",
                    "header": "choice",
                    "options": [
                        {"label": "A", "description": "A"},
                        {"label": "B", "description": "B"}
                    ]
                },
                {
                    "question": "Pick one",
                    "header": "second",
                    "options": [
                        {"label": "C", "description": "C"},
                        {"label": "D", "description": "D"}
                    ]
                }
            ]
        }),
    )
    .unwrap_err();
    assert!(error.to_string().contains("question texts must be unique"));
}

#[test]
fn team_create_makes_dirs_and_team_delete_removes_them() {
    let mut state = temp_state();
    let cwd = state.cwd.clone();
    let created = super::super::claude_tools::workflow::team_create::execute_team_create(
        &mut state,
        &cwd,
        json!({
            "team_name": "alpha",
            "description": "Coordination team"
        }),
    )
    .unwrap();
    let created: Value = serde_json::from_str(&created).unwrap();
    let team_dir = created["teamDir"].as_str().unwrap();
    let task_dir = created["taskDir"].as_str().unwrap();
    assert!(std::path::Path::new(team_dir).exists());
    assert!(std::path::Path::new(task_dir).exists());

    let deleted = super::super::claude_tools::workflow::team_delete::execute_team_delete(
        &mut state,
        &cwd,
        json!({}),
    )
    .unwrap();
    let deleted: Value = serde_json::from_str(&deleted).unwrap();
    assert_eq!(deleted["deleted"][0], "alpha");
    assert!(!std::path::Path::new(team_dir).exists());
    assert!(!std::path::Path::new(task_dir).exists());
}

#[test]
fn task_update_sets_timestamps_for_progress() {
    let mut state = temp_state();
    let cwd = state.cwd.clone();
    let created = super::super::claude_tools::workflow::task_create::execute_task_create(
        &mut state,
        &cwd,
        json!({
            "subject": "Do thing",
            "description": "Do thing"
        }),
    )
    .unwrap();
    let created: Value = serde_json::from_str(&created).unwrap();
    let task_id = created["task"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("unexpected task create output: {created}"));

    let updated = super::super::claude_tools::workflow::task_update::execute_task_update(
        &mut state,
        &cwd,
        json!({
            "taskId": task_id,
            "status": "in_progress"
        }),
    )
    .unwrap();
    let updated: Value = serde_json::from_str(&updated).unwrap();
    assert_eq!(updated["success"], true);
    assert_eq!(updated["taskId"], task_id);
    assert_eq!(updated["updatedFields"], json!(["status"]));
    assert_eq!(
        updated["statusChange"],
        json!({
            "from": "pending",
            "to": "in_progress"
        })
    );

    let tasks_path = ConfigPaths::discover(&cwd)
        .workspace_config_dir
        .join("runtime/claude_workflow/tasks.json");
    let persisted: Value = serde_json::from_str(&fs::read_to_string(tasks_path).unwrap()).unwrap();
    let task = persisted["tasks"][0].clone();
    assert_eq!(task["task_id"], task_id);
    assert_eq!(task["status"], "in_progress");
    assert!(task["started_at_ms"].is_number());
    assert!(task["updated_at_ms"].is_number());
}

#[test]
fn task_output_waits_for_agent_completion() {
    let mut state = temp_state();
    let cwd = state.cwd.clone();
    let workflow_root = cwd.join(".puffer/runtime/claude_workflow");
    fs::create_dir_all(workflow_root.join("agent_outputs")).unwrap();

    let agent_output = workflow_root.join("agent_outputs/agent-1.md");
    fs::write(&agent_output, "initial").unwrap();
    let agents_path = workflow_root.join("agents.json");
    fs::write(
        &agents_path,
        serde_json::to_string_pretty(&json!({
            "agents": [{
                "agent_id": "agent-1",
                "name": "alpha",
                "description": "demo",
                "prompt": "do work",
                "subagent_type": null,
                "model": null,
                "team_name": null,
                "mode": null,
                "isolation": null,
                "cwd": null,
                "status": "async_launched",
                "output_file": agent_output.display().to_string()
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let agents_path_bg = agents_path.clone();
    let agent_output_bg = agent_output.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        fs::write(&agent_output_bg, "done").unwrap();
        fs::write(
            &agents_path_bg,
            serde_json::to_string_pretty(&json!({
                "agents": [{
                    "agent_id": "agent-1",
                    "name": "alpha",
                    "description": "demo",
                    "prompt": "do work",
                    "subagent_type": null,
                    "model": null,
                    "team_name": null,
                    "mode": null,
                    "isolation": null,
                    "cwd": null,
                    "status": "completed",
                    "output_file": agent_output_bg.display().to_string()
                }]
            }))
            .unwrap(),
        )
        .unwrap();
    });

    let output = super::super::claude_tools::workflow::task_output::execute_task_output(
        &mut state,
        &cwd,
        json!({
            "task_id": "agent-1",
            "block": true,
            "timeout": 1_000
        }),
    )
    .unwrap();
    let output: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(output["retrieval_status"], "success");
    assert_eq!(output["task"]["task_type"], "agent");
    assert_eq!(output["task"]["status"], "completed");
    assert_eq!(output["task"]["output"], "done");
    assert_eq!(output["task"]["result"], "done");
}

#[test]
fn task_stop_rejects_non_background_tasks() {
    let mut state = temp_state();
    let cwd = state.cwd.clone();
    let created = super::super::claude_tools::workflow::task_create::execute_task_create(
        &mut state,
        &cwd,
        json!({
            "subject": "Plan work",
            "description": "Track progress"
        }),
    )
    .unwrap();
    let created: Value = serde_json::from_str(&created).unwrap();
    let task_id = created["task"]["id"].as_str().unwrap();

    let error = super::super::claude_tools::workflow::task_stop::execute_task_stop(
        &mut state,
        &cwd,
        json!({
            "task_id": task_id
        }),
    )
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("is not a running background task"));
}
