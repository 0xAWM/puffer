use crate::{
    BashToolInput, ReadFileToolInput, ToolExecutionResult, ToolInput, ToolKind, ToolOutput,
    WriteFileToolInput,
};
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Executes one built-in tool with typed input under the given working directory.
pub fn execute_builtin_tool(
    tool_id: &str,
    kind: ToolKind,
    cwd: &Path,
    input: ToolInput,
) -> Result<ToolExecutionResult> {
    match (kind, input) {
        (ToolKind::Bash, ToolInput::Bash(input)) => execute_bash(tool_id, cwd, input),
        (ToolKind::ReadFile, ToolInput::ReadFile(input)) => execute_read_file(tool_id, cwd, input),
        (ToolKind::WriteFile, ToolInput::WriteFile(input)) => {
            execute_write_file(tool_id, cwd, input)
        }
        (expected, actual) => Err(anyhow!(
            "tool input mismatch for {tool_id}: expected {:?}, got {:?}",
            expected, actual
        )),
    }
}

fn execute_bash(tool_id: &str, cwd: &Path, input: BashToolInput) -> Result<ToolExecutionResult> {
    let output = Command::new("sh")
        .arg("-lc")
        .arg(&input.command)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to execute bash tool in {}", cwd.display()))?;
    Ok(ToolExecutionResult {
        tool_id: tool_id.to_string(),
        success: output.status.success(),
        output: ToolOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            metadata: serde_json::json!({
                "status_code": output.status.code(),
                "command": input.command,
            }),
        },
    })
}

fn execute_read_file(
    tool_id: &str,
    cwd: &Path,
    input: ReadFileToolInput,
) -> Result<ToolExecutionResult> {
    let path = absolutize(cwd, &input.path);
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read file {}", path.display()))?;
    Ok(ToolExecutionResult {
        tool_id: tool_id.to_string(),
        success: true,
        output: ToolOutput {
            stdout: contents,
            stderr: String::new(),
            metadata: serde_json::json!({
                "path": path,
            }),
        },
    })
}

fn execute_write_file(
    tool_id: &str,
    cwd: &Path,
    input: WriteFileToolInput,
) -> Result<ToolExecutionResult> {
    let path = absolutize(cwd, &input.path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent dir {}", parent.display()))?;
    }
    fs::write(&path, &input.contents)
        .with_context(|| format!("failed to write file {}", path.display()))?;
    Ok(ToolExecutionResult {
        tool_id: tool_id.to_string(),
        success: true,
        output: ToolOutput {
            stdout: format!("wrote {}", path.display()),
            stderr: String::new(),
            metadata: serde_json::json!({
                "path": path,
                "bytes_written": input.contents.len(),
            }),
        },
    })
}

fn absolutize(cwd: &Path, path: &Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_and_write_tools_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let path = std::path::PathBuf::from("note.txt");
        let write = execute_builtin_tool(
            "write_file",
            ToolKind::WriteFile,
            temp.path(),
            ToolInput::WriteFile(WriteFileToolInput {
                path: path.clone(),
                contents: "hello".to_string(),
            }),
        )
        .unwrap();
        assert!(write.success);

        let read = execute_builtin_tool(
            "read_file",
            ToolKind::ReadFile,
            temp.path(),
            ToolInput::ReadFile(ReadFileToolInput { path }),
        )
        .unwrap();
        assert_eq!(read.output.stdout, "hello");
    }
}
