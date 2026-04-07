use super::*;
use puffer_resources::load_resources;
use serde_json::json;
use std::path::{Path, PathBuf};

const SLEEP_TOOL_DESCRIPTION: &str =
    "Wait for a specified duration. The user can interrupt the sleep at any time.\n\nUse this when the user tells you to sleep or rest, when you have nothing to do, or when you're waiting for something.\n\nYou may receive <tick> prompts — these are periodic check-ins. Look for useful work to do before sleeping.\n\nYou can call this concurrently with other tools — it won't interfere with them.\n\nPrefer this over `Bash(sleep ...)` — it doesn't hold a shell process.\n\nEach wake-up costs an API call, but the prompt cache expires after 5 minutes of inactivity — balance accordingly.";
const SKILL_DESCRIPTION: &str = "Execute a skill within the main conversation\n\nWhen users ask you to perform tasks, check if any of the available skills match. Skills provide specialized capabilities and domain knowledge.\n\nWhen users reference a \"slash command\" or \"/<something>\" (e.g., \"/commit\", \"/review-pr\"), they are referring to a skill. Use this tool to invoke it.\n\nHow to invoke:\n- Use this tool with the skill name and optional arguments\n- Examples:\n  - `skill: \"pdf\"` - invoke the pdf skill\n  - `skill: \"commit\", args: \"-m 'Fix bug'\"` - invoke with arguments\n  - `skill: \"review-pr\", args: \"123\"` - invoke with arguments\n  - `skill: \"ms-office-suite:pdf\"` - invoke using fully qualified name\n\nImportant:\n- Available skills are listed in system-reminder messages in the conversation\n- When a skill matches the user's request, this is a BLOCKING REQUIREMENT: invoke the relevant Skill tool BEFORE generating any other response about the task\n- NEVER mention a skill without actually calling this tool\n- Do not invoke a skill that is already running\n- Do not use this tool for built-in CLI commands (like /help, /clear, etc.)\n- If you see a <command-name> tag in the current conversation turn, the skill has ALREADY been loaded - follow the instructions directly instead of calling this tool again";
const TOOL_SEARCH_DESCRIPTION: &str = "Fetches full schema definitions for deferred tools so they can be called.\n\nDeferred tools appear by name in <system-reminder> messages. Until fetched, only the name is known - there is no parameter schema, so the tool cannot be invoked. This tool takes a query, matches it against the deferred tool list, and returns the matched tools' complete JSONSchema definitions inside a <functions> block. Once a tool's schema appears in that result, it is callable exactly like any tool defined at the top of this prompt.\n\nResult format: each matched tool appears as one <function>{\"description\": \"...\", \"name\": \"...\", \"parameters\": {...}}</function> line inside the <functions> block - the same encoding as the tool list at the top of this prompt.\n\nQuery forms:\n- \"select:Read,Edit,Grep\" - fetch these exact tools by name\n- \"notebook jupyter\" - keyword search, up to max_results best matches\n- \"+slack send\" - require \"slack\" in the name, rank by remaining terms";
const SEND_MESSAGE_DESCRIPTION: &str = "# SendMessage\n\nSend a message to another agent.\n\n```json\n{\"to\": \"researcher\", \"summary\": \"assign task 1\", \"message\": \"start on task #1\"}\n```\n\n| `to` | |\n|---|---|\n| `\"researcher\"` | Teammate by name |\n| `\"*\"` | Broadcast to all teammates — expensive (linear in team size), use only when everyone genuinely needs it |\n\nYour plain text output is NOT visible to other agents — to communicate, you MUST call this tool. Messages from teammates are delivered automatically; you don't check an inbox. Refer to teammates by name, never by UUID. When relaying, don't quote the original — it's already rendered to the user.\n\n## Protocol responses (legacy)\n\nIf you receive a JSON message with `type: \"shutdown_request\"` or `type: \"plan_approval_request\"`, respond with the matching `_response` type — echo the `request_id`, set `approve` true/false:\n\n```json\n{\"to\": \"team-lead\", \"message\": {\"type\": \"shutdown_response\", \"request_id\": \"...\", \"approve\": true}}\n{\"to\": \"researcher\", \"message\": {\"type\": \"plan_approval_response\", \"request_id\": \"...\", \"approve\": false, \"feedback\": \"add error handling\"}}\n```\n\nApproving shutdown terminates your process. Rejecting plan sends the teammate back to revise. Don't originate `shutdown_request` unless asked. Don't send structured JSON status messages — use TaskUpdate.";
const SEND_USER_MESSAGE_DESCRIPTION: &str = "Send a message the user will read. Text outside this tool is visible in the detail view, but most won't open it — the answer lives here.\n\n`message` supports markdown. `attachments` takes file paths (absolute or cwd-relative) for images, diffs, logs.\n\n`status` labels intent: 'normal' when replying to what they just asked; 'proactive' when you're initiating — a scheduled task finished, a blocker surfaced during background work, you need input on something they haven't asked about. Set it honestly; downstream routing uses it.";
const TASK_GET_DESCRIPTION: &str = "Use this tool to retrieve a task by its ID from the task list.\n\n## When to Use This Tool\n\n- When you need the full description and context before starting work on a task\n- To understand task dependencies (what it blocks, what blocks it)\n- After being assigned a task, to get complete requirements\n\n## Output\n\nReturns full task details:\n- **subject**: Task title\n- **description**: Detailed requirements and context\n- **status**: 'pending', 'in_progress', or 'completed'\n- **blocks**: Tasks waiting on this one to complete\n- **blockedBy**: Tasks that must complete before this one can start\n\n## Tips\n\n- After fetching a task, verify its blockedBy list is empty before beginning work.\n- Use TaskList to see all tasks in summary form.";
const TASK_LIST_DESCRIPTION: &str = "Use this tool to list all tasks in the task list.\n\n## When to Use This Tool\n\n- To see what tasks are available to work on (status: 'pending', no owner, not blocked)\n- To check overall progress on the project\n- To find tasks that are blocked and need dependencies resolved\n- Before assigning tasks to teammates, to see what's available\n- After completing a task, to check for newly unblocked work or claim the next available task\n- **Prefer working on tasks in ID order** (lowest ID first) when multiple tasks are available, as earlier tasks often set up context for later ones\n\n## Output\n\nReturns a summary of each task:\n- **id**: Task identifier (use with TaskGet, TaskUpdate)\n- **subject**: Brief description of the task\n- **status**: 'pending', 'in_progress', or 'completed'\n- **owner**: Agent ID if assigned, empty if available\n- **blockedBy**: List of open task IDs that must be resolved first (tasks with blockedBy cannot be claimed until dependencies resolve)\n\nUse TaskGet with a specific task ID to view full details including description and comments.\n\n## Teammate Workflow\n\nWhen working as a teammate:\n1. After completing your current task, call TaskList to find available work\n2. Look for tasks with status 'pending', no owner, and empty blockedBy\n3. **Prefer tasks in ID order** (lowest ID first) when multiple tasks are available, as earlier tasks often set up context for later ones\n4. Claim an available task using TaskUpdate (set `owner` to your name), or wait for leader assignment\n5. If blocked, focus on unblocking tasks or notify the team lead";
const TASK_STOP_DESCRIPTION: &str = "- Stops a running background task by its ID\n- Takes a task_id parameter identifying the task to stop\n- Returns a success or failure status\n- Use this tool when you need to terminate a long-running task";
const TASK_OUTPUT_DESCRIPTION: &str = "DEPRECATED: Prefer using the Read tool on the task's output file path instead. Background tasks return their output file path in the tool result, and you receive a <task-notification> with the same path when the task completes — Read that file directly.\n\n- Retrieves output from a running or completed task (background shell, agent, or remote session)\n- Takes a task_id parameter identifying the task\n- Returns the task output along with status information\n- Use block=true (default) to wait for task completion\n- Use block=false for non-blocking check of current status\n- Task IDs can be found using the /tasks command\n- Works with all task types: background shells, async agents, and remote sessions";

