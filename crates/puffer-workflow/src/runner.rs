use crate::{
    PipelineNode, WorkflowDefinition, WorkflowRun, WorkflowRunNode, WorkflowRunStatus,
    WorkflowStore,
};
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use time::OffsetDateTime;
use uuid::Uuid;

/// Context passed to an agent executor for one node.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Workflow slug.
    pub workflow_slug: String,
    /// Run id.
    pub run_id: String,
    /// Node id.
    pub node_id: String,
    /// Agent hint.
    pub agent: Option<String>,
    /// Model hint.
    pub model: Option<String>,
    /// Tool allowlist.
    pub tools: Vec<String>,
    /// Environment values.
    pub env: BTreeMap<String, String>,
    /// Optional workflow working directory.
    pub working_dir: Option<String>,
    /// Interpolated prompt.
    pub prompt: String,
}

/// Result of executing one agent node.
#[derive(Debug, Clone)]
pub struct AgentExecution {
    /// Full output text.
    pub output: String,
}

/// Trait implemented by the caller to run a local Puffer agent.
pub trait AgentExecutor {
    /// Executes one workflow node and returns its output.
    fn execute(&mut self, context: ExecutionContext) -> Result<AgentExecution>;
}

/// Native DAG runner for local Puffer agent nodes.
pub struct DagRunner<'a, E> {
    store: &'a WorkflowStore,
    executor: E,
}

impl<'a, E: AgentExecutor> DagRunner<'a, E> {
    /// Creates a DAG runner that appends completed run records to `store`.
    pub fn new(store: &'a WorkflowStore, executor: E) -> Self {
        Self { store, executor }
    }

    /// Executes a workflow and appends the run record.
    pub fn run(
        mut self,
        definition: &WorkflowDefinition,
        trigger: Value,
        trigger_key: Option<String>,
    ) -> Result<WorkflowRun> {
        let started_at_ms = now_ms();
        let run_id = Uuid::new_v4().to_string();
        let mut statuses: BTreeMap<String, WorkflowRunNode> = definition
            .pipeline
            .nodes
            .iter()
            .map(|node| {
                (
                    node.id.clone(),
                    WorkflowRunNode {
                        id: node.id.clone(),
                        status: WorkflowRunStatus::Pending,
                        started_at_ms: None,
                        ended_at_ms: None,
                        output: None,
                        error: None,
                    },
                )
            })
            .collect();
        let mut outputs = BTreeMap::new();
        let mut completed = BTreeSet::new();
        let mut failed = false;

        loop {
            let ready = ready_nodes(&definition.pipeline.nodes, &completed, &statuses);
            if ready.is_empty() {
                break;
            }
            for node in ready {
                if has_failed_dependency(node, &statuses) {
                    mark_skipped(&mut statuses, &node.id, "dependency failed");
                    continue;
                }
                let prompt = interpolate_prompt(&node.prompt, &trigger, &outputs);
                let record = statuses
                    .get_mut(&node.id)
                    .ok_or_else(|| anyhow!("missing run node `{}`", node.id))?;
                record.status = WorkflowRunStatus::Running;
                record.started_at_ms = Some(now_ms());
                let context = ExecutionContext {
                    workflow_slug: definition.slug.clone(),
                    run_id: run_id.clone(),
                    node_id: node.id.clone(),
                    agent: node.agent.clone(),
                    model: node.model.clone(),
                    tools: node.tools.clone(),
                    env: node.env.clone(),
                    working_dir: definition.pipeline.working_dir.clone(),
                    prompt,
                };
                match self.executor.execute(context) {
                    Ok(result) => {
                        let output = excerpt(&result.output);
                        outputs.insert(node.id.clone(), result.output);
                        record.status = WorkflowRunStatus::Completed;
                        record.output = Some(output);
                        record.ended_at_ms = Some(now_ms());
                        completed.insert(node.id.clone());
                    }
                    Err(error) => {
                        record.status = WorkflowRunStatus::Failed;
                        record.error = Some(error.to_string());
                        record.ended_at_ms = Some(now_ms());
                        completed.insert(node.id.clone());
                        failed = true;
                    }
                }
            }
        }

        if failed {
            skip_downstream(&definition.pipeline.nodes, &mut statuses);
        }
        let nodes: Vec<_> = definition
            .pipeline
            .nodes
            .iter()
            .filter_map(|node| statuses.remove(&node.id))
            .collect();
        let status = if nodes
            .iter()
            .any(|node| node.status == WorkflowRunStatus::Failed)
        {
            WorkflowRunStatus::Failed
        } else if nodes
            .iter()
            .any(|node| node.status == WorkflowRunStatus::Skipped)
        {
            WorkflowRunStatus::Skipped
        } else {
            WorkflowRunStatus::Completed
        };
        let error = nodes
            .iter()
            .find(|node| node.status == WorkflowRunStatus::Failed)
            .and_then(|node| node.error.clone());
        let run = WorkflowRun {
            idx: 0,
            workflow_slug: definition.slug.clone(),
            run_id,
            trigger,
            status,
            started_at_ms,
            ended_at_ms: Some(now_ms()),
            nodes,
            error,
            trigger_key,
        };
        self.store.append_run(run)
    }
}

