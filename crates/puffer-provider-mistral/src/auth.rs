use serde::{Deserialize, Serialize};

/// Authentication modes supported by the Mistral provider crate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MistralAuth {
    None,
    ApiKey(String),
}
