use crate::normalize_snapshot_text;
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::Path;

const UPDATE_ENV: &str = "PUFFER_UPDATE_SNAPSHOTS";

/// Writes a normalized snapshot file, creating parent directories when needed.
pub fn write_normalized_snapshot(snapshot_path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create snapshot parent directory `{}`",
                parent.display()
            )
        })?;
    }
    fs::write(snapshot_path, normalize_snapshot_text(contents)).with_context(|| {
        format!(
            "failed to write normalized snapshot `{}`",
            snapshot_path.display()
        )
    })?;
    Ok(())
}

/// Reads and normalizes an existing snapshot file.
pub fn read_normalized_snapshot(snapshot_path: &Path) -> Result<String> {
    let contents = fs::read_to_string(snapshot_path)
        .with_context(|| format!("failed to read snapshot `{}`", snapshot_path.display()))?;
    Ok(normalize_snapshot_text(&contents))
}

/// Asserts that a normalized snapshot matches the provided contents.
///
/// When `PUFFER_UPDATE_SNAPSHOTS=1` is present, the snapshot file is rewritten
/// instead of producing a mismatch error.
pub fn assert_normalized_snapshot(contents: &str, snapshot_path: &Path) -> Result<()> {
    let actual = normalize_snapshot_text(contents);
    if snapshot_updates_enabled() {
        return write_normalized_snapshot(snapshot_path, &actual);
    }
    let expected = read_normalized_snapshot(snapshot_path)?;
    if expected == actual {
        Ok(())
    } else {
        Err(anyhow!(
            "normalized snapshot mismatch for `{}`",
            snapshot_path.display()
        ))
    }
}

fn snapshot_updates_enabled() -> bool {
    matches!(std::env::var(UPDATE_ENV).as_deref(), Ok("1"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_and_reads_normalized_snapshots() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("snapshots/output.txt");
        write_normalized_snapshot(&path, "a  \r\nb\t \r\n").unwrap();
        let roundtrip = read_normalized_snapshot(&path).unwrap();
        assert_eq!(roundtrip, "a\nb");
    }

    #[test]
    fn assert_snapshot_passes_for_matching_normalized_content() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("expected.txt");
        write_normalized_snapshot(&path, "hello\nworld\n").unwrap();
        assert_normalized_snapshot("hello\r\nworld", &path).unwrap();
    }

    #[test]
    fn assert_snapshot_reports_mismatch() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("expected.txt");
        write_normalized_snapshot(&path, "left").unwrap();
        let error = assert_normalized_snapshot("right", &path).unwrap_err();
        assert!(error.to_string().contains("normalized snapshot mismatch"));
    }
}
