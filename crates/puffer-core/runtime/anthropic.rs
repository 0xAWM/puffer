//! Anthropic Messages API adapter — request building, SSE parsing,
//! tool-call execution, and post-turn `ConversationItem` synthesis.
//!
//! Mirrors the shape of `runtime/openai.rs` so that swapping or comparing
//! providers stays structurally parallel. Vendor-specific types
//! (`AnthropicMessage`, `AnthropicRequestConfig`, etc.) are confined to
//! this module — `runtime.rs` should not import from
//! `puffer_transport_anthropic` directly.

use super::anthropic_sse::parse_anthropic_sse;
use super::openai::conversation::{
    build_system_reminder, compact_conversation_with, inject_post_compact_context,
    items_to_anthropic_messages, transcript_to_items, ConversationItem, ToolOutputPayload,
};
use super::reflection::{self, ReflectionTraceEvent};
use super::request_tool_filter::RequestToolFilter;
use super::structured_output_support::{
    anthropic_tool_definitions_for_request, StructuredOutputConfig,
};
use super::system_prompt::render_runtime_system_prompt;
use super::tool_executor::{execute_tool_call, ToolExecutionBackend};
use super::{
    enforce_tool_result_budget, git_status_context, process_tool_result, resolve_max_output_tokens,
    run_turn_hooks, send_http_request, ToolInvocation, TurnExecution, TurnRequestOptions,
    TurnStreamEvent, APP_VERSION, MAX_TOOL_RESULT_CHARS,
};
use crate::permissions::load_runtime_permission_context;
use crate::AppState;
use anyhow::{anyhow, bail, Context, Result};
use puffer_provider_registry::{
    AuthStore, OAuthCredential, ProviderDescriptor, ProviderRegistry, StoredCredential,
};
use puffer_resources::LoadedResources;
use puffer_tools::ToolRegistry;
use puffer_transport_anthropic::{
    build_messages_request, get_session_ingress_auth, AnthropicAuth, AnthropicMessage,
    AnthropicModelRequest, AnthropicRequestConfig,
};
use reqwest::blocking::Client;
use serde_json::{json, Value};

