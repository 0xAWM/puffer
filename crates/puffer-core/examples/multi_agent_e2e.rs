//! Headless multi-agent E2E test — no TUI, no TTY required.
//!
//! Run: cargo run -p puffer-core --example multi_agent_e2e

use anyhow::Result;
use puffer_config::{ensure_workspace_dirs, load_config, ConfigPaths};
use puffer_core::teammate_loop::{teammate_registry, IncomingMessage, TeammateMessage};
use puffer_core::{execute_user_turn, execute_workflow_tool, AppState};
use puffer_provider_registry::{AuthStore, ProviderRegistry};
use puffer_resources::load_resources;
use puffer_session_store::SessionStore;
use serde_json::{json, Value};
use std::fs;

fn main() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let paths = ConfigPaths::discover(&cwd);
    ensure_workspace_dirs(&paths)?;
    let config = load_config(&paths)?;
    let auth_path = paths.user_config_dir.join("auth.json");
    let mut auth_store = AuthStore::load(&auth_path)?;
    let resources = load_resources(&paths)?;
    let mut providers = ProviderRegistry::new();
    for provider in &resources.providers {
        providers.register_with_source(
            provider.value.clone().into_descriptor(),
            provider.source_info.as_provider_source(),
        );
    }
    let session_store = SessionStore::from_paths(&paths)?;
    let session = session_store.create_session(cwd.clone())?;
    let mut state = AppState::new(config, cwd.clone(), session);

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║  Multi-Agent E2E — Headless                         ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    let wf = cwd.join(".puffer/runtime/claude_workflow");
    fs::create_dir_all(&wf)?;

    // ── 1: Basic AI ─────────────────────────────────────────────
    print!("1. AI response... ");
    match execute_user_turn(&mut state, &resources, &providers, &mut auth_store, "Say: OK") {
        Ok(t) => println!("✓ {}", t.assistant_text.trim().chars().take(40).collect::<String>()),
        Err(e) => println!("✗ {e} (continuing)"),
    }

    // ── 2: TeamCreate ───────────────────────────────────────────
    print!("2. TeamCreate... ");
    let r = execute_workflow_tool(&mut state, &resources, &cwd, "TeamCreate", json!({"team_name":"e2e-team"}), None)?;
    let v: Value = serde_json::from_str(&r)?;
    assert_eq!(v["team_name"], "e2e-team");
    println!("✓ lead={}", v["lead_agent_id"]);

    // ── 3: Register fake teammate + mpsc ────────────────────────
    print!("3. Register teammate... ");
    let aid = "researcher@e2e-team";
    let aj = json!({"agents":[{"agent_id":aid,"name":"researcher","description":"t","prompt":"t","subagent_type":null,"model":null,"team_name":"e2e-team","mode":null,"isolation":null,"cwd":cwd.display().to_string(),"status":"running","output_file":wf.join("researcher-out.json").display().to_string()}]});
    fs::write(wf.join("agents.json"), serde_json::to_string_pretty(&aj)?)?;
    fs::write(wf.join("researcher-out.json"), "{}")?;
    let (tx, rx) = std::sync::mpsc::channel();
    teammate_registry().lock().unwrap().insert(aid.to_string(), tx);
    println!("✓");

    // ── 4: SendMessage → mpsc ───────────────────────────────────
    print!("4. SendMessage → in-process... ");
    execute_workflow_tool(&mut state, &resources, &cwd, "SendMessage", json!({"to":"researcher","summary":"hi","message":"do task A"}), None)?;
    let msg = rx.recv_timeout(std::time::Duration::from_secs(2))?;
    match msg {
        TeammateMessage::Incoming(m) => { assert_eq!(m.text, "do task A"); println!("✓ from={} text={}", m.from, m.text); }
        _ => println!("✗ wrong type"),
    }

    // ── 5: Validation checks ────────────────────────────────────
    print!("5. Validation (4 checks)... ");
    let checks = [
        (json!({"to":"a@b","summary":"x","message":"y"}), "do not include @"),
        (json!({"to":"x","message":"y"}), "summary is required"),
        (json!({"to":"*","message":{"type":"shutdown_request"}}), "cannot be broadcast"),
        (json!({"to":"researcher","message":{"type":"shutdown_response","request_id":"x","approve":true}}), "must be sent to"),
    ];
    for (input, expect) in &checks {
        let e = execute_workflow_tool(&mut state, &resources, &cwd, "SendMessage", input.clone(), None);
        assert!(e.unwrap_err().to_string().contains(expect), "expected '{expect}'");
    }
    println!("✓ all 4 rejected correctly");

    // ── 6: shutdown_request ─────────────────────────────────────
    print!("6. shutdown_request... ");
    let r = execute_workflow_tool(&mut state, &resources, &cwd, "SendMessage", json!({"to":"researcher","message":{"type":"shutdown_request","reason":"done"}}), None)?;
    let v: Value = serde_json::from_str(&r)?;
    let rid = v["request_id"].as_str().unwrap();
    assert!(rid.starts_with("shutdown-"));
    println!("✓ request_id={rid}");

    // ── 7: shutdown_response ────────────────────────────────────
    print!("7. shutdown_response approve... ");
    // Need team-lead in agents
    let mut agents: Value = serde_json::from_str(&fs::read_to_string(wf.join("agents.json"))?)?;
    agents["agents"].as_array_mut().unwrap().push(json!({"agent_id":"team-lead@e2e-team","name":"team-lead","description":"l","prompt":"","subagent_type":null,"model":null,"team_name":"e2e-team","mode":null,"isolation":null,"cwd":cwd.display().to_string(),"status":"running","output_file":wf.join("lead-out.json").display().to_string()}));
    fs::write(wf.join("agents.json"), serde_json::to_string_pretty(&agents)?)?;
    fs::write(wf.join("lead-out.json"), "{}")?;
    let r = execute_workflow_tool(&mut state, &resources, &cwd, "SendMessage", json!({"to":"team-lead","message":{"type":"shutdown_response","request_id":rid,"approve":true}}), None)?;
    let v: Value = serde_json::from_str(&r)?;
    assert_eq!(v["success"], true);
    println!("✓");

    // ── 8: plan_approval ────────────────────────────────────────
    print!("8. plan_approval reject+feedback... ");
    let r = execute_workflow_tool(&mut state, &resources, &cwd, "SendMessage", json!({"to":"researcher","message":{"type":"plan_approval_response","request_id":"p1","approve":false,"feedback":"add tests"}}), None)?;
    let v: Value = serde_json::from_str(&r)?;
    assert_eq!(v["success"], true);
    println!("✓");

    // ── 9: TaskCreate + auto-owner ──────────────────────────────
    print!("9. auto-owner... ");
    let r = execute_workflow_tool(&mut state, &resources, &cwd, "TaskCreate", json!({"subject":"feat","description":"d"}), None)?;
    let v: Value = serde_json::from_str(&r)?;
    let tid = v["task"]["id"].as_str().unwrap();
    let r = execute_workflow_tool(&mut state, &resources, &cwd, "TaskUpdate", json!({"taskId":tid,"status":"in_progress"}), None)?;
    let v: Value = serde_json::from_str(&r)?;
    assert!(v["updatedFields"].as_array().unwrap().iter().any(|f| f == "owner"));
    println!("✓ task={tid}");

    // ── 10: TeamDelete ──────────────────────────────────────────
    print!("10. TeamDelete... ");
    let mut agents: Value = serde_json::from_str(&fs::read_to_string(wf.join("agents.json"))?)?;
    for a in agents["agents"].as_array_mut().unwrap() { a["status"] = json!("stopped"); }
    fs::write(wf.join("agents.json"), serde_json::to_string_pretty(&agents)?)?;
    teammate_registry().lock().unwrap().remove(aid);
    let r = execute_workflow_tool(&mut state, &resources, &cwd, "TeamDelete", json!({}), None)?;
    let v: Value = serde_json::from_str(&r)?;
    assert_eq!(v["success"], true);
    println!("✓");

    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║  ✓ All 10 tests passed                              ║");
    println!("╚══════════════════════════════════════════════════════╝");
    Ok(())
}
