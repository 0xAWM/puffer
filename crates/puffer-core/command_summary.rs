use crate::{AppState, CommandSpec, MessageRole};
use puffer_provider_registry::{AuthStore, ProviderRegistry};
use puffer_resources::{mascot_by_id, LoadedResources};
use std::fmt::Write as _;

/// Renders the most recent recorded tool and shell tasks.
pub(crate) fn render_task_summary(state: &AppState) -> String {
    if state.tasks().is_empty() {
        return "Tasks:\nNo recorded shell or tool tasks yet.".to_string();
    }

    let mut text = String::from("Tasks:\n");
    for task in state.tasks().iter().rev().take(10) {
        let status = match task.status {
            crate::TaskStatus::Completed => "completed",
            crate::TaskStatus::Failed => "failed",
        };
        let _ = writeln!(
            &mut text,
            "#{} {} [{}]\n{}",
            task.id, task.label, status, task.detail
        );
    }
    text.trim_end().to_string()
}

/// Renders a lightweight local cost-style summary for the active session.
pub(crate) fn render_cost_summary(state: &AppState) -> String {
    let elapsed_ms = now_ms().saturating_sub(state.session.created_at_ms);
    let assistant_messages = state
        .transcript
        .iter()
        .filter(|message| message.role == MessageRole::Assistant)
        .count();
    let user_messages = state
        .transcript
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .count();
    let tool_invocations = state
        .transcript
        .iter()
        .filter(|message| message.role == MessageRole::System && message.text.starts_with("Tool "))
        .count();
    format!(
        "Session cost summary:\nelapsed_ms={elapsed_ms}\nuser_messages={user_messages}\nassistant_messages={assistant_messages}\ntool_invocations={tool_invocations}\nrecorded_tasks={}\nestimated_cost_usd=unavailable",
        state.tasks().len()
    )
}

/// Renders a combined usage summary across runtime state and loaded resources.
pub(crate) fn render_usage_summary(
    state: &AppState,
    commands: &[CommandSpec],
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    auth_store: &AuthStore,
) -> String {
    let providers_with_discovery = providers
        .provider_entries()
        .filter(|provider| provider.descriptor.discovery.is_some())
        .count();
    format!(
        "Usage summary:\ncommands={}\nmessages={}\nproviders={}\nmodels={}\nauthed_providers={}\nproviders_with_discovery={}\nprompts={}\ntools={}\nskills={}\nplugins={}\nhooks={}\nactive_provider={}\nactive_model={}",
        commands.len(),
        state.transcript.len(),
        providers.providers().count(),
        providers.models().count(),
        auth_store.provider_ids().count(),
        providers_with_discovery,
        resources.prompts.len(),
        resources.tools.len(),
        resources.skills.len(),
        resources.plugins.len(),
        resources.hooks.len(),
        state.current_provider.as_deref().unwrap_or("<unset>"),
        state.current_model.as_deref().unwrap_or("<unset>"),
    )
}

/// Renders the current mascot summary, including any loaded introduction text.
pub(crate) fn render_buddy_summary(state: &AppState, resources: &LoadedResources) -> String {
    let intro = mascot_by_id(resources, &state.config.mascot.id)
        .map(|mascot| mascot.introduction.as_str())
        .unwrap_or("No mascot resource is currently loaded for this session.");
    format!(
        "{} is on duty.\nmascot_id={}\nenabled={}\n{}",
        state.config.mascot.display_name,
        state.config.mascot.id,
        state.config.mascot.enabled,
        intro
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
