use crate::dtos::{
    DiffSnapshotDto, FolderGroupDto, PermissionDialogDto, SessionListItemDto, SessionViewDto,
    TimelineItemDto,
};
use crate::repo_actions::load_repo_status;
use anyhow::{anyhow, Context, Result};
use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use puffer_session_store::{
    GitDiffSnapshot, SessionRecord, SessionStore, SessionSummary, TranscriptEvent,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Lists desktop sessions grouped by their top-level workspace folder.
pub(crate) fn list_session_groups(workspace_root: Option<String>) -> Result<Vec<FolderGroupDto>> {
    let workspace_root = resolve_workspace_root(workspace_root)?;
    let store = open_store(&workspace_root)?;
    let mut groups: BTreeMap<String, FolderGroupDto> = BTreeMap::new();

    for session in store.list_sessions()? {
        let folder_path = session_group_path(&session.cwd, &workspace_root);
        let folder_key = folder_path.display().to_string();
        let entry = groups.entry(folder_key.clone()).or_insert_with(|| FolderGroupDto {
            id: sanitize_folder_id(&folder_key),
            label: folder_label(&folder_path),
            path: folder_key.clone(),
            sessions: Vec::new(),
        });
        entry.sessions.push(session_summary_dto(&session));
    }

    for group in groups.values_mut() {
        group.sessions.sort_by(|left, right| {
            right
                .updated_at_ms
                .cmp(&left.updated_at_ms)
                .then_with(|| left.title.cmp(&right.title))
        });
    }

    Ok(groups.into_values().collect())
}

/// Loads the desktop session view for a single session id.
pub(crate) fn load_session_view(
    workspace_root: Option<String>,
    session_id: String,
) -> Result<SessionViewDto> {
    let workspace_root = resolve_workspace_root(workspace_root)?;
    let store = open_store(&workspace_root)?;
    let session_uuid = Uuid::parse_str(&session_id)
        .with_context(|| format!("invalid session id `{session_id}`"))?;
    let record = store.load_session(session_uuid)?;
    let summary = store
        .list_sessions()?
        .into_iter()
        .find(|session| session.id == session_uuid)
        .ok_or_else(|| anyhow!("session `{session_id}` not found"))?;
    let diff_history = diff_history(&record);
    let latest_diff = diff_history.first().cloned();
    let repo_status = load_repo_status(record.metadata.cwd.clone())?;

    Ok(SessionViewDto {
        session: session_summary_dto(&summary),
        timeline: timeline_items(&record),
        latest_diff,
        diff_history,
        repo_status,
    })
}

/// Loads the working directory for a session id.
pub(crate) fn load_session_cwd(workspace_root: Option<String>, session_id: &str) -> Result<PathBuf> {
    let workspace_root = resolve_workspace_root(workspace_root)?;
    let store = open_store(&workspace_root)?;
    let session_uuid = Uuid::parse_str(session_id)
        .with_context(|| format!("invalid session id `{session_id}`"))?;
    let session = store.load_session(session_uuid)?;
    Ok(session.metadata.cwd)
}

fn open_store(workspace_root: &Path) -> Result<SessionStore> {
    let paths = ConfigPaths::discover(workspace_root);
    ensure_workspace_dirs(&paths)?;
    SessionStore::from_paths(&paths).context("failed to open session store")
}

fn resolve_workspace_root(workspace_root: Option<String>) -> Result<PathBuf> {
    workspace_root
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(|| {
            std::env::current_dir().context("failed to resolve current working directory")
        })
}

fn session_summary_dto(session: &SessionSummary) -> SessionListItemDto {
    SessionListItemDto {
        id: session.id.to_string(),
        title: session
            .display_name
            .clone()
            .or_else(|| session.slug.clone())
            .or_else(|| session.cwd.file_name().map(|name| name.to_string_lossy().to_string()))
            .unwrap_or_else(|| session.id.to_string()),
        display_name: session.display_name.clone(),
        cwd: session.cwd.display().to_string(),
        updated_at_ms: session.updated_at_ms,
        created_at_ms: session.created_at_ms,
        event_count: session.event_count,
        slug: session.slug.clone(),
        tags: session.tags.clone(),
        note: session.note.clone(),
    }
}

fn session_group_path(cwd: &Path, workspace_root: &Path) -> PathBuf {
    if let Ok(relative) = cwd.strip_prefix(workspace_root) {
        if let Some(first) = relative.components().next() {
            return workspace_root.join(first.as_os_str());
        }
    }
    cwd.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| cwd.to_path_buf())
}

fn folder_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

