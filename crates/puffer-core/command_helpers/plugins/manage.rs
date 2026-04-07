use super::super::emit_system;
use super::support::{
    disabled_placeholder_for, is_disabled_placeholder, plugin_help_text, render_plugin_marketplace,
};
use crate::AppState;
use anyhow::Result;
use puffer_config::ConfigPaths;
use puffer_resources::{LoadedResources, PluginSpec, SourceKind};
use puffer_session_store::SessionStore;
use std::fs;
use std::path::{Path, PathBuf};

/// Installs a plugin manifest into the workspace plugin directory.
pub(super) fn install_workspace_plugin(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    paths: &ConfigPaths,
    plugin_ref: &str,
) -> Result<()> {
    let plugin_ref = plugin_ref.trim();
    if plugin_ref.is_empty() {
        return emit_system(
            state,
            session_store,
            format!(
                "{}\n\n{}",
                plugin_help_text(),
                render_plugin_marketplace(resources)
            ),
        );
    }

    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    fs::create_dir_all(&plugins_dir)?;
    let (plugin, raw) = resolve_plugin_install_source(resources, plugin_ref)?;
    let enabled_path = plugin_manifest_path(&plugins_dir, &plugin.id);
    let disabled_path = disabled_variant(&enabled_path);
    fs::write(&enabled_path, raw)?;
    remove_if_exists(&disabled_path)?;
    state.reload_resources_requested = true;
    emit_system(
        state,
        session_store,
        format!(
            "Installed plugin `{}` into {}.",
            plugin.id,
            enabled_path.display()
        ),
    )
}

/// Uninstalls the workspace-local copy or disabled state for one plugin.
pub(super) fn uninstall_workspace_plugin(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    paths: &ConfigPaths,
    plugin_id: &str,
) -> Result<()> {
    let plugin_id = plugin_id.trim();
    if plugin_id.is_empty() {
        return emit_system(state, session_store, plugin_help_text());
    }
    if plugin_id == "workspace" {
        return emit_system(
            state,
            session_store,
            "The workspace plugin manifest cannot be uninstalled.".to_string(),
        );
    }

    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    let enabled_path = plugin_manifest_path(&plugins_dir, plugin_id);
    let disabled_path = disabled_variant(&enabled_path);
    let removed_enabled = enabled_path.exists();
    let removed_disabled = disabled_path.exists();
    remove_if_exists(&enabled_path)?;
    remove_if_exists(&disabled_path)?;
    if removed_enabled || removed_disabled {
        state.reload_resources_requested = true;
        return emit_system(
            state,
            session_store,
            format!(
                "Removed local plugin state for `{plugin_id}` in {}.",
                plugins_dir.display()
            ),
        );
    }
    if resources.plugins.iter().any(|plugin| {
        plugin.value.id == plugin_id && plugin.source_info.kind == SourceKind::Builtin
    }) {
        return emit_system(
            state,
            session_store,
            format!(
                "Builtin plugin `{plugin_id}` cannot be uninstalled. Use `/plugin disable {plugin_id}`."
            ),
        );
    }
    if resources.plugins.iter().any(|plugin| {
        plugin.value.id == plugin_id && plugin.source_info.kind != SourceKind::Workspace
    }) {
        return emit_system(
            state,
            session_store,
            format!(
                "Plugin `{plugin_id}` is loaded from a non-workspace source and has no workspace override to uninstall."
            ),
        );
    }
    emit_system(
        state,
        session_store,
        format!("Unknown plugin `{plugin_id}`."),
    )
}

