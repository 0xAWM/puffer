use crate::{ToolDefinition, ToolExecutionResult, ToolOutput};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExternalCommandHandler {
    program: String,
    args: Vec<String>,
}

impl ExternalCommandHandler {
    fn from_definition(definition: &ToolDefinition) -> Option<Self> {
        if definition.handler == "exec" {
            let (program, args) = definition.handler_args.split_first()?;
            return Some(Self {
                program: program.clone(),
                args: args.to_vec(),
            });
        }
        definition
            .handler
            .strip_prefix("exec:")
            .map(|program| Self {
                program: program.to_string(),
                args: definition.handler_args.clone(),
            })
    }

    fn execute(
        &self,
        definition: &ToolDefinition,
        cwd: &Path,
        input: Value,
    ) -> Result<ToolExecutionResult> {
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .env("PUFFER_TOOL_ID", &definition.id)
            .env("PUFFER_TOOL_NAME", &definition.name)
            .env("PUFFER_TOOL_HANDLER", &definition.handler)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to spawn external tool handler {}", self.program))?;
        if let Some(stdin) = child.stdin.as_mut() {
            let payload = serde_json::to_vec(&input)?;
            stdin.write_all(&payload).with_context(|| {
                format!("failed to write external tool input for {}", definition.id)
            })?;
        }
        let output = child
            .wait_with_output()
            .with_context(|| format!("failed to wait for external tool {}", definition.id))?;
        let metadata = parse_output_metadata(&output.stdout);
        Ok(ToolExecutionResult {
            tool_id: definition.id.clone(),
            success: output.status.success(),
            output: ToolOutput {
                stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                metadata,
            },
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ToolRuntime {
    Builtin(crate::ToolKind),
    External(ExternalCommandHandler),
    SharedLibrary(String),
}

pub(crate) fn runtime_from_definition(definition: &ToolDefinition) -> Result<ToolRuntime> {
    if let Some(kind) = crate::builtin_tool_kind(&builtin_handler_name(&definition.handler)) {
        return Ok(ToolRuntime::Builtin(kind));
    }
    if let Some(handler) = ExternalCommandHandler::from_definition(definition) {
        return Ok(ToolRuntime::External(handler));
    }
    if let Some(path) = &definition.shared_lib {
        return Ok(ToolRuntime::SharedLibrary(path.clone()));
    }
    Err(anyhow!("unsupported tool handler {}", definition.handler))
}

pub(crate) fn execute_runtime(
    runtime: &ToolRuntime,
    definition: &ToolDefinition,
    cwd: &Path,
    input: Value,
) -> Result<ToolExecutionResult> {
    match runtime {
        ToolRuntime::Builtin(kind) => {
            let typed = crate::parse_builtin_input(*kind, input)?;
            crate::execute_builtin_tool(&definition.id, *kind, cwd, typed)
        }
        ToolRuntime::External(handler) => handler.execute(definition, cwd, input),
        ToolRuntime::SharedLibrary(path) => Err(anyhow!(
            "shared library tool handlers are not implemented yet for {} ({path})",
            definition.id
        )),
    }
}

pub(crate) fn builtin_handler_name(handler: &str) -> String {
    handler
        .strip_prefix("builtin:")
        .unwrap_or(handler)
        .to_string()
}

fn parse_output_metadata(stdout: &[u8]) -> Value {
    serde_json::from_slice::<Value>(stdout).unwrap_or_else(|_| Value::Null)
}
