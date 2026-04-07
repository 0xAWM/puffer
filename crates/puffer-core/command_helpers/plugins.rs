use super::common::open_text_file_in_editor;
use super::emit_system;
use crate::AppState;
use anyhow::Result;
use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use puffer_resources::{
    plugin_lsp_servers, plugin_mcp_servers, LoadedItem, LoadedResources, PluginSpec, SourceInfo,
    SourceKind,
};
use puffer_session_store::SessionStore;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const DISABLED_PLUGIN_PLACEHOLDER_PREFIX: &str =
    "Disabled plugin placeholder created by `puffer plugin disable`.";

/// Describes one interactive `/plugin` action exposed in the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginActionEntry {
    /// The slash command executed when the action is selected.
    pub command: String,
    /// The row description shown in the interactive picker.
    pub description: String,
}

/// Shows or materializes the workspace plugin directory.
pub(crate) fn handle_plugin_command(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    fs::create_dir_all(&plugins_dir)?;
    let plugin_path = plugins_dir.join("workspace.yaml");
    if !plugin_path.exists() {
        fs::write(&plugin_path, default_plugin_contents())?;
    }
    let trimmed = args.trim();
    let inventory = plugin_inventory(&paths, resources)?;

    match trimmed {
        "" | "show" | "manage" => {
            emit_system(state, session_store, render_plugin_summary(state, resources)?)
        }
        "path" => emit_system(
            state,
            session_store,
            format!(
                "Plugins directory: {}\nWorkspace plugin manifest: {}",
                plugins_dir.display(),
                plugin_path.display()
            ),
        ),
        "list" => emit_system(state, session_store, render_plugin_listing(&inventory)),
        "reload" => {
            state.reload_resources_requested = true;
            emit_system(
                state,
                session_store,
                "Reloading plugin changes from disk for this session...".to_string(),
            )
        }
        "open" | "edit" => open_plugin_file(state, session_store, &plugin_path),
        _ if trimmed.starts_with("show ") => {
            let plugin_id = trimmed.trim_start_matches("show ").trim();
            describe_plugin(state, session_store, &inventory, plugin_id)
        }
        _ if trimmed.starts_with("open ") || trimmed.starts_with("edit ") => {
            let plugin_id = trimmed
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            open_named_plugin_file(state, session_store, &inventory, plugin_id)
        }
        _ if trimmed.starts_with("enable ") => {
            let plugin_id = trimmed.trim_start_matches("enable ").trim();
            enable_workspace_plugin(state, resources, session_store, &paths, plugin_id)
        }
        _ if trimmed.starts_with("disable ") => {
            let plugin_id = trimmed.trim_start_matches("disable ").trim();
            disable_workspace_plugin(state, resources, session_store, &paths, plugin_id)
        }
        _ if inventory.iter().any(|plugin| plugin.value.id == trimmed) => {
            describe_plugin(state, session_store, &inventory, trimmed)
        }
        _ => emit_system(
            state,
            session_store,
            "Usage: /plugin [show|manage|list|path|open [id]|edit [id]|enable <id>|disable <id>|reload]".to_string(),
        ),
    }
}

/// Summarizes the current plugin registry after a reload request.
pub(crate) fn reload_plugins_summary(
    state: &AppState,
    resources: &LoadedResources,
) -> Result<String> {
    let paths = ConfigPaths::discover(&state.cwd);
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    Ok(format!(
        "Reloaded plugin registry for this session.\nplugins={}\nskills={}\nmcp_servers={}\nlsp_servers={}\nsource_dir={}",
        resources.plugins.len(),
        resources.skills.len(),
        resources.mcp_servers.len() + plugin_mcp_servers(resources).len(),
        plugin_lsp_servers(resources).len(),
        plugins_dir.display()
    ))
}

