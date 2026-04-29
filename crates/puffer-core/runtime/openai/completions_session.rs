//! [`TurnSession`] impl for the OpenAI Chat Completions API.
//!
//! Simpler than the Responses session: no `previous_response_id`
//! threading, no SSE streaming, no reasoning items, no usage event.
//! `one_turn_blocking` does the work; the streaming variant falls back
//! to the trait default.

use anyhow::Result;
use puffer_provider_openai::{
    build_chat_completions_request, extract_chat_completions_text,
    extract_chat_completions_tool_calls, parse_chat_completions_response,
    OpenAIChatCompletionsRequest, OpenAIChatCompletionTool, OpenAIChatResponseFormat,
    OpenAIRequestConfig, OpenAIResponsesToolChoiceMode,
};
use puffer_provider_registry::{AuthStore, ProviderDescriptor};
use puffer_resources::LoadedResources;
use puffer_tools::ToolRegistry;
use serde_json::Value;
use std::collections::HashSet;

use super::conversation::{
    build_system_reminder, items_to_chat_messages, ConversationItem,
};
use super::{
    parse_openai_text, parse_openai_text_fallback, send_openai_request_with_refresh,
    OpenAIExecutionConfig,
};
use crate::permissions::load_runtime_permission_context;
use crate::runtime::agent_loop::{AssistantTurn, TurnSession};
use crate::runtime::structured_output_support::{
    openai_chat_completion_tools_for_request, openai_chat_response_format, StructuredOutputConfig,
};
use crate::runtime::system_prompt::render_runtime_system_prompt;
use crate::runtime::tool_executor::ToolExecutionBackend;
use crate::runtime::{ToolCallRequest, TurnRequestOptions, TurnStreamEvent};
use crate::AppState;

pub(super) struct OpenAICompletionsTurnSession {
    pub execution: OpenAIExecutionConfig,
    pub tools: Vec<OpenAIChatCompletionTool>,
    pub response_format: Option<OpenAIChatResponseFormat>,
    pub system_prompt: String,
    pub plan_mode_context: Option<String>,
    pub system_reminder: String,
    pub structured_output: Option<StructuredOutputConfig>,
    pub model_id: String,
}

impl TurnSession for OpenAICompletionsTurnSession {
    fn one_turn_streaming(
        &mut self,
        state: &mut AppState,
        auth_store: &mut AuthStore,
        items: &mut Vec<ConversationItem>,
        _on_event: &mut dyn FnMut(TurnStreamEvent),
    ) -> Result<AssistantTurn> {
        // Chat Completions has no live SSE in this codebase; the
        // streaming variant falls through to a single blocking call so
        // tool execution semantics match the legacy path.
        self.one_turn_blocking(state, auth_store, items)
    }

    fn one_turn_blocking(
        &mut self,
        state: &mut AppState,
        auth_store: &mut AuthStore,
        items: &mut Vec<ConversationItem>,
    ) -> Result<AssistantTurn> {
        let _ = items.len(); // items moved into messages below; ensure no aliasing

        let messages = items_to_chat_messages(
            items,
            Some(&self.system_prompt),
            self.plan_mode_context.as_deref(),
            Some(&self.system_reminder),
        );

        let model_id = self.model_id.clone();
        let tools = self.tools.clone();
        let response_format = self.response_format.clone();

        let body_for_each_attempt = move |request_config: &OpenAIRequestConfig| {
            build_chat_completions_request(
                request_config,
                &OpenAIChatCompletionsRequest {
                    model: model_id.clone(),
                    messages: messages.clone(),
                    tools: tools.clone(),
                    tool_choice: if tools.is_empty() {
                        None
                    } else {
                        Some(OpenAIResponsesToolChoiceMode::Auto)
                    },
                    response_format: response_format.clone(),
                },
            )
        };

        let response: Value =
            send_openai_request_with_refresh(auth_store, &mut self.execution, body_for_each_attempt)?;

        let parsed = parse_chat_completions_response(&serde_json::to_string(&response)?)?;
        let tool_calls_vendor = extract_chat_completions_tool_calls(&parsed)?;
        let tool_calls: Vec<ToolCallRequest> = tool_calls_vendor
            .iter()
            .map(|tc| ToolCallRequest {
                call_id: tc.call_id.clone(),
                tool_id: tc.name.clone(),
                input: serde_json::to_string(&tc.arguments).unwrap_or_default(),
            })
            .collect();

        let assistant_text_from_msg = extract_chat_completions_text(&parsed);

        let mut pre_tool_items: Vec<ConversationItem> = Vec::new();
        if !assistant_text_from_msg.trim().is_empty() {
            pre_tool_items.push(ConversationItem::assistant_message(&assistant_text_from_msg));
        }
        for tc in &tool_calls_vendor {
            pre_tool_items.push(ConversationItem::FunctionCall {
                call_id: tc.call_id.clone(),
                name: tc.name.clone(),
                arguments: serde_json::to_string(&tc.arguments).unwrap_or_default(),
            });
        }

        let final_assistant_text = if tool_calls.is_empty() {
            if assistant_text_from_msg.trim().is_empty() {
                parse_openai_text(&response)
                    .or_else(|_| parse_openai_text_fallback(&response, state))
                    .unwrap_or_default()
            } else {
                assistant_text_from_msg
            }
        } else {
            String::new()
        };

        Ok(AssistantTurn {
            pre_tool_items,
            tool_calls,
            assistant_text: final_assistant_text,
            input_tokens_hint: None,
            emitted_tool_call_ids: HashSet::new(),
        })
    }

    fn generate_summary(&self, _old_context: &str, _model_id: &str) -> Option<String> {
        // Same rationale as Responses: compaction falls through to
        // Phase 3 (drop oldest items) when no summary is provided. Wire
        // up to the OpenAI summary helper in a follow-up.
        None
    }

    fn tool_execution_backend(&self) -> ToolExecutionBackend<'_> {
        ToolExecutionBackend::OpenAi {
            request_config: &self.execution.request_config,
            structured_output: self.structured_output.as_ref(),
        }
    }
}

pub(super) fn setup_completions_session(
    state: &mut AppState,
    resources: &LoadedResources,
    provider: &ProviderDescriptor,
    model_id: String,
    auth_store: &mut AuthStore,
    options: &TurnRequestOptions<'_>,
    use_native: bool,
) -> Result<OpenAICompletionsTurnSession> {
    let execution = super::resolve_openai_execution_config(state, auth_store, provider)?;
    let registry = ToolRegistry::from_resources(resources);
    let permission_context = load_runtime_permission_context(&state.cwd, resources, state)?;
    let response_format = openai_chat_response_format(options.structured_output, use_native);
    let tools = openai_chat_completion_tools_for_request(
        &registry,
        options.structured_output,
        use_native,
        Some(&permission_context),
        options.tool_filter,
    )?;
    let system_prompt = render_runtime_system_prompt(
        state,
        resources,
        &model_id,
        &tools
            .iter()
            .map(|tool| tool.function.name.clone())
            .collect::<std::collections::BTreeSet<_>>(),
    )?;
    let plan_mode_context = crate::plan_mode::take_plan_mode_context_message(state, resources)?;
    let system_reminder = build_system_reminder(&crate::runtime::git_status_context());

    Ok(OpenAICompletionsTurnSession {
        execution,
        tools,
        response_format,
        system_prompt,
        plan_mode_context,
        system_reminder,
        structured_output: options.structured_output.cloned(),
        model_id,
    })
}