/// Updates the workspace copy of one plugin from builtin or user resources.
pub(super) fn update_workspace_plugin(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    paths: &ConfigPaths,
    plugin_id: &str,
) -> Result<()> {
    let plugin_id = plugin_id.trim();
    if plugin_id.is_empty() {
        return emit_system(state, session_store, plugin_help_text());
    }

    let Some((source_path, raw)) = resolve_plugin_update_source(resources, plugin_id)? else {
        return emit_system(
            state,
            session_store,
            format!("No builtin or user plugin source is available for `{plugin_id}`."),
        );
    };
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    fs::create_dir_all(&plugins_dir)?;
    let enabled_path = plugin_manifest_path(&plugins_dir, plugin_id);
    let disabled_path = disabled_variant(&enabled_path);

    if enabled_path.exists() {
        let plugin: PluginSpec = serde_yaml::from_str(&fs::read_to_string(&enabled_path)?)?;
        if is_disabled_placeholder(&plugin) {
            if disabled_path.exists() {
                fs::write(&disabled_path, raw)?;
                state.reload_resources_requested = true;
                return emit_system(
                    state,
                    session_store,
                    format!(
                        "Updated disabled plugin `{plugin_id}` from {}.",
                        source_path.display()
                    ),
                );
            }
            return emit_system(
                state,
                session_store,
                format!(
                    "Plugin `{plugin_id}` remains disabled; no local copy required refreshing from {}.",
                    source_path.display()
                ),
            );
        }
    }

    if disabled_path.exists() {
        fs::write(&disabled_path, raw)?;
        state.reload_resources_requested = true;
        return emit_system(
            state,
            session_store,
            format!(
                "Updated disabled plugin `{plugin_id}` from {}.",
                source_path.display()
            ),
        );
    }

    fs::write(&enabled_path, raw)?;
    state.reload_resources_requested = true;
    emit_system(
        state,
        session_store,
        format!(
            "Updated plugin `{plugin_id}` from {}.",
            source_path.display()
        ),
    )
}

/// Disables one plugin by writing or swapping in a placeholder manifest.
pub(super) fn disable_workspace_plugin(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    paths: &ConfigPaths,
    plugin_id: &str,
) -> Result<()> {
    let plugin_id = plugin_id.trim();
    if plugin_id.is_empty() {
        return emit_system(
            state,
            session_store,
            "Usage: /plugin disable <id>".to_string(),
        );
    }
    if plugin_id == "workspace" {
        return emit_system(
            state,
            session_store,
            "The workspace plugin manifest cannot be disabled.".to_string(),
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

/// Re-enables one plugin by restoring the saved manifest or removing the placeholder.
pub(super) fn enable_workspace_plugin(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    paths: &ConfigPaths,
    plugin_id: &str,
) -> Result<()> {
    let plugin_id = plugin_id.trim();
    if plugin_id.is_empty() {
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

fn resolve_plugin_install_source(
    resources: &LoadedResources,
    plugin_ref: &str,
) -> Result<(PluginSpec, String)> {
    let path = Path::new(plugin_ref);
    if path.exists() {
        let raw = fs::read_to_string(path)?;
        let plugin: PluginSpec = serde_yaml::from_str(&raw)?;
        return Ok((plugin, raw));
    }
    let plugin = resources
        .plugins
        .iter()
        .find(|plugin| {
            plugin.value.id == plugin_ref
                && plugin.source_info.kind != SourceKind::Workspace
                && !is_disabled_placeholder(&plugin.value)
        })
        .ok_or_else(|| anyhow::anyhow!("Unknown plugin `{plugin_ref}`."))?;
    let raw = fs::read_to_string(&plugin.source_info.path)
        .unwrap_or_else(|_| serde_yaml::to_string(&plugin.value).unwrap_or_default());
    let resolved =
        serde_yaml::from_str::<PluginSpec>(&raw).unwrap_or_else(|_| plugin.value.clone());
    Ok((resolved, raw))
}

fn resolve_plugin_update_source(
    resources: &LoadedResources,
    plugin_id: &str,
) -> Result<Option<(PathBuf, String)>> {
    let source = resources
        .plugins
        .iter()
        .filter(|plugin| {
            plugin.value.id == plugin_id
                && plugin.source_info.kind != SourceKind::Workspace
                && !is_disabled_placeholder(&plugin.value)
        })
        .min_by_key(|plugin| match plugin.source_info.kind {
            SourceKind::User => 0u8,
            SourceKind::Builtin => 1u8,
            SourceKind::Workspace => 2u8,
        });
    source
        .map(|plugin| {
            let raw = fs::read_to_string(&plugin.source_info.path)
                .unwrap_or_else(|_| serde_yaml::to_string(&plugin.value).unwrap_or_default());
            Ok((plugin.source_info.path.clone(), raw))
        })
        .transpose()
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
