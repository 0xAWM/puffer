use puffer_resources::{PromptTemplate, ToolSpec};
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn read_repo_file(relative_path: &str) -> String {
    fs::read_to_string(repo_root().join(relative_path)).unwrap()
}

fn load_prompt(relative_path: &str) -> PromptTemplate {
    serde_yaml::from_str(&read_repo_file(relative_path)).unwrap()
}

fn load_tool(relative_path: &str) -> ToolSpec {
    serde_yaml::from_str(&read_repo_file(relative_path)).unwrap()
}

fn extract_template_literal(contents: &str, marker: &str) -> String {
    let start = contents.find(marker).unwrap() + marker.len();
    let source = &contents[start..];
    let mut end = None;
    let mut index = 0usize;
    let mut escaped = false;
    let mut interpolation_depth = 0usize;

    while index < source.len() {
        let ch = source[index..].chars().next().unwrap();
        let width = ch.len_utf8();
        if escaped {
            escaped = false;
            index += width;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            index += width;
            continue;
        }
        if interpolation_depth == 0 && ch == '`' {
            end = Some(start + index);
            break;
        }
        if source[index..].starts_with("${") {
            interpolation_depth += 1;
            index += 2;
            continue;
        }
        if interpolation_depth > 0 {
            match ch {
                '{' => interpolation_depth += 1,
                '}' => interpolation_depth = interpolation_depth.saturating_sub(1),
                _ => {}
            }
        }
        index += width;
    }

    contents[start..end.unwrap()].to_string()
}

fn normalize_reference_template(raw: &str) -> String {
    let unescaped = raw.replace("\\`", "`");
    let trimmed = unescaped.strip_prefix('\n').unwrap_or(&unescaped);
    dedent(trimmed)
}

fn dedent(raw: &str) -> String {
    let indent = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|ch| *ch == ' ').count())
        .min()
        .unwrap_or(0);
    raw.lines()
        .map(|line| line.strip_prefix(&" ".repeat(indent)).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn init_prompt_matches_claude_reference() {
    let prompt = load_prompt("resources/prompts/init.yaml");
    let reference = read_repo_file("references/claude-code/src/commands/init.ts");
    let expected = normalize_reference_template(&extract_template_literal(
        &reference,
        "const NEW_INIT_PROMPT = `",
    ));

    assert_eq!(prompt.template.trim_end(), expected.trim_end());
}

#[test]
fn review_prompt_matches_claude_reference_when_rendered() {
    let prompt = load_prompt("resources/prompts/review.yaml");
    let rendered = prompt.render(&std::collections::BTreeMap::from([(
        "ARGUMENTS".to_string(),
        "123".to_string(),
    )]));
    let reference = read_repo_file("references/claude-code/src/commands/review.ts");
    let expected = normalize_reference_template(&extract_template_literal(
        &reference,
        "const LOCAL_REVIEW_PROMPT = (args: string) => `",
    ))
    .replace("${args}", "123");

    assert_eq!(rendered.trim_end(), expected.trim_end());
}

#[test]
fn ask_user_question_tool_prompt_matches_claude_reference() {
    let tool = load_tool("resources/tools/ask_user_question.yaml");
    let reference =
        read_repo_file("references/claude-code/src/tools/AskUserQuestionTool/prompt.ts");
    let prompt = normalize_reference_template(&extract_template_literal(
        &reference,
        "export const ASK_USER_QUESTION_TOOL_PROMPT = `",
    ))
    .replace("${EXIT_PLAN_MODE_TOOL_NAME}", "ExitPlanMode");
    let preview =
        normalize_reference_template(&extract_template_literal(&reference, "markdown: `"));
    let expected = format!("{prompt}\n{preview}");

    assert_eq!(tool.description.trim_end(), expected.trim_end());
}

#[test]
fn enter_plan_mode_tool_prompt_matches_claude_reference() {
    let tool = load_tool("resources/tools/enter_plan_mode.yaml");
    let reference = read_repo_file("references/claude-code/src/tools/EnterPlanModeTool/prompt.ts");
    let what_happens = normalize_reference_template(&extract_template_literal(
        &reference,
        "const WHAT_HAPPENS_SECTION = `",
    ))
    .replace("${ASK_USER_QUESTION_TOOL_NAME}", "AskUserQuestion");
    let expected = normalize_reference_template(&extract_template_literal(&reference, "return `"))
        .replace("${whatHappens}", &format!("{what_happens}\n"))
        .replace("${ASK_USER_QUESTION_TOOL_NAME}", "AskUserQuestion");

    assert_eq!(tool.description.trim_end(), expected.trim_end());
}

#[test]
fn exit_plan_mode_tool_prompt_matches_claude_reference() {
    let tool = load_tool("resources/tools/exit_plan_mode.yaml");
    let reference = read_repo_file("references/claude-code/src/tools/ExitPlanModeTool/prompt.ts");
    let expected = normalize_reference_template(&extract_template_literal(
        &reference,
        "export const EXIT_PLAN_MODE_V2_TOOL_PROMPT = `",
    ))
    .replace("${ASK_USER_QUESTION_TOOL_NAME}", "AskUserQuestion");

    assert_eq!(tool.description.trim_end(), expected.trim_end());
}

#[test]
fn todo_write_tool_prompt_matches_claude_reference() {
    let tool = load_tool("resources/tools/todo_write.yaml");
    let reference = read_repo_file("references/claude-code/src/tools/TodoWriteTool/prompt.ts");
    let expected = normalize_reference_template(&extract_template_literal(
        &reference,
        "export const PROMPT = `",
    ))
    .replace("${FILE_EDIT_TOOL_NAME}", "Edit");

    assert_eq!(tool.description.trim_end(), expected.trim_end());
}
