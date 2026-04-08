use super::*;

#[test]
fn execute_openai_tool_calls_surfaces_tool_errors_as_results() {
    let missing_path = std::env::current_dir()
        .unwrap()
        .join("definitely-missing-read-target.txt");
    let resources = LoadedResources {
        tools: vec![loaded_tool("Read", "Read a file", "read")],
        ..LoadedResources::default()
    };
    let registry = ToolRegistry::from_resources(&resources);
    let mut providers = ProviderRegistry::new();
    providers.register(openai_provider("https://api.openai.com".to_string()));
    let mut state = state();
    let result = execute_openai_tool_calls(
        &mut state,
        &resources,
        &providers,
        &mut AuthStore::default(),
        &[OpenAIResponseToolCall {
            item_id: None,
            status: None,
            call_id: "call_1".to_string(),
            name: "Read".to_string(),
            arguments: json!({ "file_path": missing_path }),
        }],
        &registry,
        std::env::current_dir().unwrap().as_path(),
        &test_openai_request_config(),
        "gpt-5",
        None,
        None,
    )
    .unwrap();
    assert_eq!(result.invocations.len(), 1);
    assert!(!result.invocations[0].success);
    assert!(result.invocations[0]
        .output
        .contains("Tool execution failed:"));
}
