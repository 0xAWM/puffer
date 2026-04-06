use super::emit_system;
use crate::AppState;
use anyhow::Result;
use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use puffer_resources::LoadedResources;
use puffer_session_store::SessionStore;
use puffer_tools::ToolRegistry;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

/// Summarizes loaded tool approval and sandbox metadata.
pub(crate) fn describe_permissions(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
) -> Result<()> {
    let registry = ToolRegistry::from_resources(resources);
    if registry.tools().count() == 0 {
        return emit_system(
            state,
            session_store,
            "No tool metadata is loaded.".to_string(),
        );
    }

    let mut text = String::from("Tool permission summary:\n");
    for tool in registry.tools() {
        let _ = writeln!(
            &mut text,
            "- {} [{}]: approval={} sandbox={}",
            tool.spec.name,
            tool.spec.handler,
            tool.spec
                .policy
                .approval_policy
                .as_deref()
                .unwrap_or("<unspecified>"),
            tool.spec
                .policy
                .sandbox_policy
                .as_deref()
                .unwrap_or("<unspecified>")
        );
    }
    emit_system(state, session_store, text)
}

/// Shows or materializes the workspace permissions file.
pub(crate) fn handle_permissions_command(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let permissions_path = paths.workspace_config_dir.join("permissions.toml");
    if !permissions_path.exists() {
        fs::write(&permissions_path, default_permissions_contents(resources))?;
    }
    match args.trim() {
        "path" => emit_system(
            state,
            session_store,
            format!("Permissions file: {}", permissions_path.display()),
        ),
        "" | "show" => emit_system(
            state,
            session_store,
            format!(
                "Permissions file: {}\n{}",
                permissions_path.display(),
                fs::read_to_string(&permissions_path)?
            ),
        ),
        _ => describe_permissions(state, resources, session_store),
    }
}

/// Shows or updates the workspace config file.
pub(crate) fn handle_config_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let config_path = paths.workspace_config_file();
    let trimmed = args.trim();

    if trimmed.is_empty() || trimmed == "show" {
        return emit_system(
            state,
            session_store,
            format!(
                "Config summary:\npath={}\napp_name={}\ndefault_provider={}\ndefault_model={}\ntheme={}\nno_alt_screen={}\ntmux_golden_mode={}",
                config_path.display(),
                state.config.app_name,
                state.config.default_provider.as_deref().unwrap_or("<unset>"),
                state.config.default_model.as_deref().unwrap_or("<unset>"),
                state.config.theme,
                state.config.ui.no_alt_screen,
                state.config.ui.tmux_golden_mode,
            ),
        );
    }

    if trimmed == "path" {
        return emit_system(
            state,
            session_store,
            format!("Workspace config path: {}", config_path.display()),
        );
    }

    let Some(rest) = trimmed.strip_prefix("set ") else {
        return emit_system(
            state,
            session_store,
            "Usage: /config [show|path|set <theme|default_provider|default_model|no_alt_screen|tmux_golden_mode> <value>]".to_string(),
        );
    };
    let Some((key, value)) = rest.split_once(' ') else {
        return emit_system(
            state,
            session_store,
            "Usage: /config set <key> <value>".to_string(),
        );
    };
    let value = value.trim();
    match key {
        "theme" => state.config.theme = value.to_string(),
        "default_provider" => state.config.default_provider = Some(value.to_string()),
        "default_model" => state.config.default_model = Some(value.to_string()),
        "no_alt_screen" => state.config.ui.no_alt_screen = parse_bool(value)?,
        "tmux_golden_mode" => state.config.ui.tmux_golden_mode = parse_bool(value)?,
        _ => {
            return emit_system(
                state,
                session_store,
                format!("Unsupported config key {key}."),
            );
        }
    }
    write_workspace_config(state, &config_path)?;
    emit_system(
        state,
        session_store,
        format!("Updated {key} in {}.", config_path.display()),
    )
}

/// Shows or materializes the workspace keybindings file.
pub(crate) fn handle_keybindings_command(
    state: &mut AppState,
    session_store: &SessionStore,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let keybindings_path = paths.workspace_config_dir.join("keybindings.toml");
    if !keybindings_path.exists() {
        fs::write(&keybindings_path, default_keybindings_contents())?;
    }
    emit_system(
        state,
        session_store,
        format!(
            "Keybindings file: {}\n{}",
            keybindings_path.display(),
            fs::read_to_string(&keybindings_path)?
        ),
    )
}

/// Shows or materializes the workspace hooks directory and example hook.
pub(crate) fn handle_hooks_command(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let hooks_dir = paths.workspace_config_dir.join("resources/hooks");
    fs::create_dir_all(&hooks_dir)?;
    let hooks_path = hooks_dir.join("tool_end.yaml");
    if !hooks_path.exists() {
        fs::write(&hooks_path, default_hooks_contents())?;
    }
    if args.trim() == "path" {
        return emit_system(
            state,
            session_store,
            format!("Hooks directory: {}", hooks_dir.display()),
        );
    }
    emit_system(
        state,
        session_store,
        format!(
            "Hooks directory: {}\nloaded_hooks={}\n{}{}",
            hooks_dir.display(),
            resources.hooks.len(),
            if resources.hooks.is_empty() {
                format!("Example hook file: {}\n", hooks_path.display())
            } else {
                let mut summary = String::from("Loaded hooks:\n");
                for hook in &resources.hooks {
                    let _ = writeln!(
                        &mut summary,
                        "- {} [{}] -> {}",
                        hook.value.id, hook.value.event, hook.value.command
                    );
                }
                summary
            },
            fs::read_to_string(&hooks_path)?
        ),
    )
}

fn parse_bool(value: &str) -> Result<bool> {
    match value {
        "true" | "on" | "1" => Ok(true),
        "false" | "off" | "0" => Ok(false),
        _ => anyhow::bail!("expected a boolean value, got `{value}`"),
    }
}

fn write_workspace_config(state: &AppState, path: &PathBuf) -> Result<()> {
    fs::write(path, toml::to_string_pretty(&state.config)?)?;
    Ok(())
}

fn default_keybindings_contents() -> &'static str {
    "submit = \"enter\"\nclear_input = \"esc\"\nexit = \"ctrl+c\"\n"
}

fn default_permissions_contents(resources: &LoadedResources) -> String {
    let mut text = String::from("[tools]\n");
    for tool in &resources.tools {
        let key = tool.value.id.replace('-', "_");
        let _ = writeln!(&mut text, "{key} = \"ask\"");
    }
    if resources.tools.is_empty() {
        text.push_str("bash = \"ask\"\n");
    }
    text
}

fn default_hooks_contents() -> &'static str {
    "id: tool-end\n\
event: tool_end\n\
command: echo \"$PUFFER_TOOL_ID:$PUFFER_TOOL_SUCCESS\"\n"
}
