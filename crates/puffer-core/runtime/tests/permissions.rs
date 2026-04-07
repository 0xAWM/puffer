use super::*;
use crate::permissions::load_runtime_permission_context;
use serde_json::json;

#[test]
fn workspace_deny_rules_filter_tools_from_model_visibility() {
    let temp = tempfile::tempdir().unwrap();
    let paths = ConfigPaths::discover(temp.path());
    ensure_workspace_dirs(&paths).unwrap();
    std::fs::write(
        paths.workspace_config_dir.join("permissions.toml"),
        "[tools]\nbash = \"deny\"\n",
    )
    .unwrap();

    let mut state = state();
    state.cwd = temp.path().to_path_buf();
    let resources = LoadedResources {
        tools: vec![
            loaded_tool("Bash", "Run shell", "runtime:claude_bash"),
            loaded_tool("Read", "Read file", "runtime:claude_read"),
        ],
        ..LoadedResources::default()
    };
    let registry = ToolRegistry::from_resources(&resources);
    let permission_context =
        load_runtime_permission_context(&state.cwd, &resources, &state).unwrap();

    let openai_tools =
        super::super::structured_output_support::openai_tool_definitions_for_request(
            &registry,
            None,
            false,
            Some(&permission_context),
        )
        .unwrap();

    assert_eq!(openai_tools.len(), 1);
    assert_eq!(openai_tools[0].name, "Read");
}

#[test]
fn plan_mode_requires_approval_for_mutating_on_request_tools() {
    let temp = tempfile::tempdir().unwrap();
    let paths = ConfigPaths::discover(temp.path());
    ensure_workspace_dirs(&paths).unwrap();

    let mut state = state();
    state.cwd = temp.path().to_path_buf();
    state.plan_mode = true;
    let mut write_tool = loaded_tool("Write", "Write file", "runtime:claude_write");
    write_tool.value.approval_policy = Some("on-request".to_string());
    write_tool.value.sandbox_policy = Some("workspace-write".to_string());
    let resources = LoadedResources {
        tools: vec![write_tool],
        ..LoadedResources::default()
    };
    let registry = ToolRegistry::from_resources(&resources);
    let definition = registry.definition("Write").unwrap();
    let permission_context =
        load_runtime_permission_context(&state.cwd, &resources, &state).unwrap();

    let error = permission_context
        .enforce_tool_call(
            definition,
            &json!({"file_path": "note.txt", "content": "hello"}),
        )
        .unwrap_err();

    assert!(error
        .to_string()
        .contains("plan mode requires approval for mutating tools"));
}

#[test]
fn sandboxed_shell_commands_still_run_by_default() {
    let temp = tempfile::tempdir().unwrap();
    let paths = ConfigPaths::discover(temp.path());
    ensure_workspace_dirs(&paths).unwrap();

    let mut state = state();
    state.cwd = temp.path().to_path_buf();
    let mut bash_tool = loaded_tool("Bash", "Run shell", "runtime:claude_bash");
    bash_tool.value.approval_policy = Some("on-request".to_string());
    bash_tool.value.sandbox_policy = Some("workspace-write".to_string());
    let resources = LoadedResources {
        tools: vec![bash_tool],
        ..LoadedResources::default()
    };
    let registry = ToolRegistry::from_resources(&resources);
    let definition = registry.definition("Bash").unwrap();
    let permission_context =
        load_runtime_permission_context(&state.cwd, &resources, &state).unwrap();

    permission_context
        .enforce_tool_call(definition, &json!({"command": "pwd"}))
        .unwrap();
}

#[test]
fn unsandboxed_shell_override_requires_approval_without_workspace_opt_in() {
    let temp = tempfile::tempdir().unwrap();
    let paths = ConfigPaths::discover(temp.path());
    ensure_workspace_dirs(&paths).unwrap();

    let mut state = state();
    state.cwd = temp.path().to_path_buf();
    let mut bash_tool = loaded_tool("Bash", "Run shell", "runtime:claude_bash");
    bash_tool.value.approval_policy = Some("on-request".to_string());
    bash_tool.value.sandbox_policy = Some("workspace-write".to_string());
    let resources = LoadedResources {
        tools: vec![bash_tool],
        ..LoadedResources::default()
    };
    let registry = ToolRegistry::from_resources(&resources);
    let definition = registry.definition("Bash").unwrap();
    let permission_context =
        load_runtime_permission_context(&state.cwd, &resources, &state).unwrap();

    let error = permission_context
        .enforce_tool_call(
            definition,
            &json!({"command": "git status", "dangerouslyDisableSandbox": true}),
        )
        .unwrap_err();

    assert!(error.to_string().contains("dangerouslyDisableSandbox"));
    assert!(error
        .to_string()
        .contains("/sandbox allow-unsandboxed true"));
}

#[test]
fn destructive_shell_command_requires_approval_even_without_unsandboxed_override() {
    let temp = tempfile::tempdir().unwrap();
    let paths = ConfigPaths::discover(temp.path());
    ensure_workspace_dirs(&paths).unwrap();

    let mut state = state();
    state.cwd = temp.path().to_path_buf();
    let mut bash_tool = loaded_tool("Bash", "Run shell", "runtime:claude_bash");
    bash_tool.value.approval_policy = Some("on-request".to_string());
    bash_tool.value.sandbox_policy = Some("workspace-write".to_string());
    let resources = LoadedResources {
        tools: vec![bash_tool],
        ..LoadedResources::default()
    };
    let registry = ToolRegistry::from_resources(&resources);
    let definition = registry.definition("Bash").unwrap();
    let permission_context =
        load_runtime_permission_context(&state.cwd, &resources, &state).unwrap();

    let error = permission_context
        .enforce_tool_call(definition, &json!({"command": "rm -rf /"}))
        .unwrap_err();

    assert!(error.to_string().contains("dangerously destructive"));
}