pub(super) fn execute_anthropic(
    state: &mut AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    provider: &ProviderDescriptor,
    model_id: String,
    auth_store: &mut AuthStore,
    input: &str,
    options: TurnRequestOptions<'_>,
) -> Result<TurnExecution> {
    let structured_output = options.structured_output;
    let auth = anthropic_auth_for_provider(auth_store, provider)?;
    let registry = ToolRegistry::from_resources(resources);
    let permission_context = load_runtime_permission_context(&state.cwd, resources, state)?;
    let plan_mode_context = crate::plan_mode::take_plan_mode_context_message(state, resources)?;

    // Build canonical conversation items (shared with OpenAI path).
    let mut items = transcript_to_items(state, input);
    let mut invocations = Vec::new();
    let mut reflection_traces: Vec<ReflectionTraceEvent> = Vec::new();
    let mut reflection = options
        .reflection
        .map(|config| reflection::ReflectionTracker::new(input, config));

    let request_config = AnthropicRequestConfig {
        base_url: provider.base_url.clone(),
        session_id: state.session.id.to_string(),
        custom_headers: provider.headers.clone(),
        remote_container_id: None,
        remote_session_id: None,
        client_app: None,
        entrypoint: "cli".to_string(),
        user_type: "external".to_string(),
        version: APP_VERSION.to_string(),
        workload: None,
        additional_protection: false,
        cch_enabled: true,
        auth: auth.clone(),
        beta_header: None,
        client_request_id: None,
    };
    // Build request for URL, headers, and attribution prefix (fingerprint uses first user text).
    let request = build_messages_request(
        &request_config,
        &AnthropicModelRequest {
            model: model_id.clone(),
            max_tokens: resolve_max_output_tokens(provider, &model_id),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: input.to_string(),
            }],
        },
    )?;
    let tools = anthropic_tool_definitions_for_request(
        &registry,
        structured_output,
        Some(&permission_context),
        options.tool_filter,
    )?;
    let system_prompt = render_runtime_system_prompt(
        state,
        resources,
        &model_id,
        &tools
            .iter()
            .filter_map(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect::<std::collections::BTreeSet<_>>(),
    )?;

    // System reminder (currentDate + gitStatus) — now in system blocks, not prepended to messages.
    let git_status = git_status_context();
    let system_reminder = build_system_reminder(&git_status);

    // Anthropic summary function: uses Anthropic Messages API.
    let summary_url = request.url.clone();
    let summary_headers = request.headers.clone();
    let anthropic_summary_fn = |old_context: &str, mid: &str| -> Option<String> {
        anthropic_generate_summary(old_context, mid, &summary_url, &summary_headers)
    };

    // Pre-turn compaction using shared logic.
    let cwd = state.cwd.clone();
    let compacted =
        compact_conversation_with(&mut items, provider, &model_id, None, &anthropic_summary_fn);
    if compacted {
        inject_post_compact_context(&mut items, &cwd);
    }

    // Resolve thinking/reasoning support.
    let model_supports_thinking = provider
        .models
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| m.supports_reasoning)
        .unwrap_or(false);
    let max_output = resolve_max_output_tokens(provider, &model_id);

    loop {
        // Convert items to Anthropic wire format at each iteration.
        let wire_messages = items_to_anthropic_messages(&items);

        let mut body = json!({
            "model": model_id,
            "max_tokens": max_output,
            "messages": wire_messages,
            "system": anthropic_system_blocks(
                &request.attribution_prefix_block,
                Some(system_prompt.as_str()),
                plan_mode_context.as_deref(),
                Some(&system_reminder),
            )
        });
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools.clone());
            body["tool_choice"] = json!({"type": "auto"});
        }
        let provider_supports_thinking_api =
            provider.id == "anthropic" || provider.base_url.contains("anthropic.com");
        if model_supports_thinking && provider_supports_thinking_api && state.effort_level != "low"
        {
            let thinking_budget = match state.effort_level.as_str() {
                "high" | "max" => max_output.saturating_sub(1).min(16_384),
                _ => max_output.saturating_sub(1).min(8_192),
            };
            body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": thinking_budget
            });
        } else {
            body["temperature"] = json!(1);
        }
        if model_supports_thinking && provider_supports_thinking_api {
            body["context_management"] = json!({
                "edits": [{
                    "type": "clear_thinking_20251015",
                    "keep": "all"
                }]
            });
        }
        if state.fast_mode {
            body["speed"] = json!("fast");
        }
        body["metadata"] = json!({
            "user_id": format!(
                "{{\"session_id\":\"{}\",\"device_id\":\"puffer-cli\"}}",
                state.session.id
            )
        });

        let response =
            match send_http_request(&request.url, &request.headers, &body.to_string(), true) {
                Ok(response) => response,
                Err(error) => {
                    let err_msg = error.to_string();
                    // 413 / prompt_too_long recovery: drop oldest items and retry.
                    if (err_msg.contains("413")
                        || err_msg.contains("prompt_too_long")
                        || err_msg.contains("too long"))
                        && items.len() > 3
                    {
                        let drop_count = (items.len() / 3).max(1);
                        items.drain(..drop_count);
                        // Ensure items start with a Message.
                        if !matches!(items.first(), Some(ConversationItem::Message { .. })) {
                            items.insert(
                                0,
                                ConversationItem::user_message(
                                    "[Context truncated to fit within model limits]",
                                ),
                            );
                        }
                        continue;
                    }
                    return Err(error);
                }
            };

        if let Some(tool_results) = execute_anthropic_tool_calls(
            state,
            resources,
            providers,
            auth_store,
            &response,
            &registry,
            &cwd,
            &request_config,
            &model_id,
            structured_output,
            options.tool_filter,
        )? {
            invocations.extend(tool_results.invocations.clone());
            // Append response content as ConversationItems.
            append_anthropic_response_to_items(&mut items, &response, &tool_results);
            if let Some(observation) = reflection.as_mut().and_then(|tracker| {
                tracker.observe_batch_with_judge(
                    &tool_results.invocations,
                    &items,
                    state,
                    resources,
                    providers,
                    auth_store,
                )
            }) {
                reflection_traces.extend(observation.trace_events);
                if let Some(checkpoint) = observation.checkpoint {
                    items.push(ConversationItem::user_message(checkpoint.prompt));
                }
            }
            // Compact between tool iterations using shared logic.
            let compacted = compact_conversation_with(
                &mut items,
                provider,
                &model_id,
                None,
                &anthropic_summary_fn,
            );
            if compacted {
                inject_post_compact_context(&mut items, &cwd);
            }
            continue;
        }

        let assistant_text = parse_anthropic_text(&response)?;
        run_turn_hooks(resources, &state.cwd, &assistant_text, invocations.len());
        return Ok(TurnExecution {
            assistant_text,
            tool_invocations: invocations,
            reflection_traces,
        });
    }
}

