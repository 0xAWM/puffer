/// Authentication modes supported by the Mistral request builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MistralAuth {
    ApiKey(String),
    None,
}
