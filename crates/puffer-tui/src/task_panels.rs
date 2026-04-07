use anyhow::Result;
use puffer_core::{render_tasks_panel_text, AppState};

use crate::text_overlay::TextOverlay;
use crate::OverlayState;

/// Builds the inline `/tasks` text overlay for read-only subcommands when available.
pub(crate) fn task_text_overlay(state: &AppState, args: &str) -> Result<Option<OverlayState>> {
    let mut preview_state = state.clone();
    Ok(render_tasks_panel_text(&mut preview_state, args)?
        .map(|text| TextOverlay::open(task_panel_title(args), text)))
}

fn task_panel_title(args: &str) -> &'static str {
    match args.split_whitespace().next().unwrap_or_default() {
        "agents" => "Background Agents",
        "teams" => "Workflow Teams",
        "worktrees" => "Worktrees",
        "todos" => "Todos",
        "path" => "Task Paths",
        "output" => "Task Output",
        _ => "Background Tasks",
    }
}