/// Streaming variant of execute_anthropic — sends `stream: true` and parses
/// SSE events, emitting TextDelta in real-time.
pub(super) fn execute_anthropic_streaming<F>(
    state: &mut AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    provider: &ProviderDescriptor,
    model_id: String,
    auth_store: &mut AuthStore,
    input: &str,
    options: TurnRequestOptions<'_>,
    on_event: &mut F,
) -> Result<TurnExecution>
where
    F: FnMut(TurnStreamEvent),
{
    let structured_output = options.structured_output;
    let auth = anthropic_auth_for_provider(auth_store, provider)?;
    let registry = ToolRegistry::from_resources(resources);
    let permission_context = load_runtime_permission_context(&state.cwd, resources, state)?;
    let plan_mode_context = crate::plan_mode::take_plan_mode_context_message(state, resources)?;

    // Build canonical conversation items (shared with OpenAI path).
    let mut items = transcript_to_items(state, input);
    let mut invocations = Vec::new();
    let mut reflection_traces: Vec<ReflectionTraceEvent> = Vec::new();
    let mut reflection = options
        .reflection
        .map(|config| reflection::ReflectionTracker::new(input, config));

    let request_config = build_anthropic_request_config(state, provider, &auth);
    let request = build_messages_request(
        &request_config,
        &AnthropicModelRequest {
            model: model_id.clone(),
            max_tokens: resolve_max_output_tokens(provider, &model_id),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: input.to_string(),
            }],
        },
    )?;
    let tools = anthropic_tool_definitions_for_request(
        &registry,
        structured_output,
        Some(&permission_context),
        options.tool_filter,
    )?;
    let system_prompt = render_runtime_system_prompt(
        state,
        resources,
        &model_id,
        &tools
            .iter()
            .filter_map(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect::<std::collections::BTreeSet<_>>(),
    )?;

    // System reminder (currentDate + gitStatus) — now in system blocks.
    let git_status = git_status_context();
    let system_reminder = build_system_reminder(&git_status);

    // Anthropic summary function.
    let summary_url = request.url.clone();
    let summary_headers = request.headers.clone();
    let anthropic_summary_fn = |old_context: &str, mid: &str| -> Option<String> {
        anthropic_generate_summary(old_context, mid, &summary_url, &summary_headers)
    };

    // Pre-turn compaction.
    let cwd = state.cwd.clone();
    let compacted =
        compact_conversation_with(&mut items, provider, &model_id, None, &anthropic_summary_fn);
    if compacted {
        inject_post_compact_context(&mut items, &cwd);
    }

    let model_supports_thinking = provider
        .models
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| m.supports_reasoning)
        .unwrap_or(false);
    let provider_supports_thinking_api =
        provider.id == "anthropic" || provider.base_url.contains("anthropic.com");
    let max_output = resolve_max_output_tokens(provider, &model_id);

    loop {
        // Drain completed background tasks and inject as user messages.
        let completed = crate::runtime::claude_tools::workflow::drain_completed_shell_tasks(
            &state.cwd,
            &state.session.id,
        );
        if !completed.is_empty() {
            let notice = format!(
                "<system-reminder>\n{}\nUse TaskOutput to retrieve the full output if needed.\n</system-reminder>",
                completed.join("\n")
            );
            items.push(ConversationItem::user_message(&notice));
        }

        // Convert items to Anthropic wire format.
        let wire_messages = items_to_anthropic_messages(&items);

        let mut body = json!({
            "model": model_id,
            "max_tokens": max_output,
            "messages": wire_messages,
            "stream": true,
            "system": anthropic_system_blocks(
                &request.attribution_prefix_block,
                Some(system_prompt.as_str()),
                plan_mode_context.as_deref(),
                Some(&system_reminder),
            )
        });
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools.clone());
            body["tool_choice"] = json!({"type": "auto"});
        }
        if model_supports_thinking && provider_supports_thinking_api && state.effort_level != "low"
        {
            let thinking_budget = match state.effort_level.as_str() {
                "high" | "max" => max_output.saturating_sub(1).min(16_384),
                _ => max_output.saturating_sub(1).min(8_192),
            };
            body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": thinking_budget
            });
        } else {
            body["temperature"] = json!(1);
        }

        // Send streaming request.
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| Client::new());
        let mut http_request = client.post(&request.url);
        for (key, value) in &request.headers {
            http_request = http_request.header(key, value);
        }
        http_request = http_request
            .header("content-type", "application/json")
            .header("accept", "text/event-stream");
        let http_response = http_request
            .body(body.to_string())
            .send()
            .with_context(|| format!("failed to send streaming request to {}", request.url))?;

        if !http_response.status().is_success() {
            let status = http_response.status();
            let text = http_response.text().unwrap_or_default();
            bail!("request failed with status {status}: {text}");
        }

        let response = parse_anthropic_sse(http_response, on_event)?;

        if let Some(tool_results) = execute_anthropic_tool_calls(
            state,
            resources,
            providers,
            auth_store,
            &response,
            &registry,
            &cwd,
            &request_config,
            &model_id,
            structured_output,
            options.tool_filter,
        )? {
            if !tool_results.invocations.is_empty() {
                on_event(TurnStreamEvent::ToolInvocations(
                    tool_results.invocations.clone(),
                ));
            }
            invocations.extend(tool_results.invocations.clone());
            // Append response content as ConversationItems.
            append_anthropic_response_to_items(&mut items, &response, &tool_results);
            if let Some(observation) = reflection.as_mut().and_then(|tracker| {
                tracker.observe_batch_with_judge(
                    &tool_results.invocations,
                    &items,
                    state,
                    resources,
                    providers,
                    auth_store,
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
            // Compact between tool iterations.
            let compacted = compact_conversation_with(
                &mut items,
                provider,
                &model_id,
                None,
                &anthropic_summary_fn,
            );
            if compacted {
                inject_post_compact_context(&mut items, &cwd);
            }
            continue;
        }

        let assistant_text = parse_anthropic_text(&response)?;
        run_turn_hooks(resources, &state.cwd, &assistant_text, invocations.len());
        return Ok(TurnExecution {
            assistant_text,
            tool_invocations: invocations,
            reflection_traces,
        });
    }
}

fn build_anthropic_request_config(
    state: &AppState,
    provider: &ProviderDescriptor,
    auth: &AnthropicAuth,
) -> AnthropicRequestConfig {
    AnthropicRequestConfig {
        base_url: provider.base_url.clone(),
        session_id: state.session.id.to_string(),
        custom_headers: provider.headers.clone(),
        remote_container_id: None,
        remote_session_id: None,
        client_app: None,
        entrypoint: "cli".to_string(),
        user_type: "external".to_string(),
        version: APP_VERSION.to_string(),
        workload: None,
        additional_protection: false,
        cch_enabled: true,
        auth: auth.clone(),
        beta_header: None,
        client_request_id: None,
    }
}

fn anthropic_auth_for_provider(
    auth_store: &AuthStore,
    provider: &ProviderDescriptor,
) -> Result<AnthropicAuth> {
    match auth_store.get(&provider.id) {
        Some(StoredCredential::ApiKey { key }) => Ok(AnthropicAuth::ApiKey(key.clone())),
        Some(StoredCredential::OAuth(OAuthCredential { access_token, .. })) => {
            Ok(AnthropicAuth::OAuthBearer(access_token.clone()))
        }
        None if provider.auth_modes.is_empty() => Ok(AnthropicAuth::None),
        None => get_session_ingress_auth().ok_or_else(|| {
            anyhow!(
                "no credentials configured for provider {}; use `puffer auth set-api-key {}` first",
                provider.id,
                provider.id
            )
        }),
    }
}

fn parse_anthropic_text(response: &Value) -> Result<String> {
    let parts = response
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("anthropic response missing content array"))?
        .iter()
        .filter_map(|item| {
            let item_type = item.get("type").and_then(Value::as_str)?;
            if item_type == "text" {
                item.get("text").and_then(Value::as_str).map(str::to_string)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        bail!("anthropic response did not contain text content");
    }
    Ok(parts.join("\n"))
}

#[cfg(test)]
pub(super) fn anthropic_tool_schema(handler: &str) -> Value {
    match handler {
        "bash" => json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" }
            },
            "required": ["command"],
        }),
        "read_file" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"],
        }),
        "write_file" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "contents": { "type": "string" }
            },
            "required": ["path", "contents"],
        }),
        "replace_in_file" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "old": { "type": "string" },
                "new": { "type": "string" },
                "replace_all": { "type": "boolean" }
            },
            "required": ["path", "old", "new"],
        }),
        "list_dir" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": [],
        }),
        "search_text" => json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "path": { "type": "string" }
            },
            "required": ["query"],
        }),
        _ => json!({
            "type": "object",
            "properties": {},
        }),
    }
}

