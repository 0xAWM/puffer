pub(crate) mod artifacts;
mod auth;
mod common;
mod config;
mod ecosystem;
mod plugins;
pub(crate) mod prompt;
mod session;
mod tasks;

pub(crate) use artifacts::{handle_copy_command, handle_export_command};
pub(crate) use auth::render_login_guidance;
pub(crate) use common::{
    describe_context, describe_git_diff, emit_system, execute_skill_command, list_skills,
    record_command_checkpoint, rewind_transcript, run_doctor, terminal_setup_advice,
};
pub(crate) use config::{
    handle_config_command, handle_hooks_command, handle_keybindings_command,
    handle_permissions_command, handle_sandbox_command, persist_user_model_selection,
    reload_config_from_disk, render_config_summary, render_hooks_summary, render_permissions_panel,
};
pub use ecosystem::McpActionEntry;
pub(crate) use ecosystem::{
    handle_agents_command, handle_ide_command, handle_mcp_command, reload_resources_from_disk,
    render_mcp_actions, render_mcp_summary,
};
pub use plugins::PluginActionEntry;
pub(crate) use plugins::{
    handle_plugin_command, reload_plugins_summary, render_plugin_actions, render_plugin_summary,
};
pub use session::SessionOverlayView;
pub(crate) use session::{
    append_tool_invocations, handle_memory_command, handle_remote_control_command,
    handle_remote_env_command, handle_session_command, render_memory_panel, render_session_overlay,
    render_session_panel,
};
pub(crate) use tasks::handle_tasks_command;
