use super::emit_system;
use crate::runtime::claude_tools::workflow::{task_get, task_list, task_output, task_stop};
use crate::{AppState, TaskStatus};
use anyhow::{Context, Result};
use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use puffer_session_store::SessionStore;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Handles `/tasks` by inspecting and managing persisted workflow task state.
pub(crate) fn handle_tasks_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let trimmed = args.trim();
    match trimmed {
        "" | "show" | "list" => {
            let text = render_tasks_dashboard(state)?;
            emit_system(state, session_store, text)
        }
        "path" => emit_system(state, session_store, render_task_paths(state)?),
        "todos" => emit_system(state, session_store, render_todo_list(state)?),
        _ if trimmed.starts_with("show ") || trimmed.starts_with("get ") => {
            let task_id = task_argument(trimmed)?;
            let text = render_task_details(state, &task_id)?;
            emit_system(state, session_store, text)
        }
        _ if trimmed.starts_with("output ") => {
            let task_id = trimmed.trim_start_matches("output ").trim();
            if task_id.is_empty() {
                return emit_system(
                    state,
                    session_store,
                    "Usage: /tasks output <task-id>".to_string(),
                );
            }
            let text = render_task_output(state, task_id)?;
            emit_system(state, session_store, text)
        }
        _ if trimmed.starts_with("stop ") => {
            let task_id = trimmed.trim_start_matches("stop ").trim();
            if task_id.is_empty() {
                return emit_system(
                    state,
                    session_store,
                    "Usage: /tasks stop <task-id>".to_string(),
                );
            }
            let text = stop_task(state, task_id)?;
            emit_system(state, session_store, text)
        }
        _ => emit_system(
            state,
            session_store,
            "Usage: /tasks [show|list|path|todos|get <task-id>|show <task-id>|output <task-id>|stop <task-id>]".to_string(),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct WorkflowTaskView {
    task_id: String,
    subject: String,
    description: String,
    active_form: String,
    status: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    blocks: Vec<String>,
    #[serde(default)]
    blocked_by: Vec<String>,
    #[serde(default)]
    metadata: serde_json::Map<String, Value>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    task_type: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    process_id: Option<u32>,
    #[serde(default)]
    output_file: Option<String>,
    #[serde(default)]
    started_at_ms: Option<u64>,
    #[serde(default)]
    updated_at_ms: Option<u64>,
    #[serde(default)]
    exit_code: Option<i32>,
}

#[derive(Debug, Deserialize, Default)]
struct WorkflowTodoStoreView {
    #[serde(default)]
    todos: Vec<WorkflowTodoView>,
}

#[derive(Debug, Deserialize)]
struct WorkflowTodoView {
    content: String,
    status: String,
    active_form: String,
}

#[derive(Debug, Deserialize, Default)]
struct WorkflowAgentStoreView {
    #[serde(default)]
    agents: Vec<WorkflowAgentView>,
}

#[derive(Debug, Deserialize)]
struct WorkflowAgentView {
    agent_id: String,
    #[serde(default)]
    name: Option<String>,
    description: String,
    prompt: String,
    #[serde(default)]
    subagent_type: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    team_name: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    isolation: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    status: String,
    output_file: String,
}

#[derive(Debug)]
struct WorkflowPaths {
    root: PathBuf,
    tasks: PathBuf,
    todos: PathBuf,
    agents: PathBuf,
    shell_outputs: PathBuf,
    agent_outputs: PathBuf,
}

fn render_tasks_dashboard(state: &mut AppState) -> Result<String> {
    let paths = workflow_paths(state)?;
    let tasks = load_workflow_tasks(state)?;
    let agents = load_json_store::<WorkflowAgentStoreView>(&paths.agents)?;
    let todos = load_json_store::<WorkflowTodoStoreView>(&paths.todos)?;
    let mut text = String::new();
    let structured_tasks = tasks
        .iter()
        .filter(|task| task_kind(task) == "task")
        .collect::<Vec<_>>();
    let background_tasks = tasks
        .iter()
        .filter(|task| task_kind(task) != "task")
        .collect::<Vec<_>>();

    let _ = writeln!(
        &mut text,
        "Task dashboard\nworkflow_root={}\nstructured_tasks={}\nbackground_tasks={}\nbackground_agents={}\ntodos={}\nrecent_runtime={}",
        paths.root.display(),
        structured_tasks.len(),
        background_tasks.len(),
        agents.agents.len(),
        todos.todos.len(),
        state.tasks().len()
    );
    append_task_section(&mut text, "Task list", &structured_tasks);
    append_task_section(&mut text, "Background tasks", &background_tasks);
    append_agent_section(&mut text, &agents.agents);
    append_todo_section(&mut text, &todos.todos);
    append_runtime_section(&mut text, state);
    Ok(text.trim_end().to_string())
}

fn render_task_paths(state: &AppState) -> Result<String> {
    let paths = workflow_paths(state)?;
    Ok(format!(
        "Task paths\nworkflow_root={}\ntasks_json={}\ntodos_json={}\nagents_json={}\nshell_outputs={}\nagent_outputs={}",
        paths.root.display(),
        paths.tasks.display(),
        paths.todos.display(),
        paths.agents.display(),
        paths.shell_outputs.display(),
        paths.agent_outputs.display()
    ))
}

fn render_todo_list(state: &AppState) -> Result<String> {
    let todos = load_json_store::<WorkflowTodoStoreView>(&workflow_paths(state)?.todos)?;
    let mut text = String::new();
    append_todo_section(&mut text, &todos.todos);
    Ok(text.trim_end().to_string())
}

fn render_task_details(state: &mut AppState, task_id: &str) -> Result<String> {
    if let Ok(task) = load_task(state, task_id) {
        return Ok(render_task_detail(&task));
    }

    let agents = load_json_store::<WorkflowAgentStoreView>(&workflow_paths(state)?.agents)?;
    if let Some(agent) = agents.agents.iter().find(|agent| agent.agent_id == task_id) {
        return Ok(render_agent_detail(agent));
    }

    match render_task_output(state, task_id) {
        Ok(text) => Ok(text),
        Err(_) => Ok(format!("Unknown task `{task_id}`.")),
    }
}

fn render_task_output(state: &mut AppState, task_id: &str) -> Result<String> {
    let cwd = state.cwd.clone();
    let raw = match task_output::execute_task_output(
        state,
        &cwd,
        json!({
            "task_id": task_id,
            "block": false
        }),
    ) {
        Ok(raw) => raw,
        Err(_) => return Ok(format!("Unknown task `{task_id}`.")),
    };
    let payload: Value = serde_json::from_str(&raw).context("invalid TaskOutput payload")?;
    let mut text = String::from("Task output\n");
    let _ = writeln!(
        &mut text,
        "task_id={}\ntask_type={}\nstatus={}\nretrieval_status={}\noutput_file={}",
        payload
            .get("task_id")
            .and_then(Value::as_str)
            .unwrap_or(task_id),
        payload
            .get("task_type")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>"),
        payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>"),
        payload
            .get("retrieval_status")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>"),
        payload
            .get("outputFile")
            .and_then(Value::as_str)
            .unwrap_or("<none>")
    );
    let output = payload
        .get("output")
        .and_then(Value::as_str)
        .unwrap_or("<empty>");
    let _ = writeln!(&mut text, "\n{output}");
    Ok(text.trim_end().to_string())
}

fn stop_task(state: &mut AppState, task_id: &str) -> Result<String> {
    let cwd = state.cwd.clone();
    let raw = match task_stop::execute_task_stop(
        state,
        &cwd,
        json!({
            "task_id": task_id
        }),
    ) {
        Ok(raw) => raw,
        Err(_) => return Ok(format!("Unknown task `{task_id}`.")),
    };
    let payload: Value = serde_json::from_str(&raw).context("invalid TaskStop payload")?;
    Ok(format!(
        "Stopped task\nmessage={}\ntask_id={}\ntask_type={}\ncommand={}\noutput_file={}",
        payload
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Task stopped."),
        payload
            .get("task_id")
            .and_then(Value::as_str)
            .unwrap_or(task_id),
        payload
            .get("task_type")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>"),
        value_as_display(payload.get("command")),
        payload
            .get("outputFile")
            .or_else(|| payload.get("output_file"))
            .and_then(Value::as_str)
            .unwrap_or("<none>")
    ))
}

fn load_task(state: &mut AppState, task_id: &str) -> Result<WorkflowTaskView> {
    let cwd = state.cwd.clone();
    let raw = task_get::execute_task_get(
        state,
        &cwd,
        json!({
            "taskId": task_id
        }),
    )?;
    serde_json::from_str(&raw).context("invalid TaskGet payload")
}

fn load_workflow_tasks(state: &mut AppState) -> Result<Vec<WorkflowTaskView>> {
    let cwd = state.cwd.clone();
    let raw = task_list::execute_task_list(state, &cwd, json!({}))?;
    serde_json::from_str(&raw).context("invalid TaskList payload")
}

fn workflow_paths(state: &AppState) -> Result<WorkflowPaths> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let root = paths
        .workspace_config_dir
        .join("runtime")
        .join("claude_workflow");
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    Ok(WorkflowPaths {
        tasks: root.join("tasks.json"),
        todos: root.join("todos.json"),
        agents: root.join("agents.json"),
        shell_outputs: root.join("shell_outputs"),
        agent_outputs: root.join("agent_outputs"),
        root,
    })
}

fn task_argument(args: &str) -> Result<String> {
    let task_id = args
        .split_once(' ')
        .map(|(_, task_id)| task_id.trim())
        .unwrap_or_default();
    if task_id.is_empty() {
        anyhow::bail!("expected a task id");
    }
    Ok(task_id.to_string())
}

fn append_task_section<'a>(text: &mut String, title: &str, tasks: &[&'a WorkflowTaskView]) {
    let _ = writeln!(text, "\n{title}:");
    if tasks.is_empty() {
        let _ = writeln!(text, "- <none>");
        return;
    }
    for task in tasks {
        let _ = writeln!(
            text,
            "- {} [{}:{}] {}",
            task.task_id,
            task_kind(task),
            task.status,
            task.subject
        );
        if let Some(owner) = task.owner.as_deref() {
            let _ = writeln!(text, "  owner={owner}");
        }
        if !task.blocked_by.is_empty() {
            let _ = writeln!(text, "  blocked_by={}", task.blocked_by.join(", "));
        }
        if !task.blocks.is_empty() {
            let _ = writeln!(text, "  blocks={}", task.blocks.join(", "));
        }
        if let Some(command) = task.command.as_deref() {
            let _ = writeln!(text, "  command={}", shorten(command, 120));
        } else {
            let _ = writeln!(text, "  detail={}", shorten(&task.description, 120));
        }
    }
}

