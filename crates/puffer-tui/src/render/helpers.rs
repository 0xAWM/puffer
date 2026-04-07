use puffer_core::{AppState, MessageRole};
use ratatui::text::Line;

/// Builds the horizontal separator line used above the composer area.
pub(super) fn separator_line(width: u16) -> Line<'static> {
    Line::from("─".repeat(usize::from(width)))
}

/// Returns true when the transcript is currently showing the help pane.
pub(super) fn help_pane_active(
    state: &AppState,
    active_overlay: &Option<crate::OverlayState>,
) -> bool {
    active_overlay.is_none()
        && state.transcript.last().is_some_and(|message| {
            message.role == MessageRole::System && message.text.starts_with("Supported commands:")
        })
}
