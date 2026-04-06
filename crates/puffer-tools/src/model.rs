use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Enumerates the built-in tool handlers supported by the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Bash,
    ReadFile,
    WriteFile,
}

/// Input payload for the `bash` tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BashToolInput {
    pub command: String,
}

/// Input payload for the `read_file` tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadFileToolInput {
    pub path: PathBuf,
}

/// Input payload for the `write_file` tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteFileToolInput {
    pub path: PathBuf,
    pub contents: String,
}

/// Typed execution input for supported tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum ToolInput {
    Bash(BashToolInput),
    ReadFile(ReadFileToolInput),
    WriteFile(WriteFileToolInput),
}

/// Structured output from a tool execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolOutput {
    pub stdout: String,
    pub stderr: String,
    pub metadata: serde_json::Value,
}

/// Full result for one tool execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub tool_id: String,
    pub success: bool,
    pub output: ToolOutput,
}
