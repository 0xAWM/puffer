use crate::state::AuthPickerEntry;
use crate::{ModelPickerEntry, OverlayState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OverlayRow {
    pub(super) text: String,
    pub(super) selected: bool,
}

pub(super) fn short_id(value: &str) -> String {
    value.chars().take(8).collect()
}

pub(super) fn overlay_title(overlay: &OverlayState) -> &'static str {
    match overlay {
        OverlayState::SessionPicker { .. } => "Resume Session",
        OverlayState::AgentPicker { .. } => "Select Agent",
        OverlayState::ModelPicker { .. } => "Select Model",
        OverlayState::EffortPicker { .. } => "Select Effort",
        OverlayState::FastModePicker { .. } => "Fast Mode",
        OverlayState::ProviderPicker { .. } => "Select Provider",
        OverlayState::AuthPicker { .. } => "Select Login Method",
        OverlayState::ApiKeyPrompt { .. } => "Enter API Key",
        OverlayState::LoginPicker { .. } => "Select Provider",
        OverlayState::LogoutPicker { .. } => "Logout Provider",
        OverlayState::ThemePicker { .. } => "Select Theme",
        OverlayState::CommandPicker { .. } => "Select Command",
        OverlayState::PermissionPrompt { .. } => "Permission Needed",
        OverlayState::Session(..) => "Session",
        OverlayState::Status(..) => "Status",
        OverlayState::Text(..) => "Panel",
        OverlayState::OnboardingTheme { .. } => "Select Theme",
        OverlayState::OnboardingProvider { .. } => "Select Provider",
        OverlayState::OnboardingAuth { .. } => "Select Login Method",
        OverlayState::OnboardingModel { .. } => "Select Model",
        OverlayState::OnboardingApiKey { .. } => "Enter API Key",
        OverlayState::Usage(..) => "Usage",
    }
}

pub(super) fn overlay_rows(overlay: &OverlayState) -> Vec<OverlayRow> {
    match overlay {
        OverlayState::SessionPicker {
            sessions,
            selection,
        } => sessions
            .iter()
            .enumerate()
            .map(|(index, session)| OverlayRow {
                selected: index == *selection,
                text: format!(
                    "{}  {}",
                    short_id(&session.id.to_string()),
                    session.display_name.as_deref().unwrap_or("<unnamed>")
                ),
            })
            .collect(),
        OverlayState::AgentPicker { entries, selection }
        | OverlayState::ModelPicker {
            entries, selection, ..
        }
        | OverlayState::EffortPicker {
            entries, selection, ..
        }
        | OverlayState::FastModePicker {
            entries, selection, ..
        }
        | OverlayState::ProviderPicker {
            entries, selection, ..
        }
        | OverlayState::LoginPicker { entries, selection }
        | OverlayState::LogoutPicker { entries, selection }
        | OverlayState::ThemePicker { entries, selection }
        | OverlayState::CommandPicker {
            entries, selection, ..
        }
        | OverlayState::OnboardingTheme { entries, selection }
        | OverlayState::OnboardingProvider { entries, selection }
        | OverlayState::OnboardingAuth {
            entries, selection, ..
        }
        | OverlayState::OnboardingModel {
            entries, selection, ..
        } => entries
            .iter()
            .enumerate()
            .map(|(index, entry)| OverlayRow {
                selected: index == *selection,
                text: render_model_entry(entry),
            })
            .collect(),
        OverlayState::AuthPicker {
            entries, selection, ..
        } => entries
            .iter()
            .enumerate()
            .map(|(index, entry)| OverlayRow {
                selected: index == *selection,
                text: render_auth_entry(entry),
            })
            .collect(),
        OverlayState::ApiKeyPrompt { value, .. } => vec![
            OverlayRow {
                selected: false,
                text: "Paste an API key and press Enter.".to_string(),
            },
            OverlayRow {
                selected: true,
                text: format!("key  {}", masked_secret(value)),
            },
        ],
        OverlayState::OnboardingApiKey { input, .. } => vec![
            OverlayRow {
                selected: false,
                text: "Paste an API key and press Enter.".to_string(),
            },
            OverlayRow {
                selected: true,
                text: format!("key  {}", masked_secret(input)),
            },
        ],
        OverlayState::PermissionPrompt { .. }
        | OverlayState::Session(..)
        | OverlayState::Status(..)
        | OverlayState::Text(..)
        | OverlayState::Usage(..) => Vec::new(),
    }
}

pub(super) fn masked_secret(value: &str) -> String {
    if value.is_empty() {
        return "<empty>".to_string();
    }
    "*".repeat(value.chars().count().min(32))
}

pub(super) fn render_model_entry(entry: &ModelPickerEntry) -> String {
    if entry.description.trim().is_empty() {
        return entry.selector.clone();
    }
    if entry
        .selector
        .eq_ignore_ascii_case(entry.description.trim())
    {
        return entry.description.clone();
    }
    format!("{}  {}", entry.selector, entry.description)
}

fn render_auth_entry(entry: &AuthPickerEntry) -> String {
    format!("{}  {}", entry.label, entry.description)
}