fn append_agent_section(text: &mut String, agents: &[WorkflowAgentView]) {
    let _ = writeln!(text, "\nBackground agents:");
    if agents.is_empty() {
        let _ = writeln!(text, "- <none>");
        return;
    }
    for agent in agents {
        let label = agent.name.as_deref().unwrap_or(agent.agent_id.as_str());
        let _ = writeln!(text, "- {} [{}] {}", agent.agent_id, agent.status, label);
        let _ = writeln!(text, "  detail={}", shorten(&agent.description, 120));
        if let Some(model) = agent.model.as_deref() {
            let _ = writeln!(text, "  model={model}");
        }
        if let Some(team_name) = agent.team_name.as_deref() {
            let _ = writeln!(text, "  team={team_name}");
        }
    }
}

fn append_todo_section(text: &mut String, todos: &[WorkflowTodoView]) {
    let _ = writeln!(text, "\nTodos:");
    if todos.is_empty() {
        let _ = writeln!(text, "- <none>");
        return;
    }
    for todo in todos {
        let _ = writeln!(
            text,
            "- [{}] {} ({})",
            todo.status, todo.content, todo.active_form
        );
    }
}

fn append_runtime_section(text: &mut String, state: &AppState) {
    let _ = writeln!(text, "\nRecent runtime activity:");
    if state.tasks().is_empty() {
        let _ = writeln!(text, "- <none>");
        return;
    }
    for task in state.tasks().iter().rev().take(10) {
        let status = match task.status {
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
        };
        let _ = writeln!(text, "- #{} {} [{}]", task.id, task.label, status);
        let _ = writeln!(text, "  {}", shorten(&task.detail, 120));
    }
}