#[test]
fn sleep_tool_is_visible_to_anthropic_and_openai_tool_builders() {
    let resources = bundled_resources();
    let registry = ToolRegistry::from_resources(&resources);

    let anthropic = anthropic_tool_definitions(&registry, None).unwrap();
    let anthropic_sleep = anthropic
        .iter()
        .find(|definition| definition["name"] == json!("Sleep"))
        .expect("Sleep tool definition");
    assert_eq!(
        anthropic_sleep["description"],
        json!(SLEEP_TOOL_DESCRIPTION)
    );
    assert_eq!(
        anthropic_sleep["input_schema"]["required"],
        json!(["duration_ms"])
    );

    let openai = openai_tool_definitions(&registry, None, false).unwrap();
    let openai_sleep = openai
        .iter()
        .find(|definition| definition.name == "Sleep")
        .expect("Sleep tool definition");
    assert_eq!(openai_sleep.description, SLEEP_TOOL_DESCRIPTION);
    assert_eq!(openai_sleep.parameters["required"], json!(["duration_ms"]));
}

#[test]
fn bundled_resources_register_sleep_tool() {
    let resources = bundled_resources();
    let registry = ToolRegistry::from_resources(&resources);
    let definition = registry.definition("Sleep").expect("Sleep tool definition");

    assert_eq!(definition.handler, "runtime:sleep");
    assert_eq!(definition.description, SLEEP_TOOL_DESCRIPTION);
}

#[test]
fn workflow_tool_descriptions_match_claude_reference_for_anthropic_and_openai() {
    let resources = bundled_resources();
    let registry = ToolRegistry::from_resources(&resources);

    for (tool_id, description) in [
        ("Skill", SKILL_DESCRIPTION),
        ("ToolSearch", TOOL_SEARCH_DESCRIPTION),
        ("SendMessage", SEND_MESSAGE_DESCRIPTION),
        ("SendUserMessage", SEND_USER_MESSAGE_DESCRIPTION),
        ("TaskGet", TASK_GET_DESCRIPTION),
        ("TaskList", TASK_LIST_DESCRIPTION),
        ("TaskStop", TASK_STOP_DESCRIPTION),
        ("TaskOutput", TASK_OUTPUT_DESCRIPTION),
    ] {
        let definition = registry.definition(tool_id).expect("tool definition");
        assert_eq!(definition.description, description);

        let anthropic = anthropic_tool_definitions(&registry, None).unwrap();
        let anthropic_definition = anthropic
            .iter()
            .find(|item| item["name"] == json!(tool_id))
            .expect("anthropic tool definition");
        assert_eq!(anthropic_definition["description"], json!(description));

        let openai = openai_tool_definitions(&registry, None, false).unwrap();
        let openai_definition = openai
            .iter()
            .find(|item| item.name == tool_id)
            .expect("openai tool definition");
        assert_eq!(openai_definition.description, description);
    }
}

fn bundled_resources() -> LoadedResources {
    let root = workspace_root();
    let temp = tempfile::tempdir().unwrap();
    let paths = ConfigPaths {
        workspace_root: temp.path().join("workspace"),
        workspace_config_dir: temp.path().join("workspace/.puffer"),
        user_config_dir: temp.path().join("user"),
        builtin_resources_dir: root.join("resources"),
    };
    load_resources(&paths).unwrap()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}
