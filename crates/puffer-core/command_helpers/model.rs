use super::{emit_system, persist_user_model_selection};
use crate::AppState;
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
        return apply_default_model_selection(state, session_store);
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
            persist_user_model_selection(state)?;
            emit_system(
                state,
                session_store,
                format!("Active model set to {}/{}.", model.provider, model.id),
            )
        }
        Err(message) => emit_system(state, session_store, message),
    }
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

fn render_current_model_summary(state: &AppState) -> String {
    let current = state.current_model.as_deref().unwrap_or("<unset>");
    let base = state.config.default_model.as_deref().unwrap_or("<unset>");
    format!(
        "Current model: {current}\nDefault model: {base}\nEffort level: {}\nFast mode: {}",
        state.effort_level,
        if state.fast_mode { "on" } else { "off" }
    )
}

fn apply_default_model_selection(state: &mut AppState, session_store: &SessionStore) -> Result<()> {
    if let Some(default_model) = state.config.default_model.clone() {
        state.current_model = Some(default_model.clone());
        state.current_provider = default_model
            .split_once('/')
            .map(|(provider, _)| provider.to_string())
            .or_else(|| state.config.default_provider.clone())
            .or_else(|| state.current_provider.clone());
        return emit_system(
            state,
            session_store,
            format!("Active model reset to {}.", default_model),
        );
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
