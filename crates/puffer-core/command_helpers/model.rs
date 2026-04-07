use super::{emit_system, persist_user_model_selection, persist_user_settings};
use crate::{
    default_effort_level, effort_level_is_supported, normalized_effort_level,
    provider_preference_family, supported_effort_levels, AppState, ModelPreferenceFamily,
};
use anyhow::Result;
use puffer_provider_registry::{AuthStore, ModelDescriptor, ProviderRegistry};
use puffer_session_store::SessionStore;

/// Handles `/model` selection, refresh, and reset flows.
pub(crate) fn handle_model_command(
    state: &mut AppState,
    providers: &mut ProviderRegistry,
    auth_store: &mut AuthStore,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let trimmed_args = args.trim();
    if trimmed_args.is_empty() {
        let Some(provider_id) = state.current_provider.as_deref() else {
            return emit_system(
                state,
                session_store,
                "No provider is selected. Use onboarding or /login first.".to_string(),
            );
        };
        let discovery_error = providers
            .discover_and_merge_provider(provider_id, auth_store)
            .err();
        let Some(provider) = providers.provider(provider_id) else {
            return emit_system(
                state,
                session_store,
                format!("Selected provider {provider_id} is no longer available."),
            );
        };
        let models = provider
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let current = state.current_model.as_deref().unwrap_or("<unset>");
        let mut message =
            format!("Current model: {current}\nAvailable models for {provider_id}: {models}");
        if let Some(error) = discovery_error {
            message.push_str(&format!("\nmodel refresh failed: {error}"));
        }
        return emit_system(state, session_store, message);
    }
    if matches!(trimmed_args, "help" | "--help" | "-h") {
        return emit_system(state, session_store, model_help_text().to_string());
    }
    if matches!(trimmed_args, "show" | "current" | "info") {
        return emit_system(state, session_store, render_current_model_summary(state));
    }
    if trimmed_args == "default" {
        return apply_default_model_selection(state, providers, session_store);
    }
    if trimmed_args == "refresh" {
        let Some(provider_id) = state.current_provider.as_deref().map(str::to_string) else {
            return emit_system(
                state,
                session_store,
                "No provider is selected. Use onboarding or /login first.".to_string(),
            );
        };
        providers.discover_and_merge_provider(&provider_id, auth_store)?;
        let provider = providers
            .provider(&provider_id)
            .ok_or_else(|| anyhow::anyhow!("provider {} not found", provider_id))?;
        return emit_system(
            state,
            session_store,
            format!(
                "Refreshed models for {}.\nmodels={}",
                provider.id,
                provider.models.len()
            ),
        );
    }
    if let Some(provider_id) = trimmed_args.strip_prefix("refresh ") {
        providers.discover_and_merge_provider(provider_id.trim(), auth_store)?;
        let provider = providers
            .provider(provider_id.trim())
            .ok_or_else(|| anyhow::anyhow!("provider {} not found", provider_id.trim()))?;
        return emit_system(
            state,
            session_store,
            format!(
                "Refreshed models for {}.\nmodels={}",
                provider.id,
                provider.models.len()
            ),
        );
    }

    let _ = providers.discover_and_merge_all(auth_store);
    match resolve_model_selection(providers, state.current_provider.as_deref(), trimmed_args) {
        Ok(model) => {
            state.current_provider = Some(model.provider.clone());
            state.current_model = Some(format!("{}/{}", model.provider, model.id));
            state.config.default_provider = Some(model.provider.clone());
            state.config.default_model = Some(format!("{}/{}", model.provider, model.id));
            let effort_adjustment =
                normalize_effort_for_provider(state, providers, &model.provider);
            persist_user_model_selection(state)?;
            let mut message = format!("Active model set to {}/{}.", model.provider, model.id);
            if let Some((previous, adjusted)) = effort_adjustment {
                message.push_str(&format!(
                    "\nEffort level adjusted from {previous} to {adjusted} for {}.",
                    model.provider
                ));
            }
            emit_system(state, session_store, message)
        }
        Err(message) => emit_system(state, session_store, message),
    }
}

