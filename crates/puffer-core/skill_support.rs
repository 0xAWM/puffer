use puffer_resources::{LoadedItem, SkillSpec};
use std::path::Path;

/// Renders one loaded skill into prompt text with Claude-style argument substitutions.
pub(crate) fn render_skill_prompt(
    skill: &LoadedItem<SkillSpec>,
    args: &str,
    session_id: &str,
) -> String {
    let mut content = skill.value.content.clone();
    content = substitute_arguments(&content, args, &skill.value.argument_names);
    content = content.replace("${CLAUDE_SESSION_ID}", session_id);
    if let Some(skill_dir) = skill.source_info.path.parent() {
        let normalized = normalize_skill_dir(skill_dir);
        content = content.replace("${CLAUDE_SKILL_DIR}", &normalized);
        content = format!("Base directory for this skill: {normalized}\n\n{content}");
    }
    content
}

fn normalize_skill_dir(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn substitute_arguments(content: &str, raw_args: &str, argument_names: &[String]) -> String {
    let parsed_args = parse_arguments(raw_args);
    let original = content.to_string();
    let mut rendered = original.clone();

    for (index, value) in parsed_args.iter().enumerate() {
        rendered = rendered.replace(&format!("$ARGUMENTS[{index}]"), value);
        rendered = rendered.replace(&format!("${index}"), value);
    }
    rendered = rendered.replace("$ARGUMENTS", raw_args);
    for (name, value) in argument_names.iter().zip(parsed_args.iter()) {
        rendered = replace_named_argument(&rendered, name, value);
    }

    if rendered == original && !raw_args.trim().is_empty() {
        format!("{rendered}\n\nARGUMENTS: {raw_args}")
    } else {
        rendered
    }
}

fn parse_arguments(raw_args: &str) -> Vec<String> {
    shell_words::split(raw_args).unwrap_or_else(|_| {
        raw_args
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>()
    })
}

fn replace_named_argument(content: &str, name: &str, value: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let needle = format!("${name}");
    let mut cursor = 0usize;
    while let Some(relative) = content[cursor..].find(&needle) {
        let start = cursor + relative;
        let end = start + needle.len();
        let next = content[end..].chars().next();
        if next.is_some_and(|ch| ch == '[' || ch.is_ascii_alphanumeric() || ch == '_') {
            result.push_str(&content[cursor..end]);
        } else {
            result.push_str(&content[cursor..start]);
            result.push_str(value);
        }
        cursor = end;
    }
    result.push_str(&content[cursor..]);
    result
}

#[cfg(test)]
mod tests {
    use super::render_skill_prompt;
    use puffer_resources::{LoadedItem, SkillSpec, SourceInfo, SourceKind};
    use std::path::PathBuf;

    fn loaded_skill(content: &str, argument_names: &[&str]) -> LoadedItem<SkillSpec> {
        LoadedItem {
            value: SkillSpec {
                name: "verify".to_string(),
                description: "Verify changes".to_string(),
                content: content.to_string(),
                argument_names: argument_names
                    .iter()
                    .map(|value| value.to_string())
                    .collect(),
                ..SkillSpec::default()
            },
            source_info: SourceInfo {
                path: PathBuf::from("/tmp/work/.puffer/resources/skills/verify/SKILL.md"),
                kind: SourceKind::Workspace,
            },
        }
    }

    #[test]
    fn render_skill_prompt_substitutes_full_and_indexed_arguments() {
        let rendered = render_skill_prompt(
            &loaded_skill("Run $ARGUMENTS with $ARGUMENTS[0] and $1", &[]),
            "cargo test --lib",
            "session-1",
        );
        assert!(rendered.contains("Run cargo test --lib with cargo and test"));
        assert!(rendered
            .contains("Base directory for this skill: /tmp/work/.puffer/resources/skills/verify"));
    }

    #[test]
    fn render_skill_prompt_substitutes_named_arguments_and_session_data() {
        let rendered = render_skill_prompt(
            &loaded_skill("Ticket $ticket on ${CLAUDE_SESSION_ID}", &["ticket"]),
            "ABC-123",
            "session-1",
        );
        assert!(rendered.contains("Ticket ABC-123 on session-1"));
    }

    #[test]
    fn render_skill_prompt_appends_arguments_when_no_placeholder_exists() {
        let rendered = render_skill_prompt(
            &loaded_skill("Investigate the issue.", &[]),
            "auth regression",
            "session-1",
        );
        assert!(rendered.contains("Investigate the issue.\n\nARGUMENTS: auth regression"));
    }
}
