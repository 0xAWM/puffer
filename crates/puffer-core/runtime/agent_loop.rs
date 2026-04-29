//! Provider-agnostic turn loop driver.
//!
//! Mirrors pi-mono's `agent-loop.ts` shape: this module owns the
//! turn-by-turn driver — tool execution, reflection observation, and
//! compaction. Providers only perform a single round-trip mapping
//! `(messages, tools) → response items + pending tool calls`.
//!
//! The seam is the [`TurnSession`] trait. Each provider builds a
//! session that captures its vendor-specific setup (auth, URL, headers,
//! serialized tools, system blocks) once per user prompt, then exposes
//! neutral methods (`one_turn_streaming`, `generate_summary`,
//! `tool_execution_backend`) that the driver calls per iteration.
//!
//! What stays in the provider:
//! - HTTP request/response shape, SSE parsing, vendor JSON synthesis
//! - Auth/credentials/refresh
//! - Tool serialization to vendor wire (anthropic vs openai shape)
//!
//! What lives in the driver:
//! - Transcript ↔ `ConversationItem` boundary (`transcript_to_items`)
//! - Pre/post-turn compaction (`compact_conversation_with`)
//! - Background-task drain (`drain_completed_shell_tasks`)
//! - Tool execution (`execute_tool_call`)
//! - FunctionCallOutput synthesis from `ToolInvocation`
//! - Per-turn reflection observation
//! - End-of-turn hooks (`run_turn_hooks`)

use anyhow::Result;
use puffer_provider_registry::{AuthStore, ProviderDescriptor, ProviderRegistry};
use puffer_resources::LoadedResources;
use puffer_tools::ToolRegistry;

use super::openai::conversation::{
    compact_conversation_with, inject_post_compact_context, transcript_to_items, ConversationItem,
    ToolOutputPayload,
};
use super::reflection::{ReflectionConfig, ReflectionTraceEvent, ReflectionTracker};
use super::request_tool_filter::RequestToolFilter;
use super::tool_executor::{execute_tool_call, ToolExecutionBackend};
use super::{
    enforce_tool_result_budget, process_tool_result, run_turn_hooks, ToolCallRequest,
    ToolInvocation, TurnExecution, TurnStreamEvent, MAX_TOOL_RESULT_CHARS,
};
use crate::AppState;

/// Output of one provider round-trip. Tool execution and
/// `FunctionCallOutput` synthesis are the loop's job — sessions only
/// return pre-tool items and pending tool calls.
pub(crate) struct AssistantTurn {
    /// Items to append BEFORE tool execution: assistant Message,
    /// Reasoning items, FunctionCall items.
    pub pre_tool_items: Vec<ConversationItem>,
    /// Pending tool calls extracted from the response.
    pub tool_calls: Vec<ToolCallRequest>,
    /// Final assistant text (joined from text content blocks).
    pub assistant_text: String,
    /// Optional input-token usage hint for compaction sizing.
    pub input_tokens_hint: Option<usize>,
}

/// Provider-side session that captures vendor-specific setup and
/// performs a single LLM round-trip per call.
pub(crate) trait TurnSession {
    /// Sends one provider request with streaming events flowing through
    /// `on_event`. Returns synthesized response items + pending tool calls.
    ///
    /// `items` is `&mut` so the session can implement provider-side
    /// recovery (Anthropic's 413 / prompt_too_long path drops oldest
    /// items in place and retries before returning).
    fn one_turn_streaming(
        &mut self,
        state: &mut AppState,
        items: &mut Vec<ConversationItem>,
        on_event: &mut dyn FnMut(TurnStreamEvent),
    ) -> Result<AssistantTurn>;

    /// Non-streaming variant. Default impl forwards to
    /// `one_turn_streaming` with a no-op event sink. Providers that do
    /// genuine non-streaming HTTP (Anthropic blocking JSON) override
    /// this for transport-level differences (e.g. 413 recovery).
    fn one_turn_blocking(
        &mut self,
        state: &mut AppState,
        items: &mut Vec<ConversationItem>,
    ) -> Result<AssistantTurn> {
        let mut sink = |_: TurnStreamEvent| {};
        self.one_turn_streaming(state, items, &mut sink)
    }

    /// Provider-specific compaction summary generation.
    fn generate_summary(&self, old_context: &str, model_id: &str) -> Option<String>;

    /// Backend descriptor for `execute_tool_call`. Carries vendor refs
    /// (e.g. `&AnthropicRequestConfig`) borrowed from session state.
    fn tool_execution_backend(&self) -> ToolExecutionBackend<'_>;
}

