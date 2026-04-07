use crate::{AppState, MessageRole};
use anyhow::{anyhow, bail, Context, Result};
use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use puffer_provider_registry::{AuthStore, ProviderRegistry};
use puffer_resources::{agent_by_id, LoadedResources};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use uuid::Uuid;

#[derive(Debug, serde::Deserialize)]
struct AgentToolInput {
    description: String,
    prompt: String,
    #[serde(default)]
    subagent_type: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    run_in_background: bool,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    isolation: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    team_name: Option<String>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Serialize)]
struct AgentCompletedOutput {
    status: &'static str,
    #[serde(rename = "agentId")]
    agent_id: String,
    #[serde(rename = "agentType")]
    agent_type: String,
    description: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "teamName")]
    team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    isolation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "worktreePath")]
    worktree_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "worktreeBranch")]
    worktree_branch: Option<String>,
    #[serde(rename = "toolUses")]
    tool_uses: usize,
    result: String,
}

#[derive(Debug, Serialize)]
struct AgentAsyncOutput {
    status: &'static str,
    #[serde(rename = "agentId")]
    agent_id: String,
    #[serde(rename = "agentType")]
    agent_type: String,
    description: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "teamName")]
    team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    isolation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "worktreePath")]
    worktree_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "worktreeBranch")]
    worktree_branch: Option<String>,
    #[serde(rename = "outputFile")]
    output_file: String,
    #[serde(rename = "canReadOutputFile")]
    can_read_output_file: bool,
}

#[derive(Debug)]
struct AgentWorktree {
    repo_root: PathBuf,
    path: PathBuf,
    branch: String,
    preserve_on_completion: bool,
}

#[derive(Debug)]
struct PreparedAgentExecution {
    agent_id: String,
    agent_type: String,
    description: String,
    prompt: String,
    name: Option<String>,
    run_in_background: bool,
    nested_cwd: PathBuf,
    nested_state: AppState,
    nested_resources: LoadedResources,
    resolved_model: Option<String>,
    isolation: Option<String>,
    team_name: Option<String>,
    mode: Option<String>,
    worktree: Option<AgentWorktree>,
}

/// Executes the runtime-backed `Agent` tool by running a nested model turn.
pub(super) fn execute_agent_tool(
    state: &AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    auth_store: &mut AuthStore,
    cwd: &Path,
    input: Value,
) -> Result<String> {
    let input: AgentToolInput = serde_json::from_value(input).context("invalid Agent input")?;
    if input.prompt.trim().is_empty() {
        bail!("Agent prompt cannot be empty");
    }
    if input.cwd.is_some() && input.isolation.as_deref() == Some("worktree") {
        bail!("agent cwd override is incompatible with isolation=worktree");
    }
    if let Some(isolation) = input
        .isolation
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        if isolation != "worktree" {
            bail!("unsupported agent isolation `{isolation}`");
        }
    }

    let prepared = prepare_agent_execution(state, resources, providers, cwd, input)?;
    if prepared.prompt.trim().is_empty() {
        bail!("Agent prompt cannot be empty");
    }
    if prepared.nested_state.current_provider.is_none() && providers.providers().next().is_none() {
        bail!("no providers are registered");
    }

    if prepared.run_in_background {
        return launch_background_agent(prepared, providers.clone(), auth_store.clone());
    }

    run_agent_synchronously(prepared, providers, auth_store)
}

