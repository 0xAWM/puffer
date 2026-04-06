use super::emit_system;
use crate::AppState;
use anyhow::Result;
use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use puffer_resources::{plugin_by_id, plugin_mcp_servers, LoadedResources};
use puffer_session_store::SessionStore;
use serde::Deserialize;
use std::fmt::Write as _;
use std::fs;

/// Describes loaded plugin metadata or a specific plugin manifest.
pub(crate) fn describe_plugin(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    if args.is_empty() {
        if resources.plugins.is_empty() {
            return emit_system(
                state,
                session_store,
                "No plugins are installed.".to_string(),
            );
        }
        let mut text = String::from("Plugins:\n");
        for plugin in &resources.plugins {
            let _ = writeln!(
                &mut text,
                "{} - {}",
                plugin.value.id, plugin.value.description
            );
        }
        return emit_system(state, session_store, text);
    }
    let Some(plugin) = plugin_by_id(resources, args) else {
        return emit_system(state, session_store, format!("Unknown plugin {args}."));
    };
    let mut text = format!("Plugin {}\n{}\n", plugin.value.id, plugin.value.description);
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
    emit_system(state, session_store, text)
}

/// Lists loaded MCP servers from both resource packs and plugins.
pub(crate) fn list_mcp_servers(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
) -> Result<()> {
    let servers = plugin_mcp_servers(resources);
    if servers.is_empty() && resources.mcp_servers.is_empty() {
        return emit_system(
            state,
            session_store,
            "No MCP servers are configured.".to_string(),
        );
    }
    let mut text = String::from("MCP servers:\n");
    for server in &resources.mcp_servers {
        let _ = writeln!(
            &mut text,
            "{} [{}] -> {}",
            server.value.id, server.value.transport, server.value.endpoint
        );
    }
    for (plugin, server) in servers {
        let target = if server.target.is_empty() {
            server.endpoint.as_str()
        } else {
            server.target.as_str()
        };
        let _ = writeln!(
            &mut text,
            "{}:{} [{}] -> {}",
            plugin.id, server.id, server.transport, target
        );
    }
    emit_system(state, session_store, text)
}

/// Lists loaded IDE integration manifests.
pub(crate) fn list_ides(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
) -> Result<()> {
    if resources.ides.is_empty() {
        return emit_system(
            state,
            session_store,
            "No IDE integrations are configured.".to_string(),
        );
    }
    let mut text = String::from("IDE integrations:\n");
    for ide in &resources.ides {
        let _ = writeln!(
            &mut text,
            "{} - {}",
            ide.value.display_name, ide.value.description
        );
    }
    emit_system(state, session_store, text)
}

/// Shows or materializes the workspace agents file and agent presets.
pub(crate) fn handle_agents_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let agents_path = paths.workspace_config_dir.join("agents.yaml");
    if !agents_path.exists() {
        fs::write(
            &agents_path,
            default_agents_contents(state.current_model.as_deref()),
        )?;
    }
    let trimmed = args.trim();
    if trimmed == "path" {
        return emit_system(
            state,
            session_store,
            format!("Agents file: {}", agents_path.display()),
        );
    }
    let contents = fs::read_to_string(&agents_path)?;
    let parsed = parse_agents_file(&contents)?;
    match trimmed {
        "" | "show" => emit_system(
            state,
            session_store,
            format!("Agents file: {}\n{}", agents_path.display(), contents),
        ),
        "list" => {
            let mut text = String::from("Agents:\n");
            for agent in parsed.agents {
                let _ = writeln!(
                    &mut text,
                    "- {} role={} model={}",
                    agent.id, agent.role, agent.model
                );
            }
            emit_system(state, session_store, text)
        }
        _ if trimmed.starts_with("show ") => {
            let agent_id = trimmed.trim_start_matches("show ").trim();
            if let Some(agent) = parsed.agents.iter().find(|agent| agent.id == agent_id) {
                emit_system(
                    state,
                    session_store,
                    format!(
                        "Agent {}\nrole={}\nmodel={}",
                        agent.id, agent.role, agent.model
                    ),
                )
            } else {
                emit_system(state, session_store, format!("Unknown agent {agent_id}."))
            }
        }
        _ if trimmed.starts_with("use ") => {
            let agent_id = trimmed.trim_start_matches("use ").trim();
            if let Some(agent) = parsed.agents.iter().find(|agent| agent.id == agent_id) {
                state.current_model = Some(agent.model.clone());
                state.current_provider = agent
                    .model
                    .split_once('/')
                    .map(|(provider, _)| provider.to_string())
                    .or_else(|| state.current_provider.clone());
                emit_system(
                    state,
                    session_store,
                    format!(
                        "Selected agent {}.\nrole={}\nmodel={}",
                        agent.id, agent.role, agent.model
                    ),
                )
            } else {
                emit_system(state, session_store, format!("Unknown agent {agent_id}."))
            }
        }
        _ => emit_system(
            state,
            session_store,
            "Usage: /agents [path|list|show <id>|use <id>]".to_string(),
        ),
    }
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
    if args.trim() == "path" {
        return emit_system(
            state,
            session_store,
            format!("Plugins directory: {}", plugins_dir.display()),
        );
    }
    if !args.trim().is_empty() && args.trim() != "show" {
        return describe_plugin(state, resources, session_store, args);
    }
    emit_system(
        state,
        session_store,
        format!(
            "Plugins directory: {}\nloaded_plugins={}\n{}{}",
            plugins_dir.display(),
            resources.plugins.len(),
            if resources.plugins.is_empty() {
                format!("Example plugin file: {}\n", plugin_path.display())
            } else {
                let mut summary = String::from("Loaded plugins:\n");
                for plugin in &resources.plugins {
                    let _ = writeln!(
                        &mut summary,
                        "- {} -> {}",
                        plugin.value.id, plugin.value.display_name
                    );
                }
                summary
            },
            fs::read_to_string(&plugin_path)?
        ),
    )
}

