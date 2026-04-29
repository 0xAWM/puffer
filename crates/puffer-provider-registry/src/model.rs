use crate::auth::AuthMode;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Describes the response format used by a provider's model discovery endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelDiscoveryFormat {
    OpenAiModels,
    AnthropicModels,
    OllamaModels,
}

/// Configures runtime discovery for provider-reported models.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelDiscoveryConfig {
    pub path: String,
    pub response: ModelDiscoveryFormat,
    pub api: String,
    pub context_window: u32,
    pub max_output_tokens: u32,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default = "default_items_field")]
    pub items_field: String,
    #[serde(default = "default_id_field")]
    pub id_field: String,
    #[serde(default)]
    pub display_name_field: Option<String>,
    #[serde(default)]
    pub headers: IndexMap<String, String>,
}

/// Describes the origin of a registered provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSourceKind {
    Builtin,
    ResourcePack,
    UserConfig,
    WorkspaceConfig,
}

/// Carries source provenance for a registered provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSource {
    pub kind: ProviderSourceKind,
    #[serde(default)]
    pub path: Option<String>,
}

/// Describes one provider model exposed to the rest of the application.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub id: String,
    pub display_name: String,
    pub provider: String,
    pub api: String,
    pub context_window: u32,
    pub max_output_tokens: u32,
    #[serde(default)]
    pub supports_reasoning: bool,
    /// Optional declarative API-shape compat overrides. When `None` (the
    /// common case), runtime helpers in `puffer-core` auto-detect each
    /// flag from `base_url` / `provider.id` to preserve historical
    /// behavior. Set explicitly for third-party endpoints whose URL
    /// shape doesn't match the heuristic — e.g. an OpenRouter relay
    /// that exposes a `/chat/completions` style API but lives at a
    /// non-`api.openai.com` URL, or a self-hosted Anthropic-compatible
    /// proxy that exposes a `thinking` block. Inspired by pi-mono's
    /// `Model<TApi>.compat?` field.
    #[serde(default)]
    pub compat: Option<ModelCompat>,
}

/// API-discriminated declarative compat override. The `api` tag mirrors
/// `ModelDescriptor::api` — runtime helpers should pick the matching
/// variant or fall back to URL-based auto-detection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "api")]
pub enum ModelCompat {
    #[serde(rename = "openai-responses")]
    OpenAiResponses(OpenAiResponsesCompat),
    #[serde(rename = "openai-completions")]
    OpenAiCompletions(OpenAiCompletionsCompat),
    #[serde(rename = "anthropic-messages")]
    AnthropicMessages(AnthropicMessagesCompat),
}

/// Compat flags for OpenAI Responses-shaped providers (the canonical
/// public OpenAI Responses API plus its codex / azure variants).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAiResponsesCompat {
    /// Whether the provider supports server-side response threading via
    /// `previous_response_id`. Auto-detected from
    /// `provider.id == "openai" && base_url.contains("api.openai.com")`
    /// or `base_url.contains("/api/codex")` when not set.
    #[serde(default)]
    pub supports_response_threading: Option<bool>,
    /// HTTP path for the Responses endpoint. Auto-detected:
    /// `/responses` for `/backend-api` or `/api/codex` URLs;
    /// `/v1/responses` everywhere else.
    #[serde(default)]
    pub responses_path: Option<ResponsesPath>,
    /// Whether to inject the codex-compat `version: <const>` header.
    /// Auto-detected from `provider.id == "openai"`.
    #[serde(default)]
    pub send_codex_version_header: Option<bool>,
    /// Whether the provider is "codex-style" (uses the Codex pseudo-API
    /// shape rather than the canonical Responses shape). Auto-detected
    /// from `default_api == "openai-codex-responses"` or
    /// base_url containing `/backend-api` or `/api/codex`.
    #[serde(default)]
    pub codex_style: Option<bool>,
    /// Override base_url under OAuth credentials. When set, OAuth flows
    /// route to this URL instead of `provider.base_url`. The default
    /// `https://chatgpt.com/backend-api/codex` rewrite still applies for
    /// `provider.id == "openai"` when this is `None`.
    #[serde(default)]
    pub oauth_base_url: Option<String>,
}

/// Compat flags for OpenAI Chat-Completions-shaped providers (the
/// canonical OpenAI Chat Completions API plus its many "compatible"
/// relays — groq, cerebras, openrouter, etc.).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAiCompletionsCompat {
    // Reserved — pi-mono uses this slot for `supportsStore`,
    // `supportsDeveloperRole`, `reasoningEffortMap`, etc. Adding the
    // first such flag here is mechanical once a third-party endpoint
    // forces it. Empty struct round-trips through serde without churn.
}

/// Compat flags for Anthropic Messages-shaped providers (canonical
/// `api.anthropic.com` plus its self-hosted relays).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnthropicMessagesCompat {
    /// Whether the provider accepts the Anthropic `thinking` block.
    /// Auto-detected from `provider.id == "anthropic"` or
    /// `base_url.contains("anthropic.com")`.
    #[serde(default)]
    pub supports_thinking_api: Option<bool>,
}

/// Wire path for the OpenAI Responses API.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponsesPath {
    /// `/v1/responses` — the canonical public OpenAI endpoint.
    V1Responses,
    /// `/responses` — codex / backend-api style endpoint.
    Responses,
}

impl ModelCompat {
    /// Returns the OpenAI Responses compat block when this variant matches.
    pub fn as_openai_responses(&self) -> Option<&OpenAiResponsesCompat> {
        match self {
            ModelCompat::OpenAiResponses(c) => Some(c),
            _ => None,
        }
    }

    /// Returns the OpenAI Chat Completions compat block when this variant matches.
    pub fn as_openai_completions(&self) -> Option<&OpenAiCompletionsCompat> {
        match self {
            ModelCompat::OpenAiCompletions(c) => Some(c),
            _ => None,
        }
    }

    /// Returns the Anthropic Messages compat block when this variant matches.
    pub fn as_anthropic_messages(&self) -> Option<&AnthropicMessagesCompat> {
        match self {
            ModelCompat::AnthropicMessages(c) => Some(c),
            _ => None,
        }
    }
}

/// Describes one model provider and the models it exposes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    pub default_api: String,
    #[serde(default)]
    pub auth_modes: Vec<AuthMode>,
    #[serde(default)]
    pub headers: IndexMap<String, String>,
    #[serde(default)]
    pub query_params: IndexMap<String, String>,
    #[serde(default)]
    pub discovery: Option<ModelDiscoveryConfig>,
    #[serde(default)]
    pub models: Vec<ModelDescriptor>,
}

/// Stores one provider plus its provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisteredProvider {
    pub descriptor: ProviderDescriptor,
    pub source: ProviderSource,
}

fn default_items_field() -> String {
    "data".to_string()
}

fn default_id_field() -> String {
    "id".to_string()
}