fn render_task_detail(task: &WorkflowTaskView) -> String {
    let mut text = String::new();
    let _ = writeln!(
        &mut text,
        "Task {}\ntype={}\nstatus={}\nsubject={}\ndescription={}\nactive_form={}\nowner={}\ncommand={}\nprocess_id={}\noutput_file={}\nstarted_at_ms={}\nupdated_at_ms={}\nexit_code={}",
        task.task_id,
        task_kind(task),
        task.status,
        task.subject,
        task.description,
        task.active_form,
        task.owner.as_deref().unwrap_or("<none>"),
        task.command.as_deref().unwrap_or("<none>"),
        task.process_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        task.output_file.as_deref().unwrap_or("<none>"),
        display_optional_u64(task.started_at_ms),
        display_optional_u64(task.updated_at_ms),
        task.exit_code
            .map(|value| value.to_string())
            .unwrap_or_else(|| "<none>".to_string())
    );
    let _ = writeln!(
        &mut text,
        "blocked_by={}\nblocks={}",
        render_list(&task.blocked_by),
        render_list(&task.blocks)
    );
    let _ = writeln!(
        &mut text,
        "metadata={}",
        if task.metadata.is_empty() {
            "<none>".to_string()
        } else {
            serde_json::to_string_pretty(&task.metadata).unwrap_or_default()
        }
    );
    if let Some(output) = task.output.as_deref() {
        let _ = writeln!(&mut text, "\nOutput preview:\n{}", preview_text(output, 20));
    }
    text.trim_end().to_string()
}

