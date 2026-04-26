//! `WorkflowRegister` workflow tool for native Puffer workflow definitions.

use crate::runtime::subscription_manager;
use crate::AppState;
use anyhow::{Context, Result};
use puffer_config::ConfigPaths;
use puffer_subscriptions::{ActionSpec, PrefilterSpec, SubscriptionSpec, SubscriptionStatus};
use puffer_workflow::{RegisterOptions, TriggerSpec, WorkflowDefinition, WorkflowStore};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use time::OffsetDateTime;

#[derive(Debug, Deserialize)]
struct RegisterInput {
    workflow: Value,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    trigger: Option<TriggerSpec>,
}

/// Executes `WorkflowRegister` by validating and persisting a workflow definition.
pub fn execute_workflow_register(
    _state: &mut AppState,
    cwd: &Path,
    input: Value,
) -> Result<String> {
    let parsed: RegisterInput =
        serde_json::from_value(input).context("invalid WorkflowRegister input")?;
    let paths = ConfigPaths::discover(cwd);
    let store = WorkflowStore::new(&paths.workspace_config_dir);
    let definition = store.register_json(
        parsed.workflow,
        RegisterOptions {
            slug: parsed.slug,
            trigger: parsed.trigger,
        },
    )?;
    sync_subscription_trigger(&definition)?;
    Ok(serde_json::to_string_pretty(&definition)?)
}

fn sync_subscription_trigger(definition: &WorkflowDefinition) -> Result<()> {
    let TriggerSpec::Subscription {
        source_topic,
        pattern,
        classify_prompt,
    } = &definition.trigger
    else {
        return Ok(());
    };
    let manager = subscription_manager()?;
    let id = format!("workflow-{}", definition.slug);
    if manager.store().get(&id).is_some() {
        manager.store().delete(&id)?;
    }
    let spec = SubscriptionSpec {
        id,
        description: format!("Run workflow `{}`", definition.slug),
        source_topic: source_topic.clone(),
        status: if definition.enabled {
            SubscriptionStatus::Enabled
        } else {
            SubscriptionStatus::Paused
        },
        prefilter: pattern.as_ref().map(|pattern| PrefilterSpec::Regex {
            pattern: pattern.clone(),
            case_insensitive: true,
        }),
        classify_prompt: classify_prompt.clone(),
        classify_model: None,
        action: ActionSpec::RunWorkflow {
            slug: definition.slug.clone(),
        },
        created_at_ms: OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000,
    };
    manager.store().create(spec)?;
    Ok(())
}
