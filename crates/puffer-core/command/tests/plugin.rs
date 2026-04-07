use super::*;
use puffer_resources::{LoadedItem, PluginSpec, SourceInfo, SourceKind};

#[test]
fn plugin_command_creates_workspace_plugin_file() {
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

    dispatch_command(
        &mut state,
        &supported_commands(),
        &LoadedResources::default(),
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/plugin",
    )
    .unwrap();

    let plugin_path = paths
        .workspace_config_dir
        .join("resources/plugins/workspace.yaml");
    assert!(plugin_path.exists());
}

#[test]
fn plugin_disable_and_enable_commands_toggle_workspace_override() {
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
    let mut resources = LoadedResources::default();
    resources.plugins.push(LoadedItem {
        value: PluginSpec {
            id: "docs".to_string(),
            display_name: "Docs".to_string(),
            description: "Builtin docs helpers".to_string(),
            commands: Vec::new(),
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            lsp_servers: Vec::new(),
        },
        source_info: SourceInfo {
            path: paths.builtin_resources_dir.join("plugins/docs.yaml"),
            kind: SourceKind::Builtin,
        },
    });

    dispatch_command(
        &mut state,
        &supported_commands(),
        &resources,
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/plugin disable docs",
    )
    .unwrap();

    let workspace_override = paths
        .workspace_config_dir
        .join("resources/plugins/docs.yaml");
    assert!(workspace_override.exists());
    assert!(state.reload_resources_requested);
    let disabled: PluginSpec =
        serde_yaml::from_str(&std::fs::read_to_string(&workspace_override).unwrap()).unwrap();
    assert!(disabled.description.contains("Disabled plugin placeholder"));
    assert!(disabled.commands.is_empty());

    state.reload_resources_requested = false;
    dispatch_command(
        &mut state,
        &supported_commands(),
        &resources,
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/plugin enable docs",
    )
    .unwrap();

    assert!(!workspace_override.exists());
    assert!(state.reload_resources_requested);
}