pub(super) fn execute_anthropic_tool_calls(
    state: &mut AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    auth_store: &mut AuthStore,
    response: &Value,
    registry: &ToolRegistry,
    cwd: &std::path::Path,
    request_config: &AnthropicRequestConfig,
    model_id: &str,
    structured_output: Option<&StructuredOutputConfig>,
    tool_filter: Option<&RequestToolFilter>,
) -> Result<Option<AnthropicToolResults>> {
    let Some(content) = response.get("content").and_then(Value::as_array) else {
        return Ok(None);
    };

    let mut results = Vec::new();
    let mut invocations = Vec::new();
    for item in content {
        if item.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let tool_id = item
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("anthropic tool_use block missing name"))?;
        let tool_use_id = item
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("anthropic tool_use block missing id"))?;
        let input = item
            .get("input")
            .ok_or_else(|| anyhow!("anthropic tool_use block missing input"))?;
        let execution = execute_tool_call(
            state,
            resources,
            providers,
            auth_store,
            registry,
            model_id,
            cwd,
            ToolExecutionBackend::Anthropic {
                request_config,
                structured_output,
            },
            tool_filter,
            tool_id,
            input.clone(),
        )?;
        let raw_output = if execution.output.stderr.is_empty() {
            execution.output.stdout
        } else if execution.output.stdout.is_empty() {
            execution.output.stderr
        } else {
            format!("{}\n{}", execution.output.stdout, execution.output.stderr)
        };
        // Persist oversized tool results to disk, returning a preview message.
        // Falls back to head truncation if disk write fails.
        // (CC limits: 50K chars per tool, 200K chars per message).
        let output_text =
            process_tool_result(&raw_output, MAX_TOOL_RESULT_CHARS, &state.session.id);
        results.push(json!({
            "type": "tool_result",
            "tool_use_id": tool_use_id,
            "content": output_text,
            "is_error": !execution.success,
        }));
        invocations.push(ToolInvocation {
            call_id: tool_use_id.to_string(),
            tool_id: tool_id.to_string(),
            input: serde_json::to_string(input)?,
            output: output_text.clone(),
            success: execution.success,
        });
    }

    if results.is_empty() {
        return Ok(None);
    }

    // Enforce per-message aggregate budget (CC: 200K).
    let mut output_strings: Vec<String> = invocations.iter().map(|i| i.output.clone()).collect();
    enforce_tool_result_budget(&mut output_strings, &state.session.id);
    // Apply budget changes back to results and invocations.
    for (i, new_output) in output_strings.into_iter().enumerate() {
        if new_output != invocations[i].output {
            results[i]["content"] = json!(new_output);
            invocations[i].output = new_output;
        }
    }

    Ok(Some(AnthropicToolResults {
        results: Value::Array(results),
        invocations,
    }))
}