/// Static-per-turn inputs the loop needs from the call site. Mutable
/// references stay short-lived inside `run_*_loop` to keep the borrow
/// checker happy.
pub(crate) struct LoopInputs<'a> {
    pub state: &'a mut AppState,
    pub resources: &'a LoadedResources,
    pub providers: &'a ProviderRegistry,
    pub provider: &'a ProviderDescriptor,
    pub model_id: &'a str,
    pub auth_store: &'a mut AuthStore,
    pub input: &'a str,
    pub reflection_config: Option<ReflectionConfig>,
    pub tool_filter: Option<&'a RequestToolFilter>,
    pub registry: &'a ToolRegistry,
}

/// Streaming turn loop. Drives the conversation until the model stops
/// requesting tool calls.
pub(crate) fn run_streaming_loop(
    inputs: &mut LoopInputs<'_>,
    session: &mut dyn TurnSession,
    on_event: &mut dyn FnMut(TurnStreamEvent),
) -> Result<TurnExecution> {
    let cwd = inputs.state.cwd.clone();

    let mut items = transcript_to_items(inputs.state, inputs.input);
    let mut invocations: Vec<ToolInvocation> = Vec::new();
    let mut reflection_traces: Vec<ReflectionTraceEvent> = Vec::new();
    let mut reflection = inputs
        .reflection_config
        .clone()
        .map(|config| ReflectionTracker::new(inputs.input, config));

    // Pre-turn compaction.
    {
        let summary_fn = |old: &str, mid: &str| session.generate_summary(old, mid);
        if compact_conversation_with(&mut items, inputs.provider, inputs.model_id, None, &summary_fn)
        {
            inject_post_compact_context(&mut items, &cwd);
        }
    }

    loop {
        // Drain completed background tasks and inject as user messages.
        let completed = crate::runtime::claude_tools::workflow::drain_completed_shell_tasks(
            &inputs.state.cwd,
            &inputs.state.session.id,
        );
        if !completed.is_empty() {
            let notice = format!(
                "<system-reminder>\n{}\nUse TaskOutput to retrieve the full output if needed.\n</system-reminder>",
                completed.join("\n")
            );
            items.push(ConversationItem::user_message(&notice));
        }

        // Provider single round-trip.
        let turn = session.one_turn_streaming(inputs.state, &mut items, on_event)?;

        // No tool calls → final assistant text, run hooks, return.
        if turn.tool_calls.is_empty() {
            run_turn_hooks(
                inputs.resources,
                &cwd,
                &turn.assistant_text,
                invocations.len(),
            );
            return Ok(TurnExecution {
                assistant_text: turn.assistant_text,
                tool_invocations: invocations,
                reflection_traces,
            });
        }

        // Append response items (assistant text + reasoning + FunctionCall) BEFORE running tools.
        items.extend(turn.pre_tool_items);

        on_event(TurnStreamEvent::ToolCallsRequested(turn.tool_calls.clone()));

        // Execute tools (sequential — parallel-safe batching is a follow-up).
        let new_invocations = execute_tool_batch(inputs, session, &cwd, &turn.tool_calls)?;

        if !new_invocations.is_empty() {
            on_event(TurnStreamEvent::ToolInvocations(new_invocations.clone()));
        }

        // Append FunctionCallOutput items.
        for inv in &new_invocations {
            items.push(ConversationItem::FunctionCallOutput {
                call_id: inv.call_id.clone(),
                output: if inv.success {
                    ToolOutputPayload::success(inv.output.clone())
                } else {
                    ToolOutputPayload::error(inv.output.clone())
                },
            });
        }

        invocations.extend(new_invocations.iter().cloned());

        // Reflection observation over THIS batch only.
        if let Some(observation) = reflection.as_mut().and_then(|tracker| {
            tracker.observe_batch_with_judge(
                &new_invocations,
                &items,
                inputs.state,
                inputs.resources,
                inputs.providers,
                inputs.auth_store,
            )
        }) {
            for trace_event in &observation.trace_events {
                on_event(TurnStreamEvent::ReflectionTrace(trace_event.clone()));
            }
            reflection_traces.extend(observation.trace_events);
            if let Some(checkpoint) = observation.checkpoint {
                on_event(TurnStreamEvent::ReflectionCheckpoint(
                    checkpoint.summary.clone(),
                ));
                items.push(ConversationItem::user_message(checkpoint.prompt));
            }
        }

        // Post-iteration compaction.
        {
            let summary_fn = |old: &str, mid: &str| session.generate_summary(old, mid);
            if compact_conversation_with(
                &mut items,
                inputs.provider,
                inputs.model_id,
                turn.input_tokens_hint,
                &summary_fn,
            ) {
                inject_post_compact_context(&mut items, &cwd);
            }
        }
    }
}

