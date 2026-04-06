//! Canonical public surface for the OpenAI provider crate.
//!
//! The crate root is the only supported public entrypoint. The internal
//! `auth` and `request` modules remain private implementation details so other
//! crates consume a stable, curated API from one place.

mod auth;
mod request;

pub use auth::OpenAIAuth;
pub use auth::OpenAIOAuthConfig;
pub use auth::OpenAIOAuthCredentials;
pub use auth::OpenAIPkce;
pub use auth::OPENAI_AUTHORIZE_URL;
pub use auth::OPENAI_CODEX_CLIENT_ID;
pub use auth::OPENAI_REDIRECT_URI;
pub use auth::OPENAI_SCOPE;
pub use auth::OPENAI_TOKEN_URL;
pub use request::BuiltOpenAIRequest;
pub use request::OpenAIRequestConfig;
pub use request::OpenAIResponsesRequest;

/// Generates a PKCE verifier, challenge, and state for the OpenAI OAuth flow.
pub fn generate_pkce() -> OpenAIPkce {
    auth::generate_pkce()
}

/// Builds the OpenAI OAuth authorization URL for the provided flow settings.
pub fn build_authorization_url(config: &OpenAIOAuthConfig) -> String {
    auth::build_authorization_url(config)
}

/// Extracts an authorization code and optional state from pasted user input.
pub fn parse_authorization_input(input: &str) -> (Option<String>, Option<String>) {
    auth::parse_authorization_input(input)
}

/// Exchanges an OAuth authorization code for OpenAI bearer credentials.
pub fn exchange_authorization_code(
    code: &str,
    verifier: &str,
    redirect_uri: Option<&str>,
) -> anyhow::Result<OpenAIOAuthCredentials> {
    auth::exchange_authorization_code(code, verifier, redirect_uri)
}

/// Refreshes OpenAI bearer credentials from a stored refresh token.
pub fn refresh_oauth_token(refresh_token: &str) -> anyhow::Result<OpenAIOAuthCredentials> {
    auth::refresh_oauth_token(refresh_token)
}

/// Builds an ordered OpenAI Responses API request for execution or testing.
pub fn build_responses_request(
    config: &OpenAIRequestConfig,
    request: &OpenAIResponsesRequest,
) -> anyhow::Result<BuiltOpenAIRequest> {
    request::build_responses_request(config, request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_root_builds_authorization_url() {
        let url = build_authorization_url(&OpenAIOAuthConfig {
            state: "state-1".to_string(),
            code_challenge: "challenge-1".to_string(),
            redirect_uri: OPENAI_REDIRECT_URI.to_string(),
            originator: "puffer".to_string(),
        });
        assert!(url.contains("state=state-1"));
        assert!(url.contains("code_challenge=challenge-1"));
    }

    #[test]
    fn crate_root_builds_request() {
        let request = build_responses_request(
            &OpenAIRequestConfig {
                base_url: "https://api.openai.com".to_string(),
                version: "0.1.0".to_string(),
                auth: OpenAIAuth::ApiKey("sk-test".to_string()),
            },
            &OpenAIResponsesRequest {
                model: "gpt-5".to_string(),
                input: "hello".to_string(),
            },
        )
        .expect("request should build");
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "https://api.openai.com/v1/responses");
    }
}
