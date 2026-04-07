use puffer_resources::{AgentSpec, LoadedItem, LoadedResources};

const AGENT_TOOL_ID: &str = "Agent";
const AVAILABLE_AGENT_TYPES_VARIABLE: &str = "{{AVAILABLE_AGENT_TYPES}}";

/// Renders the Agent tool description with the current agent catalog inlined.
pub(crate) fn render_agent_tool_description(resources: &LoadedResources) -> String {
    let template = resources
        .tools
        .iter()
        .find(|item| item.value.id == AGENT_TOOL_ID)
        .map(|item| item.value.description.as_str())
        .unwrap_or_default();
    render_available_agent_types(template, resources)
}

fn render_available_agent_types(description: &str, resources: &LoadedResources) -> String {
    description.replace(
        AVAILABLE_AGENT_TYPES_VARIABLE,
        &render_agent_lines(&resources.agents),
    )
}

fn render_agent_lines(agents: &[LoadedItem<AgentSpec>]) -> String {
    if agents.is_empty() {
        return "None".to_string();
    }

    agents
        .iter()
        .map(|agent| format_agent_line(&agent.value))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_agent_line(agent: &AgentSpec) -> String {
    format!(
        "- {}: {} (Tools: {})",
        agent.id,
        agent.description,
        describe_tools(agent)
    )
}

fn describe_tools(agent: &AgentSpec) -> String {
    let has_allowlist = !agent.tools.is_empty();
    let has_denylist = !agent.disallowed_tools.is_empty();
    let allow_all = agent.tools.iter().any(|tool| tool == "*");

    if allow_all && has_denylist {
        return format!("All tools except {}", agent.disallowed_tools.join(", "));
    }

    if has_allowlist && has_denylist {
        let effective = agent
            .tools
            .iter()
            .filter(|tool| !agent.disallowed_tools.iter().any(|deny| deny == *tool))
            .cloned()
            .collect::<Vec<_>>();
        return if effective.is_empty() {
            "None".to_string()
        } else {
            effective.join(", ")
        };
    }

    if has_allowlist {
        if allow_all {
            return "All tools".to_string();
        }
        return agent.tools.join(", ");
    }

    if has_denylist {
        return format!("All tools except {}", agent.disallowed_tools.join(", "));
    }

    "All tools".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use puffer_resources::{SourceInfo, SourceKind, ToolSpec};
    use std::path::PathBuf;

    fn agent(
        id: &str,
        description: &str,
        tools: &[&str],
        disallowed_tools: &[&str],
    ) -> LoadedItem<AgentSpec> {
        LoadedItem {
            value: AgentSpec {
                id: id.to_string(),
                description: description.to_string(),
                tools: tools.iter().map(|tool| (*tool).to_string()).collect(),
                disallowed_tools: disallowed_tools
                    .iter()
                    .map(|tool| (*tool).to_string())
                    .collect(),
                ..AgentSpec::default()
            },
            source_info: SourceInfo {
                path: PathBuf::from(format!("{id}.yaml")),
                kind: SourceKind::Builtin,
            },
        }
    }

    fn agent_tool(description: &str) -> LoadedItem<ToolSpec> {
        LoadedItem {
            value: ToolSpec {
                id: AGENT_TOOL_ID.to_string(),
                name: AGENT_TOOL_ID.to_string(),
                description: description.to_string(),
                handler: "runtime:agent".to_string(),
                ..ToolSpec::default()
            },
            source_info: SourceInfo {
                path: PathBuf::from("tools/agent.yaml"),
                kind: SourceKind::Builtin,
            },
        }
    }

    #[test]
    fn render_agent_tool_description_replaces_agent_listing_placeholder() {
        let resources = LoadedResources {
            tools: vec![agent_tool("Agents:\n{{AVAILABLE_AGENT_TYPES}}")],
            agents: vec![
                agent("general-purpose", "General-purpose agent", &["*"], &[]),
                agent(
                    "Explore",
                    "Fast agent specialized for exploring codebases.",
                    &[],
                    &["Agent", "ExitPlanMode", "Edit", "Write", "NotebookEdit"],
                ),
            ],
            ..LoadedResources::default()
        };

        let rendered = render_agent_tool_description(&resources);

        assert_eq!(
            rendered,
            "Agents:\n\
- general-purpose: General-purpose agent (Tools: All tools)\n\
- Explore: Fast agent specialized for exploring codebases. (Tools: All tools except Agent, ExitPlanMode, Edit, Write, NotebookEdit)"
        );
    }

    #[test]
    fn render_agent_tool_description_uses_none_when_no_agents_are_loaded() {
        let resources = LoadedResources {
            tools: vec![agent_tool("Agents:\n{{AVAILABLE_AGENT_TYPES}}")],
            ..LoadedResources::default()
        };

        assert_eq!(render_agent_tool_description(&resources), "Agents:\nNone");
    }
}