/// Non-streaming turn loop. Same shape as streaming but uses
/// `one_turn_blocking` so providers can route through their non-stream
/// transport (Anthropic blocking JSON, with 413 recovery).
pub(crate) fn run_blocking_loop(
    inputs: &mut LoopInputs<'_>,
    session: &mut dyn TurnSession,
) -> Result<TurnExecution> {
    let cwd = inputs.state.cwd.clone();

    let mut items = transcript_to_items(inputs.state, inputs.input);
    let mut invocations: Vec<ToolInvocation> = Vec::new();
    let mut reflection_traces: Vec<ReflectionTraceEvent> = Vec::new();
    let mut reflection = inputs
        .reflection_config
        .clone()
        .map(|config| ReflectionTracker::new(inputs.input, config));

    {
        let summary_fn = |old: &str, mid: &str| session.generate_summary(old, mid);
        if compact_conversation_with(&mut items, inputs.provider, inputs.model_id, None, &summary_fn)
        {
            inject_post_compact_context(&mut items, &cwd);
        }
    }

    loop {
        let turn = session.one_turn_blocking(inputs.state, &mut items)?;

        if turn.tool_calls.is_empty() {
            run_turn_hooks(
                inputs.resources,
                &cwd,
                &turn.assistant_text,
                invocations.len(),
            );
            return Ok(TurnExecution {
                assistant_text: turn.assistant_text,
                tool_invocations: invocations,
                reflection_traces,
            });
        }

        items.extend(turn.pre_tool_items);

        let new_invocations = execute_tool_batch(inputs, session, &cwd, &turn.tool_calls)?;

        for inv in &new_invocations {
            items.push(ConversationItem::FunctionCallOutput {
                call_id: inv.call_id.clone(),
                output: if inv.success {
                    ToolOutputPayload::success(inv.output.clone())
                } else {
                    ToolOutputPayload::error(inv.output.clone())
                },
            });
        }

        invocations.extend(new_invocations.iter().cloned());

        if let Some(observation) = reflection.as_mut().and_then(|tracker| {
            tracker.observe_batch_with_judge(
                &new_invocations,
                &items,
                inputs.state,
                inputs.resources,
                inputs.providers,
                inputs.auth_store,
            )
        }) {
            reflection_traces.extend(observation.trace_events);
            if let Some(checkpoint) = observation.checkpoint {
                items.push(ConversationItem::user_message(checkpoint.prompt));
            }
        }

        {
            let summary_fn = |old: &str, mid: &str| session.generate_summary(old, mid);
            if compact_conversation_with(
                &mut items,
                inputs.provider,
                inputs.model_id,
                turn.input_tokens_hint,
                &summary_fn,
            ) {
                inject_post_compact_context(&mut items, &cwd);
            }
        }
    }
}

/// Executes one batch of tool calls produced by a single assistant turn.
///
/// Mirrors the existing serial behavior of `execute_anthropic_tool_calls`
/// (head-truncation per tool, aggregate budget). Parallel-safe batching
/// is a follow-up.
fn execute_tool_batch(
    inputs: &mut LoopInputs<'_>,
    session: &mut dyn TurnSession,
    cwd: &std::path::Path,
    tool_calls: &[ToolCallRequest],
) -> Result<Vec<ToolInvocation>> {
    let mut invocations = Vec::with_capacity(tool_calls.len());

    for call in tool_calls {
        let backend = session.tool_execution_backend();
        let input_value: serde_json::Value =
            serde_json::from_str(&call.input).unwrap_or(serde_json::Value::Null);
        let execution = execute_tool_call(
            inputs.state,
            inputs.resources,
            inputs.providers,
            inputs.auth_store,
            inputs.registry,
            inputs.model_id,
            cwd,
            backend,
            inputs.tool_filter,
            &call.tool_id,
            input_value,
        )?;
        let raw_output = if execution.output.stderr.is_empty() {
            execution.output.stdout
        } else if execution.output.stdout.is_empty() {
            execution.output.stderr
        } else {
            format!("{}\n{}", execution.output.stdout, execution.output.stderr)
        };
        let output_text = process_tool_result(
            &raw_output,
            MAX_TOOL_RESULT_CHARS,
            &inputs.state.session.id,
        );
        invocations.push(ToolInvocation {
            call_id: call.call_id.clone(),
            tool_id: call.tool_id.clone(),
            input: call.input.clone(),
            output: output_text,
            success: execution.success,
        });
    }

    // Per-message aggregate budget (CC: 200K).
    let mut output_strings: Vec<String> = invocations.iter().map(|i| i.output.clone()).collect();
    enforce_tool_result_budget(&mut output_strings, &inputs.state.session.id);
    for (i, new_output) in output_strings.into_iter().enumerate() {
        if new_output != invocations[i].output {
            invocations[i].output = new_output;
        }
    }

    Ok(invocations)
}
