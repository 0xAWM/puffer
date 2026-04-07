use super::StructuredOutputConfig;
use crate::AppState;
use puffer_provider_openai::{OpenAIResponsesTextConfig, OpenAIResponsesTool};
use puffer_provider_registry::{OAuthCredential, ProviderDescriptor};
use serde_json::{json, Value};

pub(super) const OPENAI_STRUCTURED_OUTPUT_FAMILY: &str = "openai";

pub(crate) fn build_codex_openai_request_body(
    state: &AppState,
    model_id: &str,
    input: Value,
    tools: &[OpenAIResponsesTool],
    supports_reasoning: bool,
    text: Option<OpenAIResponsesTextConfig>,
) -> Value {
    let reasoning = codex_reasoning_config(state, supports_reasoning);
    let include = if reasoning.is_some() {
        vec![json!("reasoning.encrypted_content")]
    } else {
        Vec::new()
    };
    let store = std::env::var("PUFFER_OPENAI_STORE_RESPONSES")
        .ok()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
    let mut body = json!({
        "model": model_id,
        "instructions": "",
        "input": codex_input_items(input),
        "tools": tools,
        "tool_choice": "auto",
        "parallel_tool_calls": !tools.is_empty(),
        "store": store,
        "stream": true,
        "include": include,
        "prompt_cache_key": state.session.id.to_string(),
    });
    if let Some(reasoning) = reasoning {
        body["reasoning"] = reasoning;
    }
    if let Some(text) = text {
        body["text"] = serde_json::to_value(text).unwrap_or(Value::Null);
    }
    body
}

pub(super) fn prefer_native_structured_output(
    state: &AppState,
    provider: &ProviderDescriptor,
    model_id: &str,
    structured_output: Option<&StructuredOutputConfig>,
) -> bool {
    structured_output.is_some()
        && !state.is_native_structured_output_unsupported(
            OPENAI_STRUCTURED_OUTPUT_FAMILY,
            provider.id.as_str(),
            model_id,
            provider.base_url.as_str(),
        )
}

pub(super) fn structured_output_endpoint_id(provider: &ProviderDescriptor) -> &str {
    provider.base_url.as_str()
}

pub(super) fn is_openai_structured_output_error(error: &anyhow::Error) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    [
        "response_format",
        "text.format",
        "\"text\"",
        "json_schema",
        "json schema",
        "structured output",
        "structured_output",
        "\"strict\"",
    ]
    .iter()
    .any(|pattern| text.contains(pattern))
}

pub(super) fn openai_registry_credential(
    credential: puffer_provider_openai::OpenAIOAuthCredentials,
) -> OAuthCredential {
    OAuthCredential {
        access_token: credential.access_token,
        refresh_token: credential.refresh_token,
        expires_at_ms: credential.expires_at_ms,
        account_id: credential.account_id,
        organization_id: None,
        email: credential.email,
        plan_type: credential.plan_type,
        rate_limit_tier: None,
        scopes: Vec::new(),
        organization_name: None,
        organization_role: None,
        workspace_role: None,
    }
}

pub(super) fn extend_input_with_continuation(input: Value, continuation: Value) -> Value {
    let mut items = openai_input_items(input);
    items.extend(openai_input_items(continuation));
    Value::Array(items)
}

fn codex_input_items(input: Value) -> Value {
    match input {
        Value::Array(_) => input,
        Value::String(text) => json!([
            {
                "type": "message",
                "role": "user",
                "content": [
                    {
                        "type": "input_text",
                        "text": text,
                    }
                ],
            }
        ]),
        other => other,
    }
}

fn openai_input_items(input: Value) -> Vec<Value> {
    match input {
        Value::Array(items) => items,
        Value::String(text) => vec![json!({
            "type": "message",
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": text,
                }
            ],
        })],
        Value::Null => Vec::new(),
        other => vec![other],
    }
}

fn codex_reasoning_config(state: &AppState, supports_reasoning: bool) -> Option<Value> {
    if !supports_reasoning {
        return None;
    }
    let mut reasoning = json!({ "summary": "auto" });
    match state.effort_level.as_str() {
        "low" | "medium" | "high" => {
            reasoning["effort"] = json!(state.effort_level);
        }
        "max" => {
            reasoning["effort"] = json!("high");
        }
        _ => {}
    }
    Some(reasoning)
}