fn sanitize_folder_id(path: &str) -> String {
    path.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn timeline_items(record: &SessionRecord) -> Vec<TimelineItemDto> {
    record
        .events
        .iter()
        .enumerate()
        .flat_map(|(index, event)| timeline_item(index, event))
        .collect()
}

fn timeline_item(index: usize, event: &TranscriptEvent) -> Vec<TimelineItemDto> {
    match event {
        TranscriptEvent::UserMessage { text } => vec![message_item(
            format!("user-{index}"),
            "user",
            "User",
            text.clone(),
        )],
        TranscriptEvent::AssistantMessage { text } => vec![message_item(
            format!("assistant-{index}"),
            "assistant",
            "Assistant",
            text.clone(),
        )],
        TranscriptEvent::SystemMessage { text } => parse_system_message(index, text),
        TranscriptEvent::GitDiffSnapshot { snapshot } => vec![TimelineItemDto {
            id: format!("diff-{index}"),
            kind: "diff".to_string(),
            title: snapshot.command.clone(),
            summary: snapshot.status.clone(),
            body: snapshot.patch_excerpt.clone(),
            meta: vec![snapshot.command.clone()],
            status: None,
            input: None,
            output: None,
            tool_name: None,
            permission_dialog: None,
        }],
        TranscriptEvent::SessionRenamed { name } => vec![message_item(
            format!("rename-{index}"),
            "system",
            "Session Renamed",
            format!("Renamed to {name}"),
        )],
        TranscriptEvent::CommandInvoked { name, args } => vec![TimelineItemDto {
            id: format!("command-{index}"),
            kind: "command".to_string(),
            title: format!("/{name}"),
            summary: args.clone(),
            body: args.clone(),
            meta: vec!["slash-command".to_string()],
            status: None,
            input: None,
            output: None,
            tool_name: None,
            permission_dialog: None,
        }],
        TranscriptEvent::StateSnapshot {
            current_model,
            current_provider,
            sandbox_mode,
            plan_mode,
            working_dirs,
            ..
        } => {
            let mut lines = Vec::new();
            if let Some(provider) = current_provider {
                lines.push(format!("Provider: {provider}"));
            }
            if let Some(model) = current_model {
                lines.push(format!("Model: {model}"));
            }
            lines.push(format!("Sandbox: {sandbox_mode}"));
            lines.push(format!("Plan mode: {plan_mode}"));
            if !working_dirs.is_empty() {
                lines.push(format!("Working dirs: {}", working_dirs.join(", ")));
            }
            vec![message_item(
                format!("state-{index}"),
                "system",
                "Runtime State",
                lines.join("\n"),
            )]
        }
        TranscriptEvent::TranscriptRewritten { .. } => Vec::new(),
    }
}

fn message_item(id: String, kind: &str, title: &str, body: String) -> TimelineItemDto {
    TimelineItemDto {
        id,
        kind: kind.to_string(),
        title: title.to_string(),
        summary: body.lines().next().unwrap_or(title).to_string(),
        body,
        meta: Vec::new(),
        status: None,
        input: None,
        output: None,
        tool_name: None,
        permission_dialog: None,
    }
}

fn parse_system_message(index: usize, text: &str) -> Vec<TimelineItemDto> {
    let Some(header) = text.lines().next() else {
        return Vec::new();
    };
    if let Some(rest) = header.strip_prefix("Tool ") {
        let Some((tool_id, status_suffix)) = rest.rsplit_once(" [") else {
            return vec![message_item(
                format!("system-{index}"),
                "system",
                "System",
                text.to_string(),
            )];
        };
        let status = status_suffix.trim_end_matches(']').to_string();
        let remaining = text.lines().skip(1).collect::<Vec<_>>().join("\n");
        let (input, output) = parse_tool_io(&remaining);
        let output_text = output.clone().unwrap_or_default();
        let permission_dialog = permission_dialog_for_output(&output_text);
        let mut items = vec![TimelineItemDto {
            id: format!("tool-{index}"),
            kind: "tool".to_string(),
            title: format!("Tool call: {tool_id}"),
            summary: format!("{tool_id} returned {status}"),
            body: if output_text.is_empty() {
                "No tool output recorded.".to_string()
            } else {
                output_text.clone()
            },
            meta: vec![tool_id.to_string(), status.clone()],
            status: Some(status),
            input,
            output,
            tool_name: Some(tool_id.to_string()),
            permission_dialog: permission_dialog.clone(),
        }];
        if let Some(dialog) = permission_dialog {
            items.push(TimelineItemDto {
                id: format!("permission-{index}"),
                kind: "permission".to_string(),
                title: "Permission request".to_string(),
                summary: dialog.message.clone(),
                body: [
                    Some(format!("Tool: {tool_id}")),
                    dialog
                        .scope_label
                        .as_ref()
                        .map(|scope| format!("Scope: {scope}")),
                    Some(dialog.message.clone()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("\n"),
                meta: vec![dialog.status.clone()],
                status: Some(dialog.status.clone()),
                input: None,
                output: None,
                tool_name: Some(tool_id.to_string()),
                permission_dialog: Some(dialog),
            });
        }
        return items;
    }

    vec![message_item(
        format!("system-{index}"),
        "system",
        "System",
        text.to_string(),
    )]
}

fn parse_tool_io(body: &str) -> (Option<String>, Option<String>) {
    let Some(input) = body.strip_prefix("input: ") else {
        let output = if body.trim().is_empty() {
            None
        } else {
            Some(body.to_string())
        };
        return (None, output);
    };
    match input.split_once('\n') {
        Some((input_text, output)) => {
            let output = if output.trim().is_empty() {
                None
            } else {
                Some(output.to_string())
            };
            (Some(input_text.to_string()), output)
        }
        None => (Some(input.to_string()), None),
    }
}

fn permission_dialog_for_output(output: &str) -> Option<PermissionDialogDto> {
    if output.starts_with("Permission required:") {
        return Some(PermissionDialogDto {
            status: "required".to_string(),
            message: output.to_string(),
            scope_label: Some("workspace".to_string()),
            choices: default_permission_choices(),
        });
    }
    if output.starts_with("Permission denied:") {
        return Some(PermissionDialogDto {
            status: "denied".to_string(),
            message: output.to_string(),
            scope_label: Some("workspace".to_string()),
            choices: default_permission_choices(),
        });
    }
    None
}

fn default_permission_choices() -> Vec<String> {
    vec![
        "Allow once".to_string(),
        "Allow for session".to_string(),
        "Deny".to_string(),
    ]
}

fn diff_history(record: &SessionRecord) -> Vec<DiffSnapshotDto> {
    record
        .events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            TranscriptEvent::GitDiffSnapshot { snapshot } => Some(diff_snapshot(index, snapshot)),
            _ => None,
        })
        .rev()
        .collect()
}

fn diff_snapshot(index: usize, snapshot: &GitDiffSnapshot) -> DiffSnapshotDto {
    DiffSnapshotDto {
        id: format!("diff-{index}"),
        title: snapshot.command.clone(),
        command: snapshot.command.clone(),
        status: snapshot.status.clone(),
        unstaged_diffstat: snapshot.unstaged_diffstat.clone(),
        staged_diffstat: snapshot.staged_diffstat.clone(),
        patch_excerpt: snapshot.patch_excerpt.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{diff_history, parse_system_message, session_group_path};
    use puffer_session_store::{GitDiffSnapshot, SessionMetadata, SessionRecord, TranscriptEvent};
    use std::path::Path;
    use uuid::Uuid;

    #[test]
    fn groups_session_by_first_workspace_segment() {
        let path = session_group_path(
            Path::new("/tmp/workspace/repo/src"),
            Path::new("/tmp/workspace"),
        );
        assert_eq!(path, Path::new("/tmp/workspace/repo"));
    }

    #[test]
    fn parses_permission_tool_message_into_tool_and_permission_cards() {
        let items = parse_system_message(
            4,
            "Tool Edit [error]\ninput: {\"path\":\"src/lib.rs\"}\nPermission required: workspace permission rule requires approval",
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "tool");
        assert_eq!(items[1].kind, "permission");
        assert_eq!(items[1].tool_name.as_deref(), Some("Edit"));
    }

    #[test]
    fn diff_history_returns_latest_snapshot_first() {
        let record = SessionRecord {
            metadata: SessionMetadata {
                id: Uuid::new_v4(),
                cwd: "/tmp/workspace".into(),
                created_at_ms: 0,
                updated_at_ms: 0,
                display_name: None,
                parent_session_id: None,
                slug: None,
                tags: Vec::new(),
                note: None,
            },
            events: vec![
                TranscriptEvent::GitDiffSnapshot {
                    snapshot: GitDiffSnapshot {
                        command: "/diff".to_string(),
                        status: "first".to_string(),
                        unstaged_diffstat: String::new(),
                        staged_diffstat: String::new(),
                        patch_excerpt: "first".to_string(),
                    },
                },
                TranscriptEvent::GitDiffSnapshot {
                    snapshot: GitDiffSnapshot {
                        command: "/review".to_string(),
                        status: "second".to_string(),
                        unstaged_diffstat: String::new(),
                        staged_diffstat: String::new(),
                        patch_excerpt: "second".to_string(),
                    },
                },
            ],
        };
        let history = diff_history(&record);
        assert_eq!(history[0].status, "second");
        assert_eq!(history[1].status, "first");
    }
}