/// Renders the plugin summary shown by `/plugin` with no arguments.
pub(crate) fn render_plugin_summary(
    state: &AppState,
    resources: &LoadedResources,
) -> Result<String> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    fs::create_dir_all(&plugins_dir)?;
    let plugin_path = plugins_dir.join("workspace.yaml");
    if !plugin_path.exists() {
        fs::write(&plugin_path, default_plugin_contents())?;
    }
    let inventory = plugin_inventory(&paths, resources)?;
    Ok(format!(
        "Plugins directory: {}\nworkspace_plugin_manifest={}\nloaded_plugins={}\n{}\nUse `/plugin enable <id>`, `/plugin disable <id>`, `/plugin open <id>`, or `/reload-plugins`.\n\n{}",
        plugins_dir.display(),
        plugin_path.display(),
        inventory.iter().filter(|plugin| !is_disabled_placeholder(&plugin.value)).count(),
        render_plugin_listing(&inventory),
        fs::read_to_string(&plugin_path)?
    ))
}

/// Builds the interactive `/plugin` action list used by the TUI picker.
pub(crate) fn render_plugin_actions(
    state: &AppState,
    resources: &LoadedResources,
) -> Result<Vec<PluginActionEntry>> {
    let paths = ConfigPaths::discover(&state.cwd);
    let inventory = plugin_inventory(&paths, resources)?;
    let mut actions = vec![
        PluginActionEntry {
            command: "/plugin open".to_string(),
            description: format!(
                "Edit workspace plugin manifest ({})",
                paths
                    .workspace_config_dir
                    .join("resources/plugins/workspace.yaml")
                    .display()
            ),
        },
        PluginActionEntry {
            command: "/reload-plugins".to_string(),
            description: "Reload plugin changes from disk for this session".to_string(),
        },
    ];
    for plugin in &inventory {
        let status = plugin_status(&plugin.value);
        let counts = format_plugin_counts(&plugin.value);
        let label = if plugin.value.display_name == plugin.value.id {
            plugin.value.display_name.clone()
        } else {
            format!("{} ({})", plugin.value.id, plugin.value.display_name)
        };
        actions.push(PluginActionEntry {
            command: format!(
                "/plugin {} {}",
                if is_disabled_placeholder(&plugin.value) {
                    "enable"
                } else {
                    "disable"
                },
                plugin.value.id
            ),
            description: format!(
                "{} [{}] {} • {}",
                label,
                status,
                source_kind_label(plugin.source_info.kind),
                counts
            ),
        });
        actions.push(PluginActionEntry {
            command: format!("/plugin open {}", plugin.value.id),
            description: format!("Open manifest {}", plugin.source_info.path.display()),
        });
    }
    Ok(actions)
}

fn plugin_inventory(
    paths: &ConfigPaths,
    resources: &LoadedResources,
) -> Result<Vec<LoadedItem<PluginSpec>>> {
    ensure_workspace_dirs(paths)?;
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    fs::create_dir_all(&plugins_dir)?;
    let workspace_plugin_path = plugins_dir.join("workspace.yaml");
    if !workspace_plugin_path.exists() {
        fs::write(&workspace_plugin_path, default_plugin_contents())?;
    }

    let mut inventory = resources.plugins.clone();
    if !inventory
        .iter()
        .any(|plugin| plugin.value.id == "workspace")
    {
        inventory.push(LoadedItem {
            value: serde_yaml::from_str(&fs::read_to_string(&workspace_plugin_path)?)?,
            source_info: SourceInfo {
                path: workspace_plugin_path,
                kind: SourceKind::Workspace,
            },
        });
    }
    inventory.sort_by(|left, right| left.value.id.cmp(&right.value.id));
    Ok(inventory)
}