pub(super) struct AnthropicToolResults {
    #[allow(dead_code)]
    pub(super) results: Value,
    pub(super) invocations: Vec<ToolInvocation>,
}

/// Converts an Anthropic API response + tool execution results into
/// ConversationItems and appends them to the items vector.
///
/// Extracts from the response:
/// - Text blocks → assistant message
/// - tool_use blocks → FunctionCall items
///
/// From tool_results.invocations:
/// - Each invocation → FunctionCallOutput item
fn append_anthropic_response_to_items(
    items: &mut Vec<ConversationItem>,
    response: &Value,
    tool_results: &AnthropicToolResults,
) {
    // Extract assistant text (non-tool-use content blocks) from response.
    if let Some(content) = response.get("content").and_then(Value::as_array) {
        let texts: Vec<&str> = content
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect();
        if !texts.is_empty() {
            items.push(ConversationItem::assistant_message(texts.join("\n")));
        }
    }

    // Extract tool_use blocks from response → FunctionCall items.
    if let Some(content) = response.get("content").and_then(Value::as_array) {
        for block in content {
            if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                continue;
            }
            let call_id = block
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let name = block
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let arguments = block
                .get("input")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "{}".to_string());
            items.push(ConversationItem::FunctionCall {
                call_id,
                name,
                arguments,
            });
        }
    }

    // Tool execution results → FunctionCallOutput items.
    for inv in &tool_results.invocations {
        items.push(ConversationItem::FunctionCallOutput {
            call_id: inv.call_id.clone(),
            output: if inv.success {
                ToolOutputPayload::success(inv.output.clone())
            } else {
                ToolOutputPayload::error(inv.output.clone())
            },
        });
    }
}