fn prepare_agent_execution(
    state: &AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    cwd: &Path,
    input: AgentToolInput,
) -> Result<PreparedAgentExecution> {
    let selected_agent = input
        .subagent_type
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("general-purpose");
    let agent = agent_by_id(resources, selected_agent)
        .or_else(|| {
            resources
                .agents
                .iter()
                .find(|item| item.value.id.eq_ignore_ascii_case(selected_agent))
        })
        .ok_or_else(|| {
            let available = resources
                .agents
                .iter()
                .map(|item| item.value.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow!("unknown agent `{selected_agent}`. Available agents: {available}")
        })?;

    let nested_cwd = resolve_agent_cwd(cwd, input.cwd.as_deref())?;
    let nested_resources = filter_resources_for_agent(resources, &agent.value.tools);
    let mut nested_state = state.clone();
    let mut nested_cwd = nested_cwd;
    let mut worktree = None;
    if input.isolation.as_deref() == Some("worktree") {
        let created = create_agent_worktree(cwd, &Uuid::new_v4().simple().to_string())?;
        nested_cwd = created.path.clone();
        worktree = Some(created);
    }
    nested_state.cwd = nested_cwd.clone();
    nested_state.transcript.clear();
    nested_state.push_message(MessageRole::System, agent.value.prompt.trim().to_string());
    if input.mode.as_deref() == Some("plan") {
        nested_state.plan_mode = true;
    }

    if let Some(model) = input
        .model
        .as_deref()
        .or(agent.value.model.as_deref())
        .filter(|value| !value.trim().is_empty())
    {
        let resolved = providers.resolve_model(model);
        nested_state.current_model = Some(model.to_string());
        nested_state.current_provider = resolved
            .map(|descriptor| descriptor.provider.clone())
            .or_else(|| {
                model
                    .split_once('/')
                    .map(|(provider, _)| provider.to_string())
            })
            .or_else(|| state.current_provider.clone());
    }
    Ok(PreparedAgentExecution {
        agent_id: format!("agent-{}", Uuid::new_v4().simple()),
        agent_type: agent.value.id.clone(),
        description: input.description.trim().to_string(),
        prompt: input.prompt,
        name: input
            .name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        run_in_background: input.run_in_background,
        nested_cwd,
        resolved_model: nested_state.current_model.clone(),
        nested_state,
        nested_resources,
        isolation: input.isolation,
        team_name: input
            .team_name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        mode: input
            .mode
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        worktree,
    })
}

fn run_agent_synchronously(
    mut prepared: PreparedAgentExecution,
    providers: &ProviderRegistry,
    auth_store: &mut AuthStore,
) -> Result<String> {
    let turn = super::execute_user_prompt(
        &mut prepared.nested_state,
        &prepared.nested_resources,
        providers,
        auth_store,
        &prepared.prompt,
    )?;
    let payload = AgentCompletedOutput {
        status: "completed",
        agent_id: prepared.agent_id,
        agent_type: prepared.agent_type,
        description: prepared.description,
        prompt: prepared.prompt,
        name: prepared.name,
        cwd: prepared.nested_cwd.display().to_string(),
        model: prepared.resolved_model,
        team_name: prepared.team_name,
        mode: prepared.mode,
        isolation: prepared.isolation.clone(),
        worktree_path: prepared
            .worktree
            .as_ref()
            .map(|worktree| worktree.path.display().to_string()),
        worktree_branch: prepared
            .worktree
            .as_ref()
            .map(|worktree| worktree.branch.clone()),
        tool_uses: turn.tool_invocations.len(),
        result: turn.assistant_text.trim().to_string(),
    };
    if let Some(worktree) = prepared.worktree.take() {
        cleanup_agent_worktree(worktree)?;
    }
    Ok(serde_json::to_string_pretty(&payload)?)
}

fn launch_background_agent(
    mut prepared: PreparedAgentExecution,
    providers: ProviderRegistry,
    auth_store: AuthStore,
) -> Result<String> {
    if let Some(worktree) = prepared.worktree.as_mut() {
        worktree.preserve_on_completion = true;
    }
    let output_file = agent_output_path(&prepared.nested_state.session.cwd, &prepared.agent_id)?;
    fs::write(
        &output_file,
        serde_json::to_string_pretty(&json!({
            "status": "running",
            "agentId": prepared.agent_id,
            "agentType": prepared.agent_type,
            "description": prepared.description,
            "prompt": prepared.prompt,
            "name": prepared.name,
            "cwd": prepared.nested_cwd.display().to_string(),
            "model": prepared.resolved_model,
        }))?,
    )
    .with_context(|| format!("failed to initialize {}", output_file.display()))?;

    let response = AgentAsyncOutput {
        status: "async_launched",
        agent_id: prepared.agent_id.clone(),
        agent_type: prepared.agent_type.clone(),
        description: prepared.description.clone(),
        prompt: prepared.prompt.clone(),
        name: prepared.name.clone(),
        cwd: prepared.nested_cwd.display().to_string(),
        model: prepared.resolved_model.clone(),
        team_name: prepared.team_name.clone(),
        mode: prepared.mode.clone(),
        isolation: prepared.isolation.clone(),
        worktree_path: prepared
            .worktree
            .as_ref()
            .map(|worktree| worktree.path.display().to_string()),
        worktree_branch: prepared
            .worktree
            .as_ref()
            .map(|worktree| worktree.branch.clone()),
        output_file: output_file.display().to_string(),
        can_read_output_file: true,
    };

    thread::spawn(move || {
        let mut nested_state = prepared.nested_state;
        let nested_resources = prepared.nested_resources;
        let prompt = prepared.prompt.clone();
        let result = {
            let mut nested_auth_store = auth_store;
            super::execute_user_prompt(
                &mut nested_state,
                &nested_resources,
                &providers,
                &mut nested_auth_store,
                &prompt,
            )
        };
        let final_payload = match result {
            Ok(turn) => json!(AgentCompletedOutput {
                status: "completed",
                agent_id: prepared.agent_id,
                agent_type: prepared.agent_type,
                description: prepared.description,
                prompt: prepared.prompt,
                name: prepared.name,
                cwd: prepared.nested_cwd.display().to_string(),
                model: prepared.resolved_model,
                team_name: prepared.team_name,
                mode: prepared.mode,
                isolation: prepared.isolation,
                worktree_path: prepared
                    .worktree
                    .as_ref()
                    .map(|worktree| worktree.path.display().to_string()),
                worktree_branch: prepared
                    .worktree
                    .as_ref()
                    .map(|worktree| worktree.branch.clone()),
                tool_uses: turn.tool_invocations.len(),
                result: turn.assistant_text.trim().to_string(),
            }),
            Err(error) => json!({
                "status": "failed",
                "agentId": prepared.agent_id,
                "agentType": prepared.agent_type,
                "description": prepared.description,
                "prompt": prepared.prompt,
                "name": prepared.name,
                "cwd": prepared.nested_cwd.display().to_string(),
                "model": prepared.resolved_model,
                "teamName": prepared.team_name,
                "mode": prepared.mode,
                "isolation": prepared.isolation,
                "worktreePath": prepared
                    .worktree
                    .as_ref()
                    .map(|worktree| worktree.path.display().to_string()),
                "worktreeBranch": prepared
                    .worktree
                    .as_ref()
                    .map(|worktree| worktree.branch.clone()),
                "error": error.to_string(),
            }),
        };
        let _ = fs::write(
            &output_file,
            serde_json::to_string_pretty(&final_payload)
                .unwrap_or_else(|_| "{\"status\":\"failed\"}".to_string()),
        );
    });

    Ok(serde_json::to_string_pretty(&response)?)
}

fn resolve_agent_cwd(parent_cwd: &Path, override_cwd: Option<&str>) -> Result<PathBuf> {
    let Some(override_cwd) = override_cwd.filter(|value| !value.trim().is_empty()) else {
        return Ok(parent_cwd.to_path_buf());
    };
    let requested = PathBuf::from(override_cwd.trim());
    let resolved = if requested.is_absolute() {
        requested
    } else {
        parent_cwd.join(requested)
    };
    let metadata = std::fs::metadata(&resolved)
        .with_context(|| format!("agent cwd {} does not exist", resolved.display()))?;
    if !metadata.is_dir() {
        bail!("agent cwd {} is not a directory", resolved.display());
    }
    Ok(resolved)
}

fn create_agent_worktree(parent_cwd: &Path, suffix: &str) -> Result<AgentWorktree> {
    let repo_root = git_toplevel(parent_cwd)
        .ok_or_else(|| anyhow!("agent worktree isolation requires a git repository"))?;
    let worktree_root = repo_root.join(".worktree").join("agents");
    fs::create_dir_all(&worktree_root)
        .with_context(|| format!("failed to create {}", worktree_root.display()))?;
    let branch = format!("puffer-agent-{suffix}");
    let path = worktree_root.join(suffix);
    let status = Command::new("git")
        .args([
            "-C",
            repo_root.to_string_lossy().as_ref(),
            "worktree",
            "add",
            "-b",
            &branch,
            path.to_string_lossy().as_ref(),
        ])
        .status()
        .with_context(|| format!("failed to launch git worktree add for {}", path.display()))?;
    if !status.success() {
        bail!("git worktree add failed for {}", path.display());
    }
    Ok(AgentWorktree {
        repo_root,
        path,
        branch,
        preserve_on_completion: false,
    })
}

fn cleanup_agent_worktree(worktree: AgentWorktree) -> Result<()> {
    if worktree.preserve_on_completion {
        return Ok(());
    }
    let status = Command::new("git")
        .args([
            "-C",
            worktree.path.to_string_lossy().as_ref(),
            "status",
            "--porcelain",
        ])
        .output()
        .with_context(|| format!("failed to inspect {}", worktree.path.display()))?;
    if !status.status.success() || !String::from_utf8_lossy(&status.stdout).trim().is_empty() {
        return Ok(());
    }
    let remove = Command::new("git")
        .args([
            "-C",
            worktree.repo_root.to_string_lossy().as_ref(),
            "worktree",
            "remove",
            "--force",
            worktree.path.to_string_lossy().as_ref(),
        ])
        .status()
        .with_context(|| format!("failed to remove {}", worktree.path.display()))?;
    if !remove.success() {
        bail!("git worktree remove failed for {}", worktree.path.display());
    }
    let _ = Command::new("git")
        .args([
            "-C",
            worktree.repo_root.to_string_lossy().as_ref(),
            "branch",
            "-D",
            &worktree.branch,
        ])
        .status();
    Ok(())
}

fn git_toplevel(cwd: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["-C", cwd.to_string_lossy().as_ref(), "rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then(|| PathBuf::from(text))
}

fn agent_output_path(session_cwd: &Path, agent_id: &str) -> Result<PathBuf> {
    let paths = ConfigPaths::discover(session_cwd);
    ensure_workspace_dirs(&paths)?;
    let dir = paths
        .workspace_config_dir
        .join("runtime")
        .join("agent_outputs");
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(dir.join(format!("{agent_id}.json")))
}

fn filter_resources_for_agent(resources: &LoadedResources, tools: &[String]) -> LoadedResources {
    let mut filtered = resources.clone();
    let wildcard = tools.is_empty() || tools.iter().any(|tool| tool == "*");
    filtered.tools.retain(|tool| {
        if tool.value.id.eq_ignore_ascii_case("Agent") {
            return false;
        }
        wildcard
            || tools
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(&tool.value.id))
    });
    filtered
}