fn describe_plugin(
    state: &mut AppState,
    session_store: &SessionStore,
    inventory: &[LoadedItem<PluginSpec>],
    plugin_id: &str,
) -> Result<()> {
    let Some(plugin) = inventory.iter().find(|plugin| plugin.value.id == plugin_id) else {
        return emit_system(
            state,
            session_store,
            format!("Unknown plugin `{plugin_id}`."),
        );
    };
    let mut text = String::new();
    let _ = writeln!(&mut text, "Plugin {}", plugin.value.id);
    let _ = writeln!(&mut text, "Name: {}", plugin.value.display_name);
    let _ = writeln!(&mut text, "Status: {}", plugin_status(&plugin.value));
    let _ = writeln!(
        &mut text,
        "Source: {} ({})",
        source_kind_label(plugin.source_info.kind),
        plugin.source_info.path.display()
    );
    let description = plugin_description(&plugin.value);
    if !description.is_empty() {
        let _ = writeln!(&mut text, "Description: {description}");
    }
    let _ = writeln!(&mut text, "Counts: {}", format_plugin_counts(&plugin.value));
    if !plugin.value.commands.is_empty() {
        let commands = plugin
            .value
            .commands
            .iter()
            .map(|command| command.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(&mut text, "Commands: {commands}");
    }
    if !plugin.value.skills.is_empty() {
        let _ = writeln!(&mut text, "Skills: {}", plugin.value.skills.join(", "));
    }
    if !plugin.value.mcp_servers.is_empty() {
        let ids = plugin
            .value
            .mcp_servers
            .iter()
            .map(|server| server.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(&mut text, "MCP servers: {ids}");
    }
    if !plugin.value.lsp_servers.is_empty() {
        let ids = plugin
            .value
            .lsp_servers
            .iter()
            .map(|server| server.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(&mut text, "LSP servers: {ids}");
    }
    emit_system(state, session_store, text)
}

fn render_plugin_listing(inventory: &[LoadedItem<PluginSpec>]) -> String {
    if inventory.is_empty() {
        return "Plugins:\n<none>".to_string();
    }
    let mut text = String::from("Plugins:\n");
    for plugin in inventory {
        let description = plugin_description(&plugin.value);
        let details = if description.is_empty() {
            format_plugin_counts(&plugin.value)
        } else {
            format!("{description} • {}", format_plugin_counts(&plugin.value))
        };
        let _ = writeln!(
            &mut text,
            "- {} [{}] source={} path={} • {}",
            plugin.value.id,
            plugin_status(&plugin.value),
            source_kind_label(plugin.source_info.kind),
            plugin.source_info.path.display(),
            details
        );
    }
    text
}

fn open_named_plugin_file(
    state: &mut AppState,
    session_store: &SessionStore,
    inventory: &[LoadedItem<PluginSpec>],
    plugin_id: &str,
) -> Result<()> {
    let Some(plugin) = inventory.iter().find(|plugin| plugin.value.id == plugin_id) else {
        return emit_system(
            state,
            session_store,
            format!("Unknown plugin `{plugin_id}`."),
        );
    };
    open_plugin_file(state, session_store, &plugin.source_info.path)
}

fn open_plugin_file(state: &mut AppState, session_store: &SessionStore, path: &Path) -> Result<()> {
    match open_text_file_in_editor(path) {
        Ok(status) => emit_system(state, session_store, status),
        Err(error) => emit_system(
            state,
            session_store,
            format!(
                "Could not open plugin manifest in an editor: {error}\nPath: {}",
                path.display()
            ),
        ),
    }
}

fn disable_workspace_plugin(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    paths: &ConfigPaths,
    plugin_id: &str,
) -> Result<()> {
    if plugin_id.trim().is_empty() {
        return emit_system(
            state,
            session_store,
            "Usage: /plugin disable <id>".to_string(),
        );
    }
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    fs::create_dir_all(&plugins_dir)?;
    let enabled_path = plugin_manifest_path(&plugins_dir, plugin_id);
    let disabled_path = disabled_variant(&enabled_path);
    if enabled_path.exists() {
        let plugin: PluginSpec = serde_yaml::from_str(&fs::read_to_string(&enabled_path)?)?;
        if is_disabled_placeholder(&plugin) {
            return emit_system(
                state,
                session_store,
                format!("Plugin `{plugin_id}` is already disabled."),
            );
        }
        remove_if_exists(&disabled_path)?;
        fs::rename(&enabled_path, &disabled_path)?;
        write_plugin_manifest(&enabled_path, &disabled_placeholder_for(&plugin))?;
        state.reload_resources_requested = true;
        return emit_system(
            state,
            session_store,
            format!(
                "Disabled plugin `{plugin_id}` in {}.",
                enabled_path.display()
            ),
        );
    }
    let Some(plugin) = resources
        .plugins
        .iter()
        .find(|plugin| plugin.value.id == plugin_id)
    else {
        return emit_system(
            state,
            session_store,
            format!("Unknown plugin `{plugin_id}`."),
        );
    };
    write_plugin_manifest(&enabled_path, &disabled_placeholder_for(&plugin.value))?;
    state.reload_resources_requested = true;
    emit_system(
        state,
        session_store,
        format!(
            "Disabled plugin `{plugin_id}` in {}.",
            enabled_path.display()
        ),
    )
}

fn enable_workspace_plugin(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    paths: &ConfigPaths,
    plugin_id: &str,
) -> Result<()> {
    if plugin_id.trim().is_empty() {
        return emit_system(
            state,
            session_store,
            "Usage: /plugin enable <id>".to_string(),
        );
    }
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    let enabled_path = plugin_manifest_path(&plugins_dir, plugin_id);
    let disabled_path = disabled_variant(&enabled_path);
    if enabled_path.exists() {
        let plugin: PluginSpec = serde_yaml::from_str(&fs::read_to_string(&enabled_path)?)?;
        if is_disabled_placeholder(&plugin) {
            if disabled_path.exists() {
                fs::remove_file(&enabled_path)?;
                fs::rename(&disabled_path, &enabled_path)?;
            } else {
                fs::remove_file(&enabled_path)?;
            }
            state.reload_resources_requested = true;
            return emit_system(
                state,
                session_store,
                format!("Enabled plugin `{plugin_id}`."),
            );
        }
        return emit_system(
            state,
            session_store,
            format!("Plugin `{plugin_id}` is already enabled."),
        );
    }
    if disabled_path.exists() {
        fs::rename(&disabled_path, &enabled_path)?;
        state.reload_resources_requested = true;
        return emit_system(
            state,
            session_store,
            format!(
                "Enabled plugin `{plugin_id}` in {}.",
                enabled_path.display()
            ),
        );
    }
    if resources
        .plugins
        .iter()
        .any(|plugin| plugin.value.id == plugin_id && !is_disabled_placeholder(&plugin.value))
    {
        return emit_system(
            state,
            session_store,
            format!("Plugin `{plugin_id}` is already enabled."),
        );
    }
    emit_system(
        state,
        session_store,
        format!("Unknown plugin `{plugin_id}`."),
    )
}

fn plugin_status(plugin: &PluginSpec) -> &'static str {
    if is_disabled_placeholder(plugin) {
        "disabled"
    } else {
        "enabled"
    }
}

fn plugin_description(plugin: &PluginSpec) -> String {
    plugin
        .description
        .strip_prefix(DISABLED_PLUGIN_PLACEHOLDER_PREFIX)
        .map(str::trim)
        .and_then(|value| value.strip_prefix("Original description:").map(str::trim))
        .unwrap_or(plugin.description.as_str())
        .to_string()
}

fn format_plugin_counts(plugin: &PluginSpec) -> String {
    format!(
        "commands={} skills={} mcp_servers={} lsp_servers={}",
        plugin.commands.len(),
        plugin.skills.len(),
        plugin.mcp_servers.len(),
        plugin.lsp_servers.len()
    )
}

fn is_disabled_placeholder(plugin: &PluginSpec) -> bool {
    plugin
        .description
        .starts_with(DISABLED_PLUGIN_PLACEHOLDER_PREFIX)
}

fn disabled_placeholder_for(plugin: &PluginSpec) -> PluginSpec {
    PluginSpec {
        id: plugin.id.clone(),
        display_name: plugin.display_name.clone(),
        description: format!(
            "{DISABLED_PLUGIN_PLACEHOLDER_PREFIX} Original description: {}",
            plugin_description(plugin)
        ),
        commands: Vec::new(),
        skills: Vec::new(),
        mcp_servers: Vec::new(),
        lsp_servers: Vec::new(),
    }
}

fn plugin_manifest_path(dir: &Path, plugin_id: &str) -> PathBuf {
    dir.join(format!("{plugin_id}.yaml"))
}

fn disabled_variant(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.disabled", path.display()))
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn write_plugin_manifest(path: &Path, plugin: &PluginSpec) -> Result<()> {
    fs::write(path, serde_yaml::to_string(plugin)?)?;
    Ok(())
}

fn default_plugin_contents() -> &'static str {
    "id: workspace\n\
display_name: Workspace Plugin\n\
description: Customize plugin commands for this workspace.\n"
}

fn source_kind_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Builtin => "builtin",
        SourceKind::User => "user",
        SourceKind::Workspace => "workspace",
    }
}