/// Generates an AI summary via the Anthropic Messages API.
/// Used as the summary function for `compact_conversation_with()`.
fn anthropic_generate_summary(
    old_context: &str,
    model_id: &str,
    url: &str,
    headers: &[(String, String)],
) -> Option<String> {
    let compact_prompt = format!(
        "Summarize this conversation fragment into a compact context block. \
         Preserve file paths, function names, errors, and key decisions verbatim. \
         Structure: 1) Intent 2) Key Concepts 3) Files & Code 4) Errors & Fixes \
         5) Pending Tasks 6) Current State. Be thorough but concise. \
         Do NOT use any tools.\n\n---\n\n{old_context}"
    );

    let body = json!({
        "model": model_id,
        "max_tokens": 16_384,
        "messages": [{"role": "user", "content": compact_prompt}],
    });

    match send_http_request(url, headers, &body.to_string(), true) {
        Ok(response) => parse_anthropic_text(&response).ok(),
        Err(_) => None,
    }
}

fn anthropic_system_blocks(
    attribution_prefix_block: &str,
    system_prompt: Option<&str>,
    plan_mode_context: Option<&str>,
    system_reminder: Option<&str>,
) -> Vec<Value> {
    let mut blocks = vec![json!({
        "type": "text",
        "text": attribution_prefix_block,
    })];
    if let Some(system_prompt) = system_prompt.filter(|prompt| !prompt.trim().is_empty()) {
        blocks.push(json!({
            "type": "text",
            "text": system_prompt,
            "cache_control": { "type": "ephemeral" }
        }));
    }
    if let Some(plan_mode_context) = plan_mode_context {
        blocks.push(json!({
            "type": "text",
            "text": plan_mode_context,
        }));
    }
    if let Some(reminder) = system_reminder.filter(|r| !r.trim().is_empty()) {
        blocks.push(json!({
            "type": "text",
            "text": format!(
                "<system-reminder>\nAs you answer the user's questions, you can use the following context:\n\
                 {reminder}\n\n\
                 IMPORTANT: this context may or may not be relevant to your tasks. \
                 You should not respond to this context unless it is highly relevant to your task.\n\
                 </system-reminder>"
            ),
        }));
    }
    blocks
}

/// Adapter wiring `execute_anthropic` and `execute_anthropic_streaming`
/// behind the neutral `ProviderAdapter` trait. Owns no state — all
/// per-turn context flows through method args.
pub(crate) struct AnthropicAdapter;

impl super::provider_adapter::ProviderAdapter for AnthropicAdapter {
    fn api_id(&self) -> &'static str {
        "anthropic-messages"
    }

    fn execute_turn(
        &self,
        state: &mut AppState,
        resources: &LoadedResources,
        providers: &ProviderRegistry,
        provider: &ProviderDescriptor,
        model_id: String,
        auth_store: &mut AuthStore,
        input: &str,
        options: TurnRequestOptions<'_>,
    ) -> Result<TurnExecution> {
        execute_anthropic(
            state, resources, providers, provider, model_id, auth_store, input, options,
        )
    }

    fn execute_turn_streaming(
        &self,
        state: &mut AppState,
        resources: &LoadedResources,
        providers: &ProviderRegistry,
        provider: &ProviderDescriptor,
        model_id: String,
        auth_store: &mut AuthStore,
        input: &str,
        options: TurnRequestOptions<'_>,
        on_event: &mut dyn FnMut(TurnStreamEvent),
    ) -> Result<TurnExecution> {
        // Wrap the unsized `dyn FnMut` in a sized closure so we can pass
        // it to the generic `execute_anthropic_streaming<F: FnMut(...)>`
        // signature without changing every downstream callee.
        let mut wrapped = |event: TurnStreamEvent| on_event(event);
        execute_anthropic_streaming(
            state,
            resources,
            providers,
            provider,
            model_id,
            auth_store,
            input,
            options,
            &mut wrapped,
        )
    }
}
