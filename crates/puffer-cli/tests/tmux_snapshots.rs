use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use puffer_session_store::SessionStore;
use puffer_test_support::{
    assert_normalized_snapshot, send_tmux_keys, start_tmux_command, temp_workspace, tmux_available,
    wait_for_tmux_text,
};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn tmux_help_matches_snapshot() {
    if !tmux_available() {
        return;
    }

    let (_tempdir, workspace) = configured_workspace();
    let session = start_tmux_command(
        env!("CARGO_BIN_EXE_puffer"),
        &[],
        Some(workspace.as_path()),
    )
    .unwrap();
    wait_for_tmux_text(&session, "Puffer Code", Duration::from_secs(5)).unwrap();
    send_tmux_keys(&session, &["/help", "Enter"]).unwrap();
    let capture =
        wait_for_tmux_text(&session, "Supported commands:", Duration::from_secs(5)).unwrap();
    assert_normalized_snapshot(
        &capture,
        &snapshot_path("tmux_help_snapshot.txt"),
    )
    .unwrap();
}

#[test]
fn tmux_login_overlay_matches_snapshot() {
    if !tmux_available() {
        return;
    }

    let (_tempdir, workspace) = configured_workspace();
    let session = start_tmux_command(
        env!("CARGO_BIN_EXE_puffer"),
        &[],
        Some(workspace.as_path()),
    )
    .unwrap();
    wait_for_tmux_text(&session, "Puffer Code", Duration::from_secs(5)).unwrap();
    send_tmux_keys(&session, &["/login", "Enter"]).unwrap();
    let capture =
        wait_for_tmux_text(&session, "Login Provider", Duration::from_secs(5)).unwrap();
    assert_normalized_snapshot(
        &capture,
        &snapshot_path("tmux_login_overlay_snapshot.txt"),
    )
    .unwrap();
}

fn configured_workspace() -> (tempfile::TempDir, PathBuf) {
    let (tempdir, workspace) = temp_workspace().unwrap();
    let paths = ConfigPaths::discover(&workspace);
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let session = session_store.create_session(workspace.join("dockyard")).unwrap();
    session_store
        .rename_session(session.id, "dockyard".to_string())
        .unwrap();
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    std::os::unix::fs::symlink(repo_root.join("resources"), workspace.join("resources")).unwrap();
    fs::write(
        workspace.join(".puffer/config.toml"),
        r#"
app_name = "Puffer Code"
default_provider = "anthropic"
theme = "puffer"

[mascot]
id = "clawd"
display_name = "Clawd"
enabled = true

[ui]
no_alt_screen = true
tmux_golden_mode = true
"#,
    )
    .unwrap();
    (tempdir, workspace)
}

fn snapshot_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
        .join(name)
}
