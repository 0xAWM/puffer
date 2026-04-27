use super::*;

#[test]
fn task_output_reads_runtime_background_agent_output_file() {
    let mut state = temp_state();
    let cwd = state.cwd.clone();
    let agent_output_dir = ConfigPaths::discover(&cwd)
        .workspace_config_dir
        .join("runtime")
        .join("agent_outputs");
    fs::create_dir_all(&agent_output_dir).unwrap();
    let output_file = agent_output_dir.join("agent-demo.json");
    fs::write(
        &output_file,
        serde_json::to_string_pretty(&json!({
            "status": "completed",
            "result": "background ok"
        }))
        .unwrap(),
    )
    .unwrap();

    let output = super::super::claude_tools::workflow::task_output::execute_task_output(
        &mut state,
        &cwd,
        json!({
            "task_id": "agent-demo",
            "block": false,
            "timeout": 50
        }),
    )
    .unwrap();
    let parsed: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["retrieval_status"], "success");
    assert_eq!(parsed["task"]["task_type"], "agent");
    assert_eq!(parsed["task"]["status"], "completed");
    assert_eq!(parsed["task"]["output"], "background ok");
    assert_eq!(parsed["task"]["result"], "background ok");
    assert_eq!(
        parsed["task"]["outputFile"].as_str(),
        Some(output_file.display().to_string().as_str())
    );
}

#[test]
fn send_user_message_stores_resolved_attachment_paths() {
    let mut state = temp_state();
    let cwd = state.cwd.clone();
    let attachment = cwd.join("note.txt");
    fs::write(&attachment, "hello").unwrap();

    let output =
        super::super::claude_tools::workflow::send_user_message::execute_send_user_message(
            &mut state,
            &cwd,
            json!({
                "message": "hello",
                "attachments": ["note.txt"],
                "status": "normal"
            }),
        )
        .unwrap();
    let parsed: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        parsed["message"]["attachments"][0].as_str(),
        Some(attachment.display().to_string().as_str())
    );
}

#[test]
fn send_user_message_ignores_workspace_ask_permissions() {
    let mut state = temp_state();
    let cwd = state.cwd.clone();
    let permissions_dir = ConfigPaths::discover(&cwd).workspace_config_dir;
    fs::create_dir_all(&permissions_dir).unwrap();
    fs::write(
        permissions_dir.join("permissions.toml"),
        "[tools]\nsend_user_message = \"ask\"\nbrief = \"ask\"\n",
    )
    .unwrap();

    let mut tool = loaded_tool(
        "SendUserMessage",
        "Send a message to the user",
        "runtime:workflow:send_user_message",
    );
    tool.value.approval_policy = Some("auto".to_string());
    tool.value.sandbox_policy = Some("read-only".to_string());
    let resources = LoadedResources {
        tools: vec![tool],
        ..LoadedResources::default()
    };
    let registry = ToolRegistry::from_resources(&resources);
    let mut providers = ProviderRegistry::new();
    providers.register(openai_provider("http://127.0.0.1".to_string()));
    let request_config = test_openai_request_config();

    let result = execute_tool_call(
        &mut state,
        &resources,
        &providers,
        &mut AuthStore::default(),
        &registry,
        "gpt-5",
        &cwd,
        ToolExecutionBackend::OpenAi {
            request_config: &request_config,
            structured_output: None,
        },
        None,
        Some("call_1"),
        "SendUserMessage",
        json!({
            "message": "hi",
            "status": "normal"
        }),
    )
    .unwrap();

    assert!(result.success);
    assert!(result.output.stdout.contains("\"message\": \"hi\""));
}

#[test]
fn execute_openai_tool_calls_respect_request_tool_filter() {
    let resources = LoadedResources {
        tools: vec![loaded_tool("bash", "Run shell", "bash")],
        ..LoadedResources::default()
    };
    let registry = ToolRegistry::from_resources(&resources);
    let mut providers = ProviderRegistry::new();
    providers.register(openai_provider("http://127.0.0.1".to_string()));
    let tool_calls = vec![OpenAIResponseToolCall {
        item_id: Some("fc_1".to_string()),
        status: Some("completed".to_string()),
        call_id: "call_1".to_string(),
        name: "bash".to_string(),
        arguments: json!({ "command": "printf hi" }),
    }];
    let request_config = test_openai_request_config();
    let filter = super::super::build_request_tool_filter(&["Read".to_string()])
        .unwrap()
        .unwrap();
    let mut state = temp_state();
    let cwd = state.cwd.clone();

    let result = execute_openai_tool_calls(
        &mut state,
        &resources,
        &providers,
        &mut AuthStore::default(),
        &tool_calls,
        &registry,
        &cwd,
        &request_config,
        "gpt-5",
        None,
        Some(&filter),
    )
    .unwrap();

    assert!(!result.invocations[0].success);
    assert!(result.outputs[0]
        .output
        .contains("slash command tool scope denied this tool call"));
}

#[test]
fn execute_anthropic_tool_calls_respect_request_tool_filter() {
    let resources = LoadedResources {
        tools: vec![loaded_tool("bash", "Run shell", "bash")],
        ..LoadedResources::default()
    };
    let registry = ToolRegistry::from_resources(&resources);
    let mut providers = ProviderRegistry::new();
    providers.register(provider());
    let request_config = test_anthropic_request_config();
    let response = json!({
        "content": [
            {
                "type": "tool_use",
                "id": "toolu_1",
                "name": "bash",
                "input": {
                    "command": "printf hi"
                }
            }
        ]
    });
    let filter = super::super::build_request_tool_filter(&["Read".to_string()])
        .unwrap()
        .unwrap();
    let mut state = temp_state();
    let cwd = state.cwd.clone();

    let result = execute_anthropic_tool_calls(
        &mut state,
        &resources,
        &providers,
        &mut AuthStore::default(),
        &response,
        &registry,
        &cwd,
        &request_config,
        "claude-sonnet-4-5",
        None,
        Some(&filter),
    )
    .unwrap()
    .expect("anthropic tool results");

    assert!(!result.invocations[0].success);
    assert!(result.invocations[0]
        .output
        .contains("slash command tool scope denied this tool call"));
}
