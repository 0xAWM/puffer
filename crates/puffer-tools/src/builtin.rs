use crate::model::{ToolCall, ToolExecutionContext, ToolExecutionOutput, ToolExecutionStatus, ToolInput};
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Executes one built-in tool call using the provided execution context.
pub fn execute_builtin_tool(
    call: &ToolCall,
    context: &ToolExecutionContext,
) -> Result<ToolExecutionOutput> {
    match &call.input {
        ToolInput::Bash { command } => execute_bash(&call.tool_id, command, context),
        ToolInput::ReadFile { path } => execute_read_file(&call.tool_id, path, context),
        ToolInput::WriteFile { path, content } => {
            execute_write_file(&call.tool_id, path, content, context)
        }
    }
}

/// Returns a concise human-readable summary for a tool result.
pub fn tool_output_summary(output: &ToolExecutionOutput) -> String {
    let status = match output.status {
        ToolExecutionStatus::Success => "ok",
        ToolExecutionStatus::Error => "error",
    };
    format!("tool={} status={status}", output.tool_id)
}

fn execute_bash(
    tool_id: &str,
    command: &str,
    context: &ToolExecutionContext,
) -> Result<ToolExecutionOutput> {
    let output = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .current_dir(&context.cwd)
        .output()
        .with_context(|| format!("failed to run shell command `{command}`"))?;
    let status = if output.status.success() {
        ToolExecutionStatus::Success
    } else {
        ToolExecutionStatus::Error
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(ToolExecutionOutput {
        tool_id: tool_id.to_string(),
        status,
        text: format!("{stdout}{stderr}"),
    })
}

fn execute_read_file(
    tool_id: &str,
    path: &str,
    context: &ToolExecutionContext,
) -> Result<ToolExecutionOutput> {
    let path = resolve_path(path, context);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    Ok(ToolExecutionOutput {
        tool_id: tool_id.to_string(),
        status: ToolExecutionStatus::Success,
        text,
    })
}

fn execute_write_file(
    tool_id: &str,
    path: &str,
    content: &str,
    context: &ToolExecutionContext,
) -> Result<ToolExecutionOutput> {
    let path = resolve_path(path, context);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(ToolExecutionOutput {
        tool_id: tool_id.to_string(),
        status: ToolExecutionStatus::Success,
        text: format!("wrote {}", path.display()),
    })
}

fn resolve_path(path: &str, context: &ToolExecutionContext) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        context.cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn read_and_write_file_tools_round_trip() {
        let temp = tempdir().unwrap();
        let context = ToolExecutionContext {
            cwd: temp.path().to_path_buf(),
        };
        let write = execute_builtin_tool(
            &ToolCall {
                tool_id: "write_file".to_string(),
                input: ToolInput::WriteFile {
                    path: "note.txt".to_string(),
                    content: "hello".to_string(),
                },
            },
            &context,
        )
        .unwrap();
        assert_eq!(write.status, ToolExecutionStatus::Success);

        let read = execute_builtin_tool(
            &ToolCall {
                tool_id: "read_file".to_string(),
                input: ToolInput::ReadFile {
                    path: "note.txt".to_string(),
                },
            },
            &context,
        )
        .unwrap();
        assert_eq!(read.text, "hello");
    }

    #[test]
    fn bash_tool_captures_stdout() {
        let temp = tempdir().unwrap();
        let context = ToolExecutionContext {
            cwd: temp.path().to_path_buf(),
        };
        let result = execute_builtin_tool(
            &ToolCall {
                tool_id: "bash".to_string(),
                input: ToolInput::Bash {
                    command: "printf 'hi'".to_string(),
                },
            },
            &context,
        )
        .unwrap();
        assert_eq!(result.status, ToolExecutionStatus::Success);
        assert_eq!(result.text, "hi");
    }
}
