mod dtos;
mod repo_actions;
mod session_data;

use crate::dtos::{ActionResultDto, FolderGroupDto, RepoStatusDto, SessionViewDto};
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use tauri::Builder;

#[tauri::command]
fn list_session_groups(workspace_root: Option<String>) -> Result<Vec<FolderGroupDto>, String> {
    session_data::list_session_groups(workspace_root).map_err(|error| error.to_string())
}

#[tauri::command]
fn load_session_view(
    workspace_root: Option<String>,
    session_id: String,
) -> Result<SessionViewDto, String> {
    session_data::load_session_view(workspace_root, session_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_repo_status(
    workspace_root: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
) -> Result<RepoStatusDto, String> {
    let cwd = resolve_cwd(workspace_root, session_id, cwd).map_err(|error| error.to_string())?;
    repo_actions::load_repo_status(cwd).map_err(|error| error.to_string())
}

#[tauri::command]
fn create_pull_request(
    workspace_root: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
    title: Option<String>,
    body: Option<String>,
) -> Result<ActionResultDto, String> {
    let cwd = resolve_cwd(workspace_root, session_id, cwd).map_err(|error| error.to_string())?;
    repo_actions::create_pull_request(cwd, title, body).map_err(|error| error.to_string())
}

#[tauri::command]
fn merge_pull_request(
    workspace_root: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
    pull_request_number: Option<u64>,
    merge_method: Option<String>,
) -> Result<ActionResultDto, String> {
    let cwd = resolve_cwd(workspace_root, session_id, cwd).map_err(|error| error.to_string())?;
    repo_actions::merge_pull_request(cwd, pull_request_number, merge_method.as_deref())
        .map_err(|error| error.to_string())
}

fn resolve_cwd(
    workspace_root: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
) -> Result<PathBuf> {
    if let Some(cwd) = cwd {
        return Ok(PathBuf::from(cwd));
    }
    let Some(session_id) = session_id else {
        return Err(anyhow!("missing `session_id` or `cwd` argument"));
    };
    session_data::load_session_cwd(workspace_root, &session_id)
}

/// Runs the Puffer Desktop Tauri host.
pub fn run() {
    Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_session_groups,
            load_session_view,
            get_repo_status,
            create_pull_request,
            merge_pull_request
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Puffer Desktop");
}
