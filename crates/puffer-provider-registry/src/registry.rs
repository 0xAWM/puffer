use crate::auth::{AuthStore, StoredCredential};
use crate::model::{
    ModelDescriptor, ModelDiscoveryConfig, ModelDiscoveryFormat, ProviderDescriptor, ProviderSource,
    ProviderSourceKind, RegisteredProvider,
};
use anyhow::{anyhow, Context, Result};
use indexmap::IndexMap;
use reqwest::blocking::Client;
use serde_json::Value;

/// Stores all providers and models known to the application.
#[derive(Debug, Clone, Default)]
pub struct ProviderRegistry {
    providers: IndexMap<String, RegisteredProvider>,
}

impl ProviderRegistry {
    /// Creates an empty provider registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or replaces a provider descriptor using the builtin source kind.
    pub fn register(&mut self, provider: ProviderDescriptor) {
        self.register_with_source(
            provider,
            ProviderSource {
                kind: ProviderSourceKind::Builtin,
                path: None,
            },
        );
    }

    /// Registers or replaces a provider descriptor with explicit provenance.
    pub fn register_with_source(&mut self, provider: ProviderDescriptor, source: ProviderSource) {
        self.providers.insert(
            provider.id.clone(),
            RegisteredProvider {
                descriptor: provider,
                source,
            },
        );
    }

    /// Registers a sequence of providers into the registry.
    pub fn register_many(&mut self, providers: impl IntoIterator<Item = ProviderDescriptor>) {
        for provider in providers {
            self.register(provider);
        }
    }

    /// Returns an iterator over all registered provider descriptors in insertion order.
    pub fn providers(&self) -> impl Iterator<Item = &ProviderDescriptor> {
        self.providers.values().map(|provider| &provider.descriptor)
    }

    /// Returns an iterator over all registered providers including provenance.
    pub fn provider_entries(&self) -> impl Iterator<Item = &RegisteredProvider> {
        self.providers.values()
    }

    /// Looks up a provider descriptor by id.
    pub fn provider(&self, id: &str) -> Option<&ProviderDescriptor> {
        self.providers.get(id).map(|provider| &provider.descriptor)
    }

    /// Looks up a registered provider entry by id.
    pub fn provider_entry(&self, id: &str) -> Option<&RegisteredProvider> {
        self.providers.get(id)
    }

    /// Returns an iterator over all known models across all providers.
    pub fn models(&self) -> impl Iterator<Item = &ModelDescriptor> {
        self.providers
            .values()
            .flat_map(|provider| provider.descriptor.models.iter())
    }

    /// Resolves a model from a `provider/model` selector string.
    pub fn resolve_model(&self, value: &str) -> Option<&ModelDescriptor> {
        let (provider_id, model_id) = value.split_once('/')?;
        self.provider(provider_id)?
            .models
            .iter()
            .find(|model| model.id == model_id)
    }

    /// Discovers and merges runtime models for every provider that exposes discovery config.
    pub fn discover_and_merge_all(&mut self, auth_store: &AuthStore) -> Result<()> {
        let provider_ids = self.providers.keys().cloned().collect::<Vec<_>>();
        for provider_id in provider_ids {
            self.discover_and_merge_provider(&provider_id, auth_store)?;
        }
        Ok(())
    }

    /// Discovers and merges runtime models for one provider when discovery is configured.
    pub fn discover_and_merge_provider(
        &mut self,
        provider_id: &str,
        auth_store: &AuthStore,
    ) -> Result<()> {
        let Some(provider) = self.providers.get(provider_id).cloned() else {
            return Err(anyhow!("provider {provider_id} is not registered"));
        };
        let Some(discovery) = provider.descriptor.discovery.clone() else {
            return Ok(());
        };
        let discovered = fetch_models_for_provider(&provider.descriptor, &discovery, auth_store)?;
        if discovered.is_empty() {
            return Ok(());
        }
        if let Some(entry) = self.providers.get_mut(provider_id) {
            merge_discovered_models(&mut entry.descriptor.models, discovered);
        }
        Ok(())
    }
}