fn render_agent_detail(agent: &WorkflowAgentView) -> String {
    format!(
        "Agent {}\nname={}\nstatus={}\ndescription={}\nprompt={}\nsubagent_type={}\nmodel={}\nteam_name={}\nmode={}\nisolation={}\ncwd={}\noutput_file={}",
        agent.agent_id,
        agent.name.as_deref().unwrap_or("<none>"),
        agent.status,
        agent.description,
        agent.prompt,
        agent.subagent_type.as_deref().unwrap_or("<none>"),
        agent.model.as_deref().unwrap_or("<none>"),
        agent.team_name.as_deref().unwrap_or("<none>"),
        agent.mode.as_deref().unwrap_or("<none>"),
        agent.isolation.as_deref().unwrap_or("<none>"),
        agent.cwd.as_deref().unwrap_or("<none>"),
        agent.output_file
    )
}

fn load_json_store<T>(path: &Path) -> Result<T>
where
    T: DeserializeOwned + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn task_kind(task: &WorkflowTaskView) -> &str {
    task.task_type.as_deref().unwrap_or("task")
}

fn render_list(values: &[String]) -> String {
    if values.is_empty() {
        "<none>".to_string()
    } else {
        values.join(", ")
    }
}

fn display_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "<none>".to_string())
}

fn shorten(text: &str, limit: usize) -> String {
    let mut shortened = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= limit {
            shortened.push_str("...");
            return shortened;
        }
        shortened.push(ch);
    }
    shortened
}

fn preview_text(text: &str, max_lines: usize) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() <= max_lines {
        return text.to_string();
    }
    let mut preview = lines[..max_lines].join("\n");
    let _ = write!(
        &mut preview,
        "\n... ({} more lines, use `/tasks output <task-id>` for full output)",
        lines.len() - max_lines
    );
    preview
}

fn value_as_display(value: Option<&Value>) -> String {
    match value {
        Some(Value::Null) | None => "<none>".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}
