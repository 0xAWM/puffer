use crate::auth::MistralAuth;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Runtime request configuration for the Mistral provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MistralRequestConfig {
    pub base_url: String,
    pub version: String,
    pub auth: MistralAuth,
    pub custom_headers: IndexMap<String, String>,
    pub session_id: Option<String>,
}

/// A chat request accepted by the Mistral conversations API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MistralChatRequest {
    pub model: String,
    pub messages: Vec<MistralChatMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<MistralTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<MistralToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: bool,
}

/// One chat message item sent to Mistral.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MistralChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<MistralToolCall>,
}

/// One tool-call item nested under an assistant message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MistralToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: MistralToolFunctionCall,
}

/// The function-call payload nested under a Mistral tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MistralToolFunctionCall {
    pub name: String,
    pub arguments: String,
}

/// A tool definition accepted by the Mistral conversations API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MistralTool {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: MistralToolFunctionDefinition,
}

/// A function definition nested under a Mistral tool payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MistralToolFunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub strict: bool,
}

/// A tool-choice directive accepted by Mistral.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum MistralToolChoice {
    Mode(MistralToolChoiceMode),
    Named(MistralToolFunction),
}

/// A function-selector tool choice for Mistral.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MistralToolFunction {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: MistralToolChoiceName,
}

/// A function name nested under a named tool choice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MistralToolChoiceName {
    pub name: String,
}

/// A simple tool-choice mode accepted by Mistral.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MistralToolChoiceMode {
    Auto,
    None,
    Any,
    Required,
}

/// An ordered HTTP request representation for tests and execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltMistralRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

pub(crate) fn build_chat_request(
    config: &MistralRequestConfig,
    request: &MistralChatRequest,
) -> anyhow::Result<BuiltMistralRequest> {
    let mut headers = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Accept".to_string(), "application/json".to_string()),
        (
            "User-Agent".to_string(),
            format!("puffer-code/{}", config.version),
        ),
    ];
    match &config.auth {
        MistralAuth::None => {}
        MistralAuth::ApiKey(key) => {
            headers.push(("Authorization".to_string(), format!("Bearer {key}")));
        }
    }
    for (key, value) in &config.custom_headers {
        headers.push((key.clone(), value.clone()));
    }
    if let Some(session_id) = &config.session_id {
        if !headers
            .iter()
            .any(|(key, _)| key.eq_ignore_ascii_case("x-affinity"))
        {
            headers.push(("x-affinity".to_string(), session_id.clone()));
        }
    }
    Ok(BuiltMistralRequest {
        method: "POST",
        url: format!(
            "{}{}",
            config.base_url.trim_end_matches('/'),
            normalized_path(&config.base_url, "/v1/chat/completions")
        ),
        headers,
        body: serde_json::to_string(request)?,
    })
}

fn normalized_path(base_url: &str, path: &str) -> String {
    if base_url.trim_end_matches('/').ends_with("/v1") && path.starts_with("/v1/") {
        path[3..].to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn api_key_request_uses_expected_headers_and_affinity() {
        let request = build_chat_request(
            &MistralRequestConfig {
                base_url: "https://api.mistral.ai".to_string(),
                version: "0.1.0".to_string(),
                auth: MistralAuth::ApiKey("mistral-key".to_string()),
                custom_headers: IndexMap::new(),
                session_id: Some("session-123".to_string()),
            },
            &MistralChatRequest {
                model: "devstral-medium-latest".to_string(),
                messages: vec![MistralChatMessage {
                    role: "user".to_string(),
                    content: Some(json!("hello")),
                    tool_call_id: None,
                    name: None,
                    tool_calls: Vec::new(),
                }],
                tools: Vec::new(),
                tool_choice: None,
                max_tokens: Some(512),
                stream: false,
            },
        )
        .expect("request");
        assert_eq!(request.url, "https://api.mistral.ai/v1/chat/completions");
        assert!(request
            .headers
            .iter()
            .any(|(key, value)| key == "Authorization" && value == "Bearer mistral-key"));
        assert!(request
            .headers
            .iter()
            .any(|(key, value)| key == "x-affinity" && value == "session-123"));
    }

    #[test]
    fn none_auth_omits_authorization_header() {
        let request = build_chat_request(
            &MistralRequestConfig {
                base_url: "http://127.0.0.1:8080/v1".to_string(),
                version: "0.1.0".to_string(),
                auth: MistralAuth::None,
                custom_headers: IndexMap::new(),
                session_id: None,
            },
            &MistralChatRequest {
                model: "demo".to_string(),
                messages: vec![MistralChatMessage {
                    role: "user".to_string(),
                    content: Some(json!("hello")),
                    tool_call_id: None,
                    name: None,
                    tool_calls: Vec::new(),
                }],
                tools: Vec::new(),
                tool_choice: None,
                max_tokens: None,
                stream: false,
            },
        )
        .expect("request");
        assert!(!request
            .headers
            .iter()
            .any(|(key, _)| key.eq_ignore_ascii_case("authorization")));
        assert_eq!(request.url, "http://127.0.0.1:8080/v1/chat/completions");
    }
}
