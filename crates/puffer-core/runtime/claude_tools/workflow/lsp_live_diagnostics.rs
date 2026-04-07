use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StoredDiagnostic {
    pub(super) message: String,
    pub(super) severity: Option<u64>,
    pub(super) source: Option<String>,
    pub(super) code: Option<String>,
    pub(super) line: usize,
    pub(super) character: usize,
    pub(super) end_line: usize,
    pub(super) end_character: usize,
}

type DiagnosticMap = BTreeMap<PathBuf, BTreeMap<String, Vec<StoredDiagnostic>>>;

pub(super) fn record_publish_diagnostics(
    workspace_root: &Path,
    uri: &str,
    diagnostics: &[Value],
) -> anyhow::Result<()> {
    let mut state = diagnostics_state()
        .lock()
        .map_err(|_| anyhow::anyhow!("LSP diagnostics state lock poisoned"))?;
    let workspace = state.entry(workspace_root.to_path_buf()).or_default();
    workspace.insert(
        uri.to_string(),
        diagnostics
            .iter()
            .filter_map(parse_diagnostic)
            .collect::<Vec<_>>(),
    );
    Ok(())
}

pub(super) fn diagnostics_for_file(
    workspace_root: &Path,
    uri: &str,
) -> anyhow::Result<Vec<StoredDiagnostic>> {
    let state = diagnostics_state()
        .lock()
        .map_err(|_| anyhow::anyhow!("LSP diagnostics state lock poisoned"))?;
    Ok(state
        .get(workspace_root)
        .and_then(|workspace| workspace.get(uri))
        .cloned()
        .unwrap_or_default())
}

pub(super) fn clear_workspace_diagnostics(workspace_root: &Path) -> anyhow::Result<()> {
    let mut state = diagnostics_state()
        .lock()
        .map_err(|_| anyhow::anyhow!("LSP diagnostics state lock poisoned"))?;
    state.remove(workspace_root);
    Ok(())
}

pub(super) fn clear_all_diagnostics() -> anyhow::Result<()> {
    let mut state = diagnostics_state()
        .lock()
        .map_err(|_| anyhow::anyhow!("LSP diagnostics state lock poisoned"))?;
    state.clear();
    Ok(())
}

fn diagnostics_state() -> &'static Mutex<DiagnosticMap> {
    static STATE: OnceLock<Mutex<DiagnosticMap>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn parse_diagnostic(value: &Value) -> Option<StoredDiagnostic> {
    let range = value.get("range")?;
    let start = range.get("start")?;
    let end = range.get("end")?;
    Some(StoredDiagnostic {
        message: value.get("message")?.as_str()?.to_string(),
        severity: value.get("severity").and_then(Value::as_u64),
        source: value.get("source").and_then(Value::as_str).map(ToOwned::to_owned),
        code: value.get("code").map(|code| match code {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            _ => String::new(),
        }).filter(|value| !value.is_empty()),
        line: start.get("line").and_then(Value::as_u64).unwrap_or(0) as usize + 1,
        character: start
            .get("character")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize
            + 1,
        end_line: end.get("line").and_then(Value::as_u64).unwrap_or(0) as usize + 1,
        end_character: end
            .get("character")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize
            + 1,
    })
}