/// Handles `/effort` status and provider-aware effort updates.
pub(crate) fn handle_effort_command(
    state: &mut AppState,
    providers: &ProviderRegistry,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let family = active_provider_family(state, providers);
    let trimmed_args = args.trim();
    if matches!(trimmed_args, "" | "show" | "current" | "info" | "status") {
        return emit_system(
            state,
            session_store,
            render_current_effort_summary(state, providers),
        );
    }
    if matches!(trimmed_args, "help" | "--help" | "-h") {
        return emit_system(state, session_store, effort_help_text(family));
    }

    let normalized = trimmed_args.to_ascii_lowercase();
    if normalized == "auto" || normalized == "unset" {
        state.effort_level = "auto".to_string();
        state.config.effort_level = None;
        persist_user_settings(state)?;
        return emit_system(
            state,
            session_store,
            format!(
                "Effort level set to auto.\nCurrent provider default: {}",
                default_effort_level(family)
            ),
        );
    }

    if !effort_level_is_supported(family, &normalized) {
        let options = supported_effort_levels(family).join(", ");
        state.effort_level = "auto".to_string();
        state.config.effort_level = None;
        persist_user_settings(state)?;
        return emit_system(
            state,
            session_store,
            format!("Invalid effort level `{trimmed_args}`. Valid options are: {options}"),
        );
    }

    state.effort_level = normalized.clone();
    state.config.effort_level = Some(normalized.clone());
    persist_user_settings(state)?;
    emit_system(
        state,
        session_store,
        format!(
            "Effort level set to {normalized}.\nProvider family: {}",
            effort_family_label(family)
        ),
    )
}

/// Handles `/fast` status, help text, and direct toggles.
pub(crate) fn handle_fast_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let trimmed_args = args.trim().to_ascii_lowercase();
    if trimmed_args.is_empty()
        || matches!(
            trimmed_args.as_str(),
            "show" | "current" | "info" | "status"
        )
    {
        return emit_system(
            state,
            session_store,
            format!(
                "Fast mode is {}.",
                if state.fast_mode { "on" } else { "off" }
            ),
        );
    }
    if matches!(trimmed_args.as_str(), "help" | "--help" | "-h") {
        return emit_system(
            state,
            session_store,
            "Usage: /fast [on|off|toggle|status]\nTurns the current session's fast-mode preference on or off."
                .to_string(),
        );
    }
    match trimmed_args.as_str() {
        "toggle" => state.fast_mode = !state.fast_mode,
        "on" | "true" | "1" => state.fast_mode = true,
        "off" | "false" | "0" => state.fast_mode = false,
        other => {
            return emit_system(
                state,
                session_store,
                format!("Invalid fast-mode setting `{other}`. Use on, off, toggle, or status."),
            );
        }
    }
    state.config.fast_mode = state.fast_mode;
    persist_user_settings(state)?;
    emit_system(
        state,
        session_store,
        format!(
            "Fast mode is now {}.",
            if state.fast_mode { "on" } else { "off" }
        ),
    )
}

/// Applies a provider/model/effort/fast-mode preference bundle and persists it.
pub(crate) fn apply_model_preferences(
    state: &mut AppState,
    provider_id: &str,
    model_id: &str,
    effort: &str,
    fast_mode: bool,
) -> Result<()> {
    state.current_provider = Some(provider_id.to_string());
    state.current_model = Some(format!("{provider_id}/{model_id}"));
    state.config.default_provider = Some(provider_id.to_string());
    state.config.default_model = Some(format!("{provider_id}/{model_id}"));
    state.effort_level = effort.to_string();
    state.config.effort_level = if effort == "auto" {
        None
    } else {
        Some(effort.to_string())
    };
    state.fast_mode = fast_mode;
    state.config.fast_mode = fast_mode;
    persist_user_settings(state)
}

fn model_help_text() -> &'static str {
    "Usage: /model [show|current|info|default|refresh [provider]|<model>|<provider/model>]\n\
Examples:\n\
- /model\n\
- /model current\n\
- /model default\n\
- /model gpt-5\n\
- /model <provider/model>\n\
- /model anthropic/claude-sonnet-4-5"
}

fn render_current_effort_summary(state: &AppState, providers: &ProviderRegistry) -> String {
    let family = active_provider_family(state, providers);
    if state.config.effort_level.is_none() {
        return format!(
            "Current effort level: auto\nProvider default: {}",
            default_effort_level(family)
        );
    }
    format!(
        "Current effort level: {}\nSupported values for {}: {}, auto",
        state.effort_level,
        effort_family_label(family),
        supported_effort_levels(family).join(", ")
    )
}

fn effort_help_text(family: ModelPreferenceFamily) -> String {
    format!(
        "Usage: /effort [{}|auto]\nCurrent provider family: {}",
        supported_effort_levels(family).join("|"),
        effort_family_label(family)
    )
}

fn effort_family_label(family: ModelPreferenceFamily) -> &'static str {
    match family {
        ModelPreferenceFamily::Anthropic => "Anthropic-style models",
        ModelPreferenceFamily::OpenAi => "OpenAI-style models",
        ModelPreferenceFamily::Other => "generic models",
    }
}

