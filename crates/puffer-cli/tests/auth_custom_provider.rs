use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use std::fs;
use std::process::Command;

#[test]
fn oauth_url_uses_openai_family_for_custom_provider() {
    let (_tempdir, workspace) = configured_workspace(
        "custom-openai",
        r#"
id: custom-openai
display_name: Custom OpenAI
base_url: https://example.invalid/v1
default_api: openai-responses
auth_modes:
  - oauth
models:
  - id: demo
    display_name: Demo
    provider: custom-openai
    api: openai-responses
    context_window: 1000
    max_output_tokens: 100
    supports_reasoning: false
"#,
    );
    let output = run_puffer(&workspace, &["auth", "oauth-url", "custom-openai"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("auth.openai.com"));
}

#[test]
fn oauth_start_uses_anthropic_family_for_custom_provider() {
    let (_tempdir, workspace) = configured_workspace(
        "custom-anthropic",
        r#"
id: custom-anthropic
display_name: Custom Anthropic
base_url: https://example.invalid
default_api: anthropic-messages
auth_modes:
  - oauth
models:
  - id: demo
    display_name: Demo
    provider: custom-anthropic
    api: anthropic-messages
    context_window: 1000
    max_output_tokens: 100
    supports_reasoning: false
"#,
    );
    let output = run_puffer(&workspace, &["auth", "oauth-start", "custom-anthropic"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"provider\": \"custom-anthropic\""));
    assert!(stdout.contains("oauth/authorize"));
    assert!(stdout.contains("\"redirect_uri\""));
}

fn configured_workspace(
    name: &str,
    provider_yaml: &str,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let workspace = tempdir.path().join("workspace");
    fs::create_dir_all(&workspace).expect("workspace");
    let paths = ConfigPaths::discover(&workspace);
    ensure_workspace_dirs(&paths).expect("dirs");
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("cli crate parent")
        .parent()
        .expect("repo root");
    std::os::unix::fs::symlink(repo_root.join("resources"), workspace.join("resources"))
        .expect("resource symlink");
    let provider_dir = paths.workspace_config_dir.join("resources/providers");
    fs::create_dir_all(&provider_dir).expect("provider dir");
    fs::write(provider_dir.join(format!("{name}.yaml")), provider_yaml).expect("provider yaml");
    (tempdir, workspace)
}

fn run_puffer(workspace: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_puffer"))
        .args(args)
        .current_dir(workspace)
        .env("HOME", workspace)
        .output()
        .expect("puffer process")
}
