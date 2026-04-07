/// Describes one slash-command action exposed in an interactive TUI picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandActionEntry {
    /// The slash command executed when the action is selected.
    pub command: String,
    /// The row description shown in the interactive picker.
    pub description: String,
}
