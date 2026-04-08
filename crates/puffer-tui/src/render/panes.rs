use super::prompt_border_style;
use puffer_core::{AppState, CommandSpec};
use puffer_resources::LoadedResources;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

const HELP_WIDE_BREAKPOINT: u16 = 120;
const FEATURED_HELP_COMMANDS: [&str; 10] = [
    "help", "review", "resume", "login", "model", "agents", "usage", "doctor", "config", "skills",
];

/// Renders the help pane using a responsive command summary layout.
pub(super) fn render_help_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    commands: &[CommandSpec],
    resources: &LoadedResources,
) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(prompt_border_style(state));
    frame.render_widget(&block, area);
    let inner = block.inner(area);
    let content = Rect {
        x: inner.x.saturating_add(1),
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: inner.height,
    };

    if content.width >= HELP_WIDE_BREAKPOINT && content.height >= 20 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(62),
                Constraint::Length(1),
                Constraint::Percentage(38),
            ])
            .split(content);
        frame.render_widget(
            Paragraph::new(Text::from(help_command_lines(
                commands,
                columns[0].width,
                columns[0].height,
            )))
            .wrap(Wrap { trim: false }),
            columns[0],
        );
        frame.render_widget(
            Paragraph::new(Text::from(vertical_separator(columns[1].height))),
            columns[1],
        );
        frame.render_widget(
            Paragraph::new(Text::from(help_side_lines(resources))).wrap(Wrap { trim: false }),
            columns[2],
        );
    } else {
        frame.render_widget(
            Paragraph::new(Text::from(help_compact_lines(
                commands,
                resources,
                content.width,
                content.height,
            )))
            .wrap(Wrap { trim: false }),
            content,
        );
    }
}

fn help_command_lines(commands: &[CommandSpec], width: u16, height: u16) -> Vec<Line<'static>> {
    let row_width = usize::from(width.saturating_sub(1));
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Puffer Code v{}  Supported commands", env!("CARGO_PKG_VERSION")),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(
            "Core commands for common coding workflows. Use /skills for loaded skill commands and /skill:<name> aliases.",
        ),
        Line::default(),
    ];
    let entries = featured_help_commands(commands);
    let column_width = row_width.saturating_sub(3) / 2;
    let row_capacity = usize::from(height.saturating_sub(6)).max(1);
    let pairs = entries.chunks(2).take(row_capacity);
    for pair in pairs {
        let left = format_help_entry(pair[0], column_width);
        let right = pair
            .get(1)
            .map(|command| format_help_entry(command, column_width))
            .unwrap_or_default();
        lines.push(Line::from(format!(
            "{:<left_width$}   {}",
            left,
            right,
            left_width = column_width
        )));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Esc to cancel",
        Style::default().add_modifier(Modifier::DIM),
    )));
    lines
}

fn help_side_lines(resources: &LoadedResources) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "Tips to get started",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Review changes, ask a question, or type /."),
        Line::from("/review inspects the current worktree."),
        Line::from("/resume opens the saved-session picker."),
        Line::from("/login changes provider auth."),
        Line::from(Span::styled(
            "Resources",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(
            "prompts {} · tools {} · skills {}",
            resources.prompts.len(),
            resources.tools.len(),
            resources.skills.len(),
        )),
        Line::from(format!(
            "plugins {} · mcp {} · ides {}",
            resources.plugins.len(),
            resources.mcp_servers.len(),
            resources.ides.len(),
        )),
        Line::from(Span::styled(
            "? for shortcuts",
            Style::default().add_modifier(Modifier::DIM),
        )),
    ]
}

fn help_compact_lines(
    commands: &[CommandSpec],
    resources: &LoadedResources,
    width: u16,
    height: u16,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "Supported commands",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    let entry_width = usize::from(width);
    let reserve_footer = if height >= 9 { 2 } else { 1 };
    let max_entries = usize::from(height.saturating_sub(reserve_footer + 1)).max(1);
    for command in featured_help_commands(commands)
        .into_iter()
        .take(max_entries)
    {
        lines.push(Line::from(format_help_entry(command, entry_width)));
    }
    if height >= 9 {
        lines.push(Line::from(Span::styled(
            format!(
                "Resources: prompts {} · tools {} · skills {}",
                resources.prompts.len(),
                resources.tools.len(),
                resources.skills.len(),
            ),
            Style::default().add_modifier(Modifier::DIM),
        )));
    }
    lines.push(Line::from(Span::styled(
        "Esc to cancel",
        Style::default().add_modifier(Modifier::DIM),
    )));
    lines
}

fn featured_help_commands(commands: &[CommandSpec]) -> Vec<&CommandSpec> {
    let visible = commands
        .iter()
        .filter(|command| !command.hidden)
        .collect::<Vec<_>>();
    let mut featured = FEATURED_HELP_COMMANDS
        .iter()
        .filter_map(|name| {
            visible
                .iter()
                .copied()
                .find(|command| command.name == *name)
        })
        .collect::<Vec<_>>();
    if featured.is_empty() {
        featured.extend(visible.into_iter().take(10));
    }
    featured
}

fn format_help_entry(command: &CommandSpec, max_width: usize) -> String {
    if max_width <= 8 {
        return truncate(&format!("/{}", command.name), max_width);
    }
    let name = format!("/{}", command.name);
    let name_width = 12.min(max_width.saturating_sub(2));
    let description_width = max_width.saturating_sub(name_width + 2);
    format!(
        "{:<name_width$} {}",
        truncate(&name, name_width),
        truncate(&command.description, description_width)
    )
}

#[cfg(test)]
mod tests {
    use super::featured_help_commands;
    use puffer_core::{CommandKind, CommandSpec};

    #[test]
    fn featured_help_commands_skip_hidden_entries() {
        let commands = vec![
            CommandSpec {
                name: "help".to_string(),
                aliases: Vec::new(),
                description: "Visible".to_string(),
                argument_hint: None,
                kind: CommandKind::Local,
                hidden: false,
            },
            CommandSpec {
                name: "terminal-setup".to_string(),
                aliases: Vec::new(),
                description: "Hidden".to_string(),
                argument_hint: None,
                kind: CommandKind::Local,
                hidden: true,
            },
        ];

        let featured = featured_help_commands(&commands);
        assert_eq!(featured.len(), 1);
        assert_eq!(featured[0].name, "help");
    }
}

fn vertical_separator(height: u16) -> Vec<Line<'static>> {
    (0..height)
        .map(|_| {
            Line::from(Span::styled(
                "│",
                Style::default().add_modifier(Modifier::DIM),
            ))
        })
        .collect()
}

fn truncate(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }
    let prefix = value.chars().take(max_chars - 3).collect::<String>();
    format!("{prefix}...")
}
