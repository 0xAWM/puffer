use crate::AppState;
use anyhow::{Context, Result};
use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

const DEFAULT_PLAN_TEXT: &str = "# Current Plan\n\n- Add concrete implementation steps here.\n";

/// Returns true when the plan body contains user-authored content beyond the default scaffold.
pub(crate) fn plan_has_user_content(plan_body: &str) -> bool {
    let trimmed = plan_body.trim();
    !trimmed.is_empty() && trimmed != DEFAULT_PLAN_TEXT.trim()
}

/// Returns the session-scoped plan file path used by plan mode and workflow tools.
pub(crate) fn plan_file_path(state: &AppState) -> Result<PathBuf> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let plan_dir = paths.workspace_config_dir.join("plans");
    fs::create_dir_all(&plan_dir)
        .with_context(|| format!("failed to create {}", plan_dir.display()))?;
    Ok(plan_dir.join(format!("{}.md", state.session.id)))
}

/// Ensures the session-scoped plan file exists and returns its path.
#[cfg(test)]
pub(crate) fn ensure_plan_file(state: &AppState) -> Result<PathBuf> {
    let path = plan_file_path(state)?;
    if !path.exists() {
        fs::write(&path, DEFAULT_PLAN_TEXT)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(path)
}

/// Loads the current plan contents when a plan file has already been written.
pub(crate) fn read_plan_text(state: &AppState) -> Result<Option<String>> {
    let path = plan_file_path(state)?;
    match fs::read_to_string(&path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

/// Persists updated plan contents to the session-scoped plan file.
#[cfg(test)]
pub(crate) fn persist_plan_output(state: &AppState, plan_text: &str) -> Result<PathBuf> {
    let path = plan_file_path(state)?;
    fs::write(&path, plan_text).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::{ensure_plan_file, plan_file_path, plan_has_user_content, DEFAULT_PLAN_TEXT};
    use crate::AppState;
    use puffer_config::{ensure_workspace_dirs, ConfigPaths, PufferConfig};
    use puffer_session_store::SessionStore;
    use tempfile::tempdir;

    fn state() -> AppState {
        let tempdir = tempdir().unwrap();
        let root = tempdir.keep();
        let paths = ConfigPaths::discover(&root);
        ensure_workspace_dirs(&paths).unwrap();
        let session_store = SessionStore::from_paths(&paths).unwrap();
        let session = session_store.create_session(root.clone()).unwrap();
        AppState::new(PufferConfig::default(), root, session)
    }

    #[test]
    fn plan_file_path_does_not_materialize_the_file() {
        let state = state();
        let path = plan_file_path(&state).unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn ensure_plan_file_writes_the_default_scaffold() {
        let state = state();
        let path = ensure_plan_file(&state).unwrap();

        assert_eq!(std::fs::read_to_string(path).unwrap(), DEFAULT_PLAN_TEXT);
    }

    #[test]
    fn plan_has_user_content_ignores_the_default_scaffold() {
        assert!(!plan_has_user_content(DEFAULT_PLAN_TEXT));
        assert!(plan_has_user_content(
            "# Current Plan\n\n1. Verify the fix.\n"
        ));
    }
}
