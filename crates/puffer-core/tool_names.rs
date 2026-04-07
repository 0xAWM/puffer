use puffer_resources::ToolSpec;
use puffer_tools::ToolDefinition;
use std::collections::BTreeSet;

/// Normalizes tool ids, aliases, and legacy selector names into one canonical form.
pub(crate) fn canonical_tool_name(raw: &str) -> String {
    let collapsed = raw
        .trim()
        .replace('-', "_")
        .replace(' ', "_")
        .to_ascii_lowercase()
        .replace('_', "");
    match collapsed.as_str() {
        "task" | "agenttool" => "agent".to_string(),
        "ls" | "listdir" => "glob".to_string(),
        "readfile" | "filereadtool" => "read".to_string(),
        "replaceinfile" | "fileedittool" => "edit".to_string(),
        "writefile" | "filewritetool" => "write".to_string(),
        "searchtext" => "grep".to_string(),
        "agentoutputtool" | "bashoutputtool" => "taskoutput".to_string(),
        "killshell" => "taskstop".to_string(),
        "brief" => "sendusermessage".to_string(),
        "readmcpresource" => "readmcpresourcetool".to_string(),
        "listmcpresources" => "listmcpresourcestool".to_string(),
        other => other.to_string(),
    }
}

/// Returns true when a loaded tool definition matches the provided selector.
pub(crate) fn tool_definition_matches_selector(
    definition: &ToolDefinition,
    selector: &str,
) -> bool {
    let selector = canonical_tool_name(selector);
    canonical_definition_names(definition)
        .into_iter()
        .any(|candidate| candidate == selector)
}

/// Returns true when a declarative tool resource matches the provided selector.
pub(crate) fn tool_spec_matches_selector(tool: &ToolSpec, selector: &str) -> bool {
    let selector = canonical_tool_name(selector);
    canonical_spec_names(tool)
        .into_iter()
        .any(|candidate| candidate == selector)
}

fn canonical_definition_names(definition: &ToolDefinition) -> Vec<String> {
    canonical_names(
        std::iter::once(definition.id.as_str())
            .chain(std::iter::once(definition.name.as_str()))
            .chain(std::iter::once(definition.handler.as_str()))
            .chain(definition.aliases.iter().map(String::as_str)),
    )
}

fn canonical_spec_names(tool: &ToolSpec) -> Vec<String> {
    canonical_names(
        std::iter::once(tool.id.as_str())
            .chain(std::iter::once(tool.name.as_str()))
            .chain(std::iter::once(tool.handler.as_str()))
            .chain(tool.aliases.iter().map(String::as_str)),
    )
}

fn canonical_names<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    values
        .into_iter()
        .map(canonical_tool_name)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::canonical_tool_name;

    #[test]
    fn canonical_tool_name_maps_provider_and_legacy_aliases() {
        assert_eq!(canonical_tool_name("Task"), "agent");
        assert_eq!(canonical_tool_name("read_file"), "read");
        assert_eq!(canonical_tool_name("replace_in_file"), "edit");
        assert_eq!(canonical_tool_name("write_file"), "write");
        assert_eq!(canonical_tool_name("list_dir"), "glob");
        assert_eq!(canonical_tool_name("search_text"), "grep");
        assert_eq!(canonical_tool_name("AgentOutputTool"), "taskoutput");
        assert_eq!(canonical_tool_name("KillShell"), "taskstop");
        assert_eq!(canonical_tool_name("Brief"), "sendusermessage");
    }
}
