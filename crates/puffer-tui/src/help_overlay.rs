use crate::text_overlay::TextOverlay;
use puffer_core::{supported_commands, CommandSpec};
use puffer_resources::LoadedResources;
use std::fmt::Write as _;

const FEATURED_HELP_COMMANDS: [&str; 10] = [
    "help", "review", "resume", "login", "model", "agents", "usage", "doctor", "config",
    "skills",
];
const DOCS_URL: &str = "https://code.claude.com/docs/en/overview";

/// Builds the dedicated `/help` overlay used by the interactive TUI.
pub(crate) fn open_help_overlay(resources: &LoadedResources) -> crate::OverlayState {
    TextOverlay::open("Help", build_help_body(&supported_commands(), resources))
}

fn build_help_body(commands: &[CommandSpec], resources: &LoadedResources) -> String {
    let mut body = String::new();
    let _ = writeln!(&mut body, "Puffer Code v{}", env!("CARGO_PKG_VERSION"));
    body.push('\n');
    body.push_str(
        "Puffer Code understands your codebase, makes edits with your permission, and executes commands right from your terminal.\n",
    );
    body.push('\n');
    body.push_str("Shortcuts\n");
    body.push_str("/       open slash-command suggestions\n");
    body.push_str("Tab     complete the highlighted slash command\n");
    body.push_str("Enter   submit the current prompt\n");
    body.push_str("Up/Down move through slash suggestions or scroll output\n");
    body.push_str("Esc     close panels, clear the prompt, or interrupt a running turn\n");
    body.push_str("Ctrl+O  toggle tool details\n");
    body.push_str("Ctrl+C  exit\n");
    body.push('\n');
    body.push_str("Featured Commands\n");
    for command in featured_help_commands(commands) {
        let _ = writeln!(
            &mut body,
            "/{:<16} {}",
            command.name, command.description
        );
    }
    body.push('\n');
    body.push_str("All Commands\n");
    let mut sorted = commands.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|command| command.name);
    for command in sorted {
        let _ = writeln!(
            &mut body,
            "/{:<16} {}",
            command.name, command.description
        );
    }
    body.push('\n');
    let _ = writeln!(
        &mut body,
        "Resources\nprompts {} · tools {} · skills {}\nplugins {} · mcp {} · ides {}",
        resources.prompts.len(),
        resources.tools.len(),
        resources.skills.len(),
        resources.plugins.len(),
        resources.mcp_servers.len(),
        resources.ides.len()
    );
    body.push('\n');
    let _ = writeln!(&mut body, "Run /skills to list loaded /skill:<name> entries.");
    let _ = writeln!(&mut body, "For more help: {DOCS_URL}");
    body
}

fn featured_help_commands<'a>(commands: &'a [CommandSpec]) -> Vec<&'a CommandSpec> {
    let mut featured = FEATURED_HELP_COMMANDS
        .iter()
        .filter_map(|name| commands.iter().find(|command| command.name == *name))
        .collect::<Vec<_>>();
    if featured.is_empty() {
        featured.extend(commands.iter().take(10));
    }
    featured
}
