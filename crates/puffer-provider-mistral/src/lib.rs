//! Public surface for the Mistral provider crate.

mod auth;
mod request;
mod response;

pub use auth::MistralAuth;
pub use request::BuiltMistralRequest;
pub use request::MistralChatMessage;
pub use request::MistralChatRequest;
pub use request::MistralRequestConfig;
pub use request::MistralTool;
pub use request::MistralToolChoice;
pub use request::MistralToolChoiceMode;
pub use request::MistralToolChoiceName;
pub use request::MistralToolFunction;
pub use request::MistralToolFunctionDefinition;
pub use request::MistralToolCall;
pub use request::MistralToolFunctionCall;
pub use response::MistralChatChoice;
pub use response::MistralChatChoiceMessage;
pub use response::MistralChatResponse;
pub use response::MistralParsedToolCall;

/// Builds an ordered Mistral chat request.
pub fn build_chat_request(
    config: &MistralRequestConfig,
    request: &MistralChatRequest,
) -> anyhow::Result<BuiltMistralRequest> {
    request::build_chat_request(config, request)
}

/// Parses a serialized Mistral chat payload.
pub fn parse_chat_response(payload: &str) -> anyhow::Result<MistralChatResponse> {
    response::parse_chat_response(payload)
}

/// Extracts assistant text from a parsed Mistral chat response.
pub fn extract_chat_text(response: &MistralChatResponse) -> String {
    response::extract_chat_text(response)
}

/// Extracts tool calls from a parsed Mistral chat response.
pub fn extract_chat_tool_calls(
    response: &MistralChatResponse,
) -> anyhow::Result<Vec<MistralParsedToolCall>> {
    response::extract_chat_tool_calls(response)
}