fn ready_nodes<'a>(
    nodes: &'a [PipelineNode],
    completed: &BTreeSet<String>,
    statuses: &BTreeMap<String, WorkflowRunNode>,
) -> Vec<&'a PipelineNode> {
    nodes
        .iter()
        .filter(|node| {
            matches!(
                statuses.get(&node.id).map(|record| record.status),
                Some(WorkflowRunStatus::Pending)
            ) && node.depends_on.iter().all(|dep| completed.contains(dep))
        })
        .collect()
}

fn has_failed_dependency(
    node: &PipelineNode,
    statuses: &BTreeMap<String, WorkflowRunNode>,
) -> bool {
    node.depends_on.iter().any(|dep| {
        statuses
            .get(dep)
            .map(|record| {
                matches!(
                    record.status,
                    WorkflowRunStatus::Failed | WorkflowRunStatus::Skipped
                )
            })
            .unwrap_or(false)
    })
}

fn mark_skipped(statuses: &mut BTreeMap<String, WorkflowRunNode>, node_id: &str, reason: &str) {
    if let Some(record) = statuses.get_mut(node_id) {
        record.status = WorkflowRunStatus::Skipped;
        record.error = Some(reason.to_string());
        record.ended_at_ms = Some(now_ms());
    }
}

fn skip_downstream(nodes: &[PipelineNode], statuses: &mut BTreeMap<String, WorkflowRunNode>) {
    loop {
        let to_skip: Vec<String> = nodes
            .iter()
            .filter(|node| {
                matches!(
                    statuses.get(&node.id).map(|record| record.status),
                    Some(WorkflowRunStatus::Pending)
                ) && has_failed_dependency(node, statuses)
            })
            .map(|node| node.id.clone())
            .collect();
        if to_skip.is_empty() {
            break;
        }
        for id in to_skip {
            mark_skipped(statuses, &id, "dependency failed");
        }
    }
}

fn interpolate_prompt(prompt: &str, trigger: &Value, outputs: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(prompt.len());
    let mut rest = prompt;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str("{{");
            rest = after;
            continue;
        };
        let token = after[..end].trim();
        out.push_str(&resolve_token(token, trigger, outputs));
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn resolve_token(token: &str, trigger: &Value, outputs: &BTreeMap<String, String>) -> String {
    if let Some(path) = token.strip_prefix("trigger.") {
        return json_path(trigger, path).unwrap_or_default();
    }
    if let Some(path) = token.strip_prefix("nodes.") {
        let mut parts = path.splitn(2, '.');
        let Some(node_id) = parts.next() else {
            return String::new();
        };
        if parts.next() == Some("output") {
            return outputs.get(node_id).cloned().unwrap_or_default();
        }
    }
    String::new()
}

fn json_path(value: &Value, path: &str) -> Option<String> {
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(match current {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    })
}

fn excerpt(output: &str) -> String {
    const LIMIT: usize = 4000;
    if output.len() <= LIMIT {
        output.to_string()
    } else {
        format!("{}...", &output[..LIMIT])
    }
}

fn now_ms() -> i128 {
    OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RegisterOptions, TriggerSpec, WorkflowStore};
    use anyhow::bail;
    use serde_json::json;

    #[derive(Default)]
    struct FakeExecutor {
        calls: Vec<String>,
        fail: Option<String>,
    }

    impl AgentExecutor for FakeExecutor {
        fn execute(&mut self, context: ExecutionContext) -> Result<AgentExecution> {
            self.calls.push(context.node_id.clone());
            if self.fail.as_deref() == Some(context.node_id.as_str()) {
                bail!("boom");
            }
            Ok(AgentExecution {
                output: format!("{}:{}", context.node_id, context.prompt),
            })
        }
    }

    #[test]
    fn executes_dependencies_and_interpolation() {
        let temp = tempfile::tempdir().unwrap();
        let store = WorkflowStore::new(temp.path());
        let definition = crate::WorkflowDefinition::from_json(
            json!({
                "slug":"x",
                "trigger":{"type":"cron","cron":"* * * * *"},
                "pipeline":{"name":"x","nodes":[
                    {"id":"a","prompt":"{{ trigger.text }}"},
                    {"id":"b","depends_on":["a"],"prompt":"got {{ nodes.a.output }}"}
                ]}
            }),
            RegisterOptions::default(),
        )
        .unwrap();
        let run = DagRunner::new(&store, FakeExecutor::default())
            .run(&definition, json!({"text":"hi"}), None)
            .unwrap();
        assert_eq!(run.status, WorkflowRunStatus::Completed);
        assert!(run.nodes[1].output.as_deref().unwrap().contains("a:hi"));
    }

    #[test]
    fn skips_downstream_on_failure() {
        let temp = tempfile::tempdir().unwrap();
        let store = WorkflowStore::new(temp.path());
        let definition = crate::WorkflowDefinition::from_json(
            json!({
                "name":"x",
                "nodes":[
                    {"id":"a","prompt":"go"},
                    {"id":"b","depends_on":["a"],"prompt":"after"}
                ]
            }),
            RegisterOptions {
                slug: Some("x".into()),
                trigger: Some(TriggerSpec::Cron {
                    cron: "* * * * *".into(),
                }),
            },
        )
        .unwrap();
        let run = DagRunner::new(
            &store,
            FakeExecutor {
                fail: Some("a".into()),
                ..FakeExecutor::default()
            },
        )
        .run(&definition, json!({}), None)
        .unwrap();
        assert_eq!(run.nodes[0].status, WorkflowRunStatus::Failed);
        assert_eq!(run.nodes[1].status, WorkflowRunStatus::Skipped);
    }
}
