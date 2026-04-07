use super::*;
use puffer_resources::{LoadedItem, PluginSpec, SourceInfo, SourceKind};
use std::fs;

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
            agents: Vec::new(),
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

#[test]
fn plugin_validate_reports_duplicate_entries() {
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
            commands: vec![
                puffer_resources::PluginCommandSpec {
                    name: "search".to_string(),
                    description: String::new(),
                },
                puffer_resources::PluginCommandSpec {
                    name: "search".to_string(),
                    description: String::new(),
                },
            ],
            skills: vec!["reviewer".to_string(), "reviewer".to_string()],
            agents: Vec::new(),
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
        "/plugin validate docs",
    )
    .unwrap();

    assert!(matches!(
        state.transcript.last(),
        Some(RenderedMessage {
            role: MessageRole::System,
            text,
        }) if text.contains("duplicate command `search`") && text.contains("duplicate skill `reviewer`")
    ));
}

#[test]
fn plugin_errors_filters_resource_diagnostics() {
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
    resources
        .diagnostics
        .push("workspace plugin `docs` from /tmp/plugins/docs.yaml overrides builtin resource from /builtin/plugins/docs.yaml".to_string());
    resources
        .diagnostics
        .push("workspace prompt `review` from /tmp/prompts/review.yaml overrides builtin resource from /builtin/prompts/review.yaml".to_string());

    dispatch_command(
        &mut state,
        &supported_commands(),
        &resources,
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/plugin errors",
    )
    .unwrap();

    assert!(matches!(
        state.transcript.last(),
        Some(RenderedMessage {
            role: MessageRole::System,
            text,
        }) if text.contains("errors=1") && text.contains("plugin `docs`") && !text.contains("prompt `review`")
    ));
}

#[test]
fn plugin_marketplace_lists_builtin_plugins() {
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
    let builtin_path = paths.builtin_resources_dir.join("plugins/docs.yaml");
    fs::create_dir_all(builtin_path.parent().unwrap()).unwrap();
    fs::write(
        &builtin_path,
        "id: docs\ndisplay_name: Docs\ndescription: Builtin docs helpers\n",
    )
    .unwrap();
    resources.plugins.push(LoadedItem {
        value: PluginSpec {
            id: "docs".to_string(),
            display_name: "Docs".to_string(),
            description: "Builtin docs helpers".to_string(),
            commands: Vec::new(),
            skills: Vec::new(),
            agents: Vec::new(),
            mcp_servers: Vec::new(),
            lsp_servers: Vec::new(),
        },
        source_info: SourceInfo {
            path: builtin_path,
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
        "/plugin marketplace",
    )
    .unwrap();

    assert!(matches!(
        state.transcript.last(),
        Some(RenderedMessage {
            role: MessageRole::System,
            text,
        }) if text.contains("Plugin marketplace") && text.contains("docs") && text.contains("Builtin docs helpers")
    ));
}

#[test]
fn plugin_install_update_and_uninstall_manage_workspace_copy() {
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
    let builtin_path = paths.builtin_resources_dir.join("plugins/docs.yaml");
    fs::create_dir_all(builtin_path.parent().unwrap()).unwrap();
    fs::write(
        &builtin_path,
        "id: docs\ndisplay_name: Docs\ndescription: Builtin docs helpers\n",
    )
    .unwrap();
    resources.plugins.push(LoadedItem {
        value: PluginSpec {
            id: "docs".to_string(),
            display_name: "Docs".to_string(),
            description: "Builtin docs helpers".to_string(),
            commands: Vec::new(),
            skills: Vec::new(),
            agents: Vec::new(),
            mcp_servers: Vec::new(),
            lsp_servers: Vec::new(),
        },
        source_info: SourceInfo {
            path: builtin_path.clone(),
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
        "/plugin install docs",
    )
    .unwrap();

    let workspace_copy = paths
        .workspace_config_dir
        .join("resources/plugins/docs.yaml");
    assert!(workspace_copy.exists());
    assert!(state.reload_resources_requested);
    assert!(fs::read_to_string(&workspace_copy)
        .unwrap()
        .contains("Builtin docs helpers"));

    state.reload_resources_requested = false;
    fs::write(
        &builtin_path,
        "id: docs\ndisplay_name: Docs\ndescription: Refreshed builtin docs helpers\n",
    )
    .unwrap();
    dispatch_command(
        &mut state,
        &supported_commands(),
        &resources,
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/plugin update docs",
    )
    .unwrap();

    assert!(state.reload_resources_requested);
    assert!(fs::read_to_string(&workspace_copy)
        .unwrap()
        .contains("Refreshed builtin docs helpers"));

    state.reload_resources_requested = false;
    dispatch_command(
        &mut state,
        &supported_commands(),
        &resources,
        &mut ProviderRegistry::new(),
        &mut AuthStore::default(),
        &session_store,
        "/plugin uninstall docs",
    )
    .unwrap();

    assert!(!workspace_copy.exists());
    assert!(state.reload_resources_requested);
}
