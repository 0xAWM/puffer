mod auth_data;
mod dtos;
mod repo_actions;
mod session_data;
mod settings_data;

use crate::dtos::{
    FolderGroupDto, RepoActionResultDto, RepoStatusDto, SessionDetailDto, SettingsSnapshotDto,
};
use anyhow::Result;
use tauri::Builder;

#[tauri::command]
fn list_grouped_sessions() -> Result<Vec<FolderGroupDto>, String> {
    session_data::list_grouped_sessions().map_err(|error| error.to_string())
}

#[tauri::command]
fn load_session_detail(session_id: String) -> Result<SessionDetailDto, String> {
    session_data::load_session_detail(&session_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn refresh_repo_status(session_id: String) -> Result<RepoStatusDto, String> {
    let cwd = session_data::load_session_cwd(&session_id).map_err(|error| error.to_string())?;
    Ok(repo_actions::repo_status(&session_id, &cwd))
}

#[tauri::command]
fn create_pull_request(
    session_id: String,
    title: Option<String>,
    body: Option<String>,
) -> Result<RepoActionResultDto, String> {
    let cwd = session_data::load_session_cwd(&session_id).map_err(|error| error.to_string())?;
    Ok(repo_actions::create_pull_request(
        &session_id,
        &cwd,
        title,
        body,
    ))
}

#[tauri::command]
fn merge_pull_request(
    session_id: String,
    pull_request_number: Option<u64>,
    merge_method: Option<String>,
) -> Result<RepoActionResultDto, String> {
    let cwd = session_data::load_session_cwd(&session_id).map_err(|error| error.to_string())?;
    Ok(repo_actions::merge_pull_request(
        &session_id,
        &cwd,
        pull_request_number,
        merge_method,
    ))
}

#[tauri::command]
fn load_settings_snapshot() -> Result<SettingsSnapshotDto, String> {
    settings_data::load_settings_snapshot().map_err(|error| error.to_string())
}

#[tauri::command]
fn login_with_oauth(provider_id: String) -> Result<SettingsSnapshotDto, String> {
    auth_data::login_with_oauth(&provider_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn login_with_api_key(provider_id: String, api_key: String) -> Result<SettingsSnapshotDto, String> {
    auth_data::login_with_api_key(&provider_id, &api_key).map_err(|error| error.to_string())
}

#[tauri::command]
fn logout_provider(provider_id: String) -> Result<SettingsSnapshotDto, String> {
    auth_data::logout_provider(&provider_id).map_err(|error| error.to_string())
}

/// Runs the Puffer Desktop Tauri host.
pub fn run() {
    Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_grouped_sessions,
            load_session_detail,
            refresh_repo_status,
            create_pull_request,
            merge_pull_request,
            load_settings_snapshot,
            login_with_oauth,
            login_with_api_key,
            logout_provider
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Puffer Desktop");
}