/// Shows or materializes the workspace MCP directory.
pub(crate) fn handle_mcp_command(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let mcp_dir = paths.workspace_config_dir.join("resources/mcp_servers");
    fs::create_dir_all(&mcp_dir)?;
    let server_path = mcp_dir.join("workspace.yaml");
    if !server_path.exists() {
        fs::write(&server_path, default_mcp_contents())?;
    }
    if args.trim() == "path" {
        return emit_system(
            state,
            session_store,
            format!("MCP directory: {}", mcp_dir.display()),
        );
    }
    if !args.trim().is_empty() && args.trim() != "show" {
        return list_mcp_servers(state, resources, session_store);
    }
    let mut summary = String::new();
    if resources.mcp_servers.is_empty() && plugin_mcp_servers(resources).is_empty() {
        let _ = writeln!(&mut summary, "Example MCP file: {}", server_path.display());
    } else {
        let _ = writeln!(&mut summary, "Loaded MCP servers:");
        for server in &resources.mcp_servers {
            let _ = writeln!(
                &mut summary,
                "- {} -> {}",
                server.value.id, server.value.display_name
            );
        }
        for (plugin, server) in plugin_mcp_servers(resources) {
            let _ = writeln!(
                &mut summary,
                "- {}:{} -> {}",
                plugin.id, server.id, server.display_name
            );
        }
    }
    emit_system(
        state,
        session_store,
        format!(
            "MCP directory: {}\n{}{}",
            mcp_dir.display(),
            summary,
            fs::read_to_string(&server_path)?
        ),
    )
}

/// Shows or materializes the workspace IDE integration directory.
pub(crate) fn handle_ide_command(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let ide_dir = paths.workspace_config_dir.join("resources/ides");
    fs::create_dir_all(&ide_dir)?;
    let ide_path = ide_dir.join("workspace.yaml");
    if !ide_path.exists() {
        fs::write(&ide_path, default_ide_contents())?;
    }
    if args.trim() == "path" {
        return emit_system(
            state,
            session_store,
            format!("IDE directory: {}", ide_dir.display()),
        );
    }
    if args.trim() == "list" {
        return list_ides(state, resources, session_store);
    }
    if args.trim() == "open" {
        return emit_system(
            state,
            session_store,
            format!("Open your IDE integration from {}.", ide_dir.display()),
        );
    }
    emit_system(
        state,
        session_store,
        format!(
            "IDE directory: {}\nloaded_ides={}\n{}{}",
            ide_dir.display(),
            resources.ides.len(),
            if resources.ides.is_empty() {
                format!("Example IDE file: {}\n", ide_path.display())
            } else {
                let mut summary = String::from("Loaded IDE integrations:\n");
                for ide in &resources.ides {
                    let _ = writeln!(
                        &mut summary,
                        "- {} -> {}",
                        ide.value.id, ide.value.display_name
                    );
                }
                summary
            },
            fs::read_to_string(&ide_path)?
        ),
    )
}

/// Summarizes the current plugin registry after a reload request.
pub(crate) fn reload_plugins_summary(
    state: &AppState,
    resources: &LoadedResources,
) -> Result<String> {
    let paths = ConfigPaths::discover(&state.cwd);
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    Ok(format!(
        "Reloaded plugin registry for this session.\nplugins={}\nskills={}\nmcp_servers={}\nsource_dir={}",
        resources.plugins.len(),
        resources.skills.len(),
        resources.mcp_servers.len(),
        plugins_dir.display()
    ))
}

fn default_agents_contents(model: Option<&str>) -> String {
    format!(
        "agents:\n  - id: default\n    role: coding\n    model: {}\n",
        model.unwrap_or("anthropic/claude-sonnet-4-5")
    )
}

fn default_plugin_contents() -> &'static str {
    "id: workspace\n\
display_name: Workspace Plugin\n\
description: Customize plugin commands for this workspace.\n\
commands:\n\
  - name: demo\n\
    description: Example command\n"
}

fn default_mcp_contents() -> &'static str {
    "id: workspace\n\
display_name: Workspace MCP\n\
transport: stdio\n\
endpoint: \"\"\n\
target: workspace\n\
description: Example MCP server\n"
}

fn default_ide_contents() -> &'static str {
    "id: workspace\n\
display_name: Workspace IDE\n\
description: Example IDE integration\n"
}

fn parse_agents_file(raw: &str) -> Result<AgentsFile> {
    Ok(serde_yaml::from_str(raw)?)
}

#[derive(Debug, Clone, Deserialize)]
struct AgentsFile {
    agents: Vec<AgentEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentEntry {
    id: String,
    role: String,
    model: String,
}
