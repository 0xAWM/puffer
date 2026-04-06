use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A minimal Mistral chat response payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MistralChatResponse {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<MistralChatChoice>,
}

/// One completion choice returned by Mistral.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MistralChatChoice {
    #[serde(default)]
    pub index: Option<u32>,
    #[serde(default)]
    pub finish_reason: Option<String>,
    pub message: MistralChatChoiceMessage,
}

/// An assistant message returned by Mistral.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MistralChatChoiceMessage {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(default)]
    pub tool_calls: Vec<crate::MistralToolCall>,
}

/// A parsed tool call emitted by Mistral.
#[derive(Debug, Clone, PartialEq)]
pub struct MistralParsedToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
}

pub(crate) fn parse_chat_response(payload: &str) -> Result<MistralChatResponse> {
    serde_json::from_str(payload).context("failed to parse Mistral chat payload")
}

pub(crate) fn extract_chat_text(response: &MistralChatResponse) -> String {
    response
        .choices
        .first()
        .and_then(|choice| choice.message.content.as_ref())
        .map(extract_content_text)
        .unwrap_or_default()
}

pub(crate) fn extract_chat_tool_calls(
    response: &MistralChatResponse,
) -> Result<Vec<MistralParsedToolCall>> {
    response
        .choices
        .first()
        .into_iter()
        .flat_map(|choice| choice.message.tool_calls.iter())
        .map(|tool_call| {
            let arguments = serde_json::from_str(&tool_call.function.arguments).with_context(|| {
                format!(
                    "failed to parse Mistral tool arguments for call {}",
                    tool_call.id
                )
            })?;
            Ok(MistralParsedToolCall {
                call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                arguments,
            })
        })
        .collect()
}

fn extract_content_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.get("text")
                .and_then(Value::as_str)
                .or_else(|| item.get("content").and_then(Value::as_str))
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_from_string_content() {
        let response = parse_chat_response(
            r#"{
                "id": "cmpl_123",
                "choices": [
                    {
                        "message": {
                            "role": "assistant",
                            "content": "hello from mistral"
                        }
                    }
                ]
            }"#,
        )
        .expect("response");
        assert_eq!(extract_chat_text(&response), "hello from mistral");
    }

    #[test]
    fn extracts_text_and_tool_calls_from_array_content() {
        let response = parse_chat_response(
            r#"{
                "choices": [
                    {
                        "message": {
                            "role": "assistant",
                            "content": [
                                { "type": "text", "text": "Inspecting" }
                            ],
                            "tool_calls": [
                                {
                                    "id": "abc123xyz",
                                    "type": "function",
                                    "function": {
                                        "name": "read_file",
                                        "arguments": "{\"path\":\"Cargo.toml\"}"
                                    }
                                }
                            ]
                        }
                    }
                ]
            }"#,
        )
        .expect("response");
        assert_eq!(extract_chat_text(&response), "Inspecting");
        let calls = extract_chat_tool_calls(&response).expect("tool calls");
        assert_eq!(calls[0].call_id, "abc123xyz");
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].arguments, serde_json::json!({ "path": "Cargo.toml" }));
    }

    #[test]
    fn rejects_invalid_tool_argument_json() {
        let response = parse_chat_response(
            r#"{
                "choices": [
                    {
                        "message": {
                            "tool_calls": [
                                {
                                    "id": "abc123xyz",
                                    "type": "function",
                                    "function": {
                                        "name": "read_file",
                                        "arguments": "{not-json}"
                                    }
                                }
                            ]
                        }
                    }
                ]
            }"#,
        )
        .expect("response");
        let error = extract_chat_tool_calls(&response).expect_err("invalid json");
        assert!(error
            .to_string()
            .contains("failed to parse Mistral tool arguments"));
    }
}