fn active_provider_family(state: &AppState, providers: &ProviderRegistry) -> ModelPreferenceFamily {
    state
        .current_provider
        .as_deref()
        .map(|provider_id| provider_preference_family(providers, provider_id))
        .unwrap_or(ModelPreferenceFamily::Other)
}

fn render_current_model_summary(state: &AppState) -> String {
    let current = state.current_model.as_deref().unwrap_or("<unset>");
    let base = state.config.default_model.as_deref().unwrap_or("<unset>");
    format!(
        "Current model: {current}\nDefault model: {base}\nEffort level: {}\nFast mode: {}",
        state.effort_level,
        if state.fast_mode { "on" } else { "off" }
    )
}

fn normalize_effort_for_provider(
    state: &mut AppState,
    providers: &ProviderRegistry,
    provider_id: &str,
) -> Option<(String, String)> {
    let family = provider_preference_family(providers, provider_id);
    let previous = state.effort_level.clone();
    let normalized = match state.config.effort_level.as_deref() {
        Some(configured) => normalized_effort_level(family, configured),
        None => default_effort_level(family).to_string(),
    };
    if normalized == previous {
        return None;
    }
    state.effort_level = normalized.clone();
    if state.config.effort_level.is_some() {
        state.config.effort_level = Some(normalized.clone());
    }
    Some((previous, normalized))
}

fn apply_default_model_selection(
    state: &mut AppState,
    providers: &ProviderRegistry,
    session_store: &SessionStore,
) -> Result<()> {
    if let Some(default_model) = state.config.default_model.clone() {
        state.current_model = Some(default_model.clone());
        state.current_provider = default_model
            .split_once('/')
            .map(|(provider, _)| provider.to_string())
            .or_else(|| state.config.default_provider.clone())
            .or_else(|| state.current_provider.clone());
        let mut message = format!("Active model reset to {}.", default_model);
        if let Some(provider_id) = state.current_provider.clone() {
            if let Some((previous, adjusted)) =
                normalize_effort_for_provider(state, providers, &provider_id)
            {
                message.push_str(&format!(
                    "\nEffort level adjusted from {previous} to {adjusted} for {provider_id}."
                ));
                persist_user_settings(state)?;
            }
        }
        return emit_system(state, session_store, message);
    }
    state.current_model = None;
    state.current_provider = state.config.default_provider.clone();
    emit_system(
        state,
        session_store,
        "Cleared the active model override.".to_string(),
    )
}

fn resolve_model_selection<'a>(
    providers: &'a ProviderRegistry,
    active_provider: Option<&str>,
    requested: &str,
) -> std::result::Result<&'a ModelDescriptor, String> {
    let requested = requested.trim();
    if requested.is_empty() {
        return Err("No model was provided.".to_string());
    }
    if let Some(model) = providers.resolve_model(requested) {
        return Ok(model);
    }
    if let Some((provider_id, model_id)) = requested.split_once('/') {
        let Some(provider) = providers.provider(provider_id) else {
            return Err(format!("Unknown provider `{provider_id}`."));
        };
        return find_model_within_provider(provider.models.as_slice(), model_id).ok_or_else(|| {
            format!(
                "Unknown model {} for provider {}.",
                model_id.trim(),
                provider_id.trim()
            )
        });
    }
    if let Some(provider_id) = active_provider {
        if let Some(provider) = providers.provider(provider_id) {
            if let Some(model) = find_model_within_provider(provider.models.as_slice(), requested) {
                return Ok(model);
            }
        }
    }
    let mut matching = providers
        .models()
        .filter(|model| model_matches_request(model, requested))
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.id.cmp(&right.id))
    });
    matching
        .into_iter()
        .next()
        .ok_or_else(|| format!("Model '{}' not found.", requested))
}

fn find_model_within_provider<'a>(
    models: &'a [ModelDescriptor],
    requested: &str,
) -> Option<&'a ModelDescriptor> {
    models
        .iter()
        .find(|model| model_matches_request(model, requested))
}

fn model_matches_request(model: &ModelDescriptor, requested: &str) -> bool {
    let requested = requested.trim().to_ascii_lowercase();
    let id = model.id.to_ascii_lowercase();
    let display = model.display_name.to_ascii_lowercase();
    id == requested
        || display == requested
        || alias_matches_model(&requested, &id, &display)
        || id.contains(&requested)
        || display.contains(&requested)
}

fn alias_matches_model(alias: &str, id: &str, display: &str) -> bool {
    matches!(alias, "sonnet" | "opus" | "haiku") && (id.contains(alias) || display.contains(alias))
}