fn fetch_models_for_provider(
    provider: &ProviderDescriptor,
    discovery: &ModelDiscoveryConfig,
    auth_store: &AuthStore,
) -> Result<Vec<ModelDescriptor>> {
    let url = format!(
        "{}{}",
        provider.base_url.trim_end_matches('/'),
        discovery.path
    );
    let client = Client::new();
    let mut request = client.get(&url);
    for (key, value) in &provider.headers {
        request = request.header(key, value);
    }
    request = apply_discovery_auth(request, provider.id.as_str(), auth_store);
    let response = request
        .send()
        .with_context(|| format!("failed to fetch models from {url}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("model discovery for {} failed with {status}", provider.id));
    }
    let payload = response
        .json::<Value>()
        .with_context(|| format!("failed to parse discovery response from {url}"))?;
    parse_discovered_models(provider, discovery, &payload)
}

fn apply_discovery_auth(
    mut request: reqwest::blocking::RequestBuilder,
    provider_id: &str,
    auth_store: &AuthStore,
) -> reqwest::blocking::RequestBuilder {
    match auth_store.get(provider_id) {
        Some(StoredCredential::ApiKey { key }) if provider_id == "anthropic" => {
            request = request.header("x-api-key", key);
            request = request.header("anthropic-version", "2023-06-01");
            request
        }
        Some(StoredCredential::ApiKey { key }) => request.header("Authorization", format!("Bearer {key}")),
        Some(StoredCredential::OAuth(credential)) => {
            request.header("Authorization", format!("Bearer {}", credential.access_token))
        }
        None => request,
    }
}

fn parse_discovered_models(
    provider: &ProviderDescriptor,
    discovery: &ModelDiscoveryConfig,
    payload: &Value,
) -> Result<Vec<ModelDescriptor>> {
    let items = payload
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("discovery response for {} missing data array", provider.id))?;
    let mut models = Vec::new();
    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("discovery model for {} missing id", provider.id))?;
        let display_name = match discovery.response {
            ModelDiscoveryFormat::AnthropicModels => item
                .get("display_name")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(id),
            ModelDiscoveryFormat::OpenAiModels => item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(id),
        };
        models.push(ModelDescriptor {
            id: id.to_string(),
            display_name: display_name.to_string(),
            provider: provider.id.clone(),
            api: discovery.api.clone(),
            context_window: discovery.context_window,
            max_output_tokens: discovery.max_output_tokens,
            supports_reasoning: discovery.supports_reasoning,
        });
    }
    Ok(models)
}

fn merge_discovered_models(existing: &mut Vec<ModelDescriptor>, discovered: Vec<ModelDescriptor>) {
    for model in discovered {
        if existing.iter().any(|current| current.id == model.id) {
            continue;
        }
        existing.push(model);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthMode;

    fn provider_descriptor() -> ProviderDescriptor {
        ProviderDescriptor {
            id: "anthropic".to_string(),
            display_name: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            default_api: "anthropic-messages".to_string(),
            auth_modes: vec![AuthMode::ApiKey, AuthMode::OAuth],
            headers: IndexMap::new(),
            discovery: None,
            models: vec![ModelDescriptor {
                id: "claude-sonnet-4-5".to_string(),
                display_name: "Claude Sonnet 4.5".to_string(),
                provider: "anthropic".to_string(),
                api: "anthropic-messages".to_string(),
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_reasoning: true,
            }],
        }
    }

    #[test]
    fn registry_tracks_provider_sources() {
        let mut registry = ProviderRegistry::new();
        registry.register_with_source(
            provider_descriptor(),
            ProviderSource {
                kind: ProviderSourceKind::ResourcePack,
                path: Some("resources/providers/anthropic.yaml".to_string()),
            },
        );

        let entry = registry
            .provider_entry("anthropic")
            .expect("provider entry");
        assert_eq!(entry.source.kind, ProviderSourceKind::ResourcePack);
        assert_eq!(
            registry
                .resolve_model("anthropic/claude-sonnet-4-5")
                .expect("model")
                .display_name,
            "Claude Sonnet 4.5"
        );
    }

    #[test]
    fn parse_openai_discovery_response_maps_models() {
        let provider = ProviderDescriptor {
            discovery: Some(ModelDiscoveryConfig {
                path: "/v1/models".to_string(),
                response: ModelDiscoveryFormat::OpenAiModels,
                api: "openai-responses".to_string(),
                context_window: 272_000,
                max_output_tokens: 16_384,
                supports_reasoning: true,
            }),
            ..provider_descriptor()
        };
        let models = parse_discovered_models(
            &provider,
            provider.discovery.as_ref().unwrap(),
            &serde_json::json!({
                "data": [
                    { "id": "gpt-5" },
                    { "id": "gpt-5-mini" }
                ]
            }),
        )
        .unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gpt-5");
        assert_eq!(models[0].api, "openai-responses");
    }

    #[test]
    fn merge_discovered_models_only_adds_missing_ids() {
        let mut models = vec![ModelDescriptor {
            id: "claude-sonnet-4-5".to_string(),
            display_name: "Claude Sonnet 4.5".to_string(),
            provider: "anthropic".to_string(),
            api: "anthropic-messages".to_string(),
            context_window: 200_000,
            max_output_tokens: 8_192,
            supports_reasoning: true,
        }];
        merge_discovered_models(
            &mut models,
            vec![
                ModelDescriptor {
                    id: "claude-sonnet-4-5".to_string(),
                    display_name: "Claude Sonnet 4.5".to_string(),
                    provider: "anthropic".to_string(),
                    api: "anthropic-messages".to_string(),
                    context_window: 200_000,
                    max_output_tokens: 8_192,
                    supports_reasoning: true,
                },
                ModelDescriptor {
                    id: "claude-opus-4-1".to_string(),
                    display_name: "Claude Opus 4.1".to_string(),
                    provider: "anthropic".to_string(),
                    api: "anthropic-messages".to_string(),
                    context_window: 200_000,
                    max_output_tokens: 8_192,
                    supports_reasoning: true,
                },
            ],
        );
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|model| model.id == "claude-opus-4-1"));
    }
}
