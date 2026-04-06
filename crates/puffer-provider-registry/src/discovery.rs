use crate::auth::{AuthStore, StoredCredential};
use crate::model::{
    ModelDescriptor, ModelDiscoveryConfig, ModelDiscoveryFormat, ProviderDescriptor,
};
use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde_json::Value;

/// Fetches and parses provider model catalogs from runtime discovery endpoints.
#[derive(Debug, Clone)]
pub struct ModelDiscoveryClient {
    client: Client,
}

impl Default for ModelDiscoveryClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelDiscoveryClient {
    /// Creates a blocking discovery client for provider model lookups.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Fetches and parses discovered models for one provider descriptor.
    pub fn discover_models(
        &self,
        provider: &ProviderDescriptor,
        auth_store: &AuthStore,
    ) -> Result<Vec<ModelDescriptor>> {
        let Some(discovery) = provider.discovery.as_ref() else {
            return Ok(Vec::new());
        };
        let payload = self.fetch_payload(provider, discovery, auth_store)?;
        parse_discovered_models(provider, discovery, &payload)
    }

    fn fetch_payload(
        &self,
        provider: &ProviderDescriptor,
        discovery: &ModelDiscoveryConfig,
        auth_store: &AuthStore,
    ) -> Result<Value> {
        let url = format!(
            "{}{}",
            provider.base_url.trim_end_matches('/'),
            discovery.path
        );
        let mut request = self.client.get(&url);
        for (key, value) in &provider.headers {
            request = request.header(key, value);
        }
        for (key, value) in &discovery.headers {
            request = request.header(key, value);
        }
        request = apply_discovery_auth(request, provider.id.as_str(), auth_store);
        let response = request
            .send()
            .with_context(|| format!("failed to fetch models from {url}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(anyhow!(
                "model discovery for {} failed with {status}",
                provider.id
            ));
        }
        response
            .json::<Value>()
            .with_context(|| format!("failed to parse discovery response from {url}"))
    }
}

/// Merges newly discovered models into an existing provider catalog without duplicates.
pub fn merge_discovered_models(existing: &mut Vec<ModelDescriptor>, discovered: Vec<ModelDescriptor>) {
    for model in discovered {
        if existing.iter().any(|current| current.id == model.id) {
            continue;
        }
        existing.push(model);
    }
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
        Some(StoredCredential::ApiKey { key }) => {
            request.header("Authorization", format!("Bearer {key}"))
        }
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
        .get(discovery.items_field.as_str())
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow!(
                "discovery response for {} missing {} array",
                provider.id,
                discovery.items_field
            )
        })?;
    let mut models = Vec::new();
    for item in items {
        let id = item
            .get(discovery.id_field.as_str())
            .and_then(Value::as_str)
            .ok_or_else(|| {
                anyhow!(
                    "discovery model for {} missing {}",
                    provider.id,
                    discovery.id_field
                )
            })?;
        let display_name = resolve_display_name(item, discovery, id);
        models.push(ModelDescriptor {
            id: id.to_string(),
            display_name,
            provider: provider.id.clone(),
            api: discovery.api.clone(),
            context_window: discovery.context_window,
            max_output_tokens: discovery.max_output_tokens,
            supports_reasoning: discovery.supports_reasoning,
        });
    }
    Ok(models)
}

fn resolve_display_name(item: &Value, discovery: &ModelDiscoveryConfig, id: &str) -> String {
    if let Some(field) = discovery.display_name_field.as_deref() {
        return item
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string();
    }
    match discovery.response {
        ModelDiscoveryFormat::AnthropicModels => item
            .get("display_name")
            .or_else(|| item.get("name"))
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string(),
        ModelDiscoveryFormat::OpenAiModels => item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthMode;
    use indexmap::IndexMap;

    fn provider(discovery: ModelDiscoveryConfig) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "custom".to_string(),
            display_name: "Custom".to_string(),
            base_url: "https://example.invalid".to_string(),
            default_api: "custom-api".to_string(),
            auth_modes: vec![AuthMode::ApiKey],
            headers: IndexMap::new(),
            discovery: Some(discovery),
            models: Vec::new(),
        }
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

    #[test]
    fn discovery_uses_custom_field_names() {
        let discovery = ModelDiscoveryConfig {
            path: "/models".to_string(),
            response: ModelDiscoveryFormat::OpenAiModels,
            api: "custom-api".to_string(),
            context_window: 32_000,
            max_output_tokens: 4_096,
            supports_reasoning: false,
            items_field: "items".to_string(),
            id_field: "slug".to_string(),
            display_name_field: Some("label".to_string()),
            headers: IndexMap::new(),
        };
        let payload = serde_json::json!({
            "items": [
                { "slug": "reasoner", "label": "Reasoner" }
            ]
        });
        let models =
            parse_discovered_models(&provider(discovery), &provider(ModelDiscoveryConfig {
                path: "/models".to_string(),
                response: ModelDiscoveryFormat::OpenAiModels,
                api: "custom-api".to_string(),
                context_window: 32_000,
                max_output_tokens: 4_096,
                supports_reasoning: false,
                items_field: "items".to_string(),
                id_field: "slug".to_string(),
                display_name_field: Some("label".to_string()),
                headers: IndexMap::new(),
            })
            .discovery
            .unwrap(), &payload)
            .expect("models");
        assert_eq!(models[0].id, "reasoner");
        assert_eq!(models[0].display_name, "Reasoner");
    }
}
