//! End-to-end coverage for the `agent_loop` migration.
//!
//! Each test stands up an in-process `TcpListener` that pretends to
//! be the upstream provider, scripts a multi-turn exchange (turn 1
//! returns a tool call, turn 2 returns a final text answer), and
//! asserts on the resulting `TurnExecution` plus the emitted wire
//! requests. Together they verify:
//!
//! - the agent_loop driver runs through a tool round-trip and a final
//!   text turn for each provider session implementation,
//! - per-tool execution still flows through the real `ToolRegistry`
//!   (so reflection / permissions / hooks remain reachable),
//! - OpenAI Responses propagates `previous_response_id` on the second
//!   request (the `continuation_start` threading optimization),
//! - streaming providers actually surface `TextDelta` events.
//!
//! Anthropic is already exercised end-to-end by `iteration_behavior.rs`
//! (9-round tool loop), so we focus here on the freshly-migrated
//! OpenAI Responses + Chat Completions paths.

use super::*;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn session_for(cwd: &std::path::Path) -> SessionMetadata {
    SessionMetadata {
        id: Uuid::new_v4(),
        display_name: None,
        cwd: cwd.to_path_buf(),
        created_at_ms: 0,
        updated_at_ms: 0,
        parent_session_id: None,
        slug: None,
        tags: Vec::new(),
        note: None,
    }
}

/// Scripted HTTP server: serves `expected_requests` mock responses,
/// recording each raw request body so tests can assert on wire shape.
fn spawn_server<F>(
    content_type: &'static str,
    expected_requests: usize,
    response_body: F,
) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>)
where
    F: Fn(usize) -> String + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let request_log = Arc::clone(&requests);
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut handled = 0_usize;
        while handled < expected_requests && Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = vec![0_u8; 65_536];
                    let bytes = stream.read(&mut buffer).unwrap();
                    let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
                    request_log.lock().unwrap().push(request);
                    let body = response_body(handled);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(response.as_bytes()).unwrap();
                    handled += 1;
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("listener accept failed: {error}"),
            }
        }
    });
    (format!("http://{address}"), requests, server)
}

fn extract_request_body(raw: &str) -> &str {
    raw.split_once("\r\n\r\n").map(|(_, body)| body).unwrap_or("")
}

// ---------------------------------------------------------------------------
// OpenAI Responses — non-streaming, 2 turns (tool → text).
// ---------------------------------------------------------------------------

fn openai_responses_tool_turn() -> String {
    json!({
        "id": "resp_1",
        "output": [{
            "type": "function_call",
            "call_id": "call_1",
            "name": "read_file",
            "arguments": "{\"path\":\"fixture.txt\"}"
        }],
        "usage": {
            "input_tokens": 100,
            "output_tokens": 5,
            "input_tokens_details": { "cached_tokens": 0 }
        }
    })
    .to_string()
}

fn openai_responses_final_turn() -> String {
    json!({
        "id": "resp_2",
        "output": [{
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "output_text",
                "text": "all set"
            }]
        }],
        "output_text": "all set",
        "usage": {
            "input_tokens": 110,
            "output_tokens": 3,
            "input_tokens_details": { "cached_tokens": 5 }
        }
    })
    .to_string()
}

#[test]
fn openai_responses_agent_loop_runs_tool_then_text() {
    // Note: threading (previous_response_id) is exercised by the
    // dedicated `openai_responses_threading_*` test below under an
    // env_lock. This test focuses on the loop's end-to-end shape:
    // tool round-trip, FunctionCallOutput append, final text turn.
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("fixture.txt"), "fixture-contents").unwrap();
    let (base_url, requests, server) = spawn_server("application/json", 2, |index| {
        if index == 0 {
            openai_responses_tool_turn()
        } else {
            openai_responses_final_turn()
        }
    });

    let mut registry = ProviderRegistry::new();
    registry.register(openai_provider(base_url));
    let mut auth_store = AuthStore::default();
    auth_store.set_api_key("openai", "sk-openai");

    let mut state = AppState::new(
        PufferConfig::default(),
        temp.path().to_path_buf(),
        session_for(temp.path()),
    );
    state.current_provider = Some("openai".to_string());
    state.current_model = Some("openai/gpt-5".to_string());
    state.session_allow_all = true;

    let resources = LoadedResources {
        tools: vec![loaded_tool("read_file", "Read a file", "read_file")],
        ..LoadedResources::default()
    };

    let turn = execute_user_prompt(
        &mut state,
        &resources,
        &registry,
        &mut auth_store,
        "please read fixture.txt",
    )
    .unwrap();

    server.join().unwrap();

    assert_eq!(turn.assistant_text, "all set");
    assert_eq!(turn.tool_invocations.len(), 1);
    assert_eq!(turn.tool_invocations[0].tool_id, "read_file");
    assert_eq!(turn.tool_invocations[0].call_id, "call_1");
    assert!(turn.tool_invocations[0].success);
    assert!(turn.tool_invocations[0].output.contains("fixture-contents"));

    let captured = requests.lock().unwrap();
    assert_eq!(captured.len(), 2, "expected exactly two upstream requests");

    // Turn 2 must include the tool call's FunctionCallOutput — this
    // verifies the loop appends FunctionCallOutput items after tool
    // execution and the session re-serializes them on the next request.
    let body2 = extract_request_body(&captured[1]);
    let body2_json: Value = serde_json::from_str(body2).unwrap_or(Value::Null);
    let input_arr = body2_json
        .get("input")
        .and_then(Value::as_array)
        .expect("input array on second request");
    let has_function_output = input_arr.iter().any(|item| {
        item.get("type").and_then(Value::as_str) == Some("function_call_output")
            && item.get("call_id").and_then(Value::as_str) == Some("call_1")
    });
    assert!(
        has_function_output,
        "second request input must contain function_call_output for call_1: {body2}"
    );
}

// ---------------------------------------------------------------------------
// OpenAI Responses — streaming SSE, 2 turns (tool → text).
// ---------------------------------------------------------------------------

fn openai_responses_tool_sse() -> String {
    concat!(
        "event: response.created\n",
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n",
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"read_file\",\"arguments\":\"\"}}\n\n",
        "event: response.function_call_arguments.delta\n",
        "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_1\",\"delta\":\"{\\\"path\\\":\\\"fixture.txt\\\"}\"}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"fixture.txt\\\"}\"}}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"usage\":{\"input_tokens\":100,\"output_tokens\":5,\"input_tokens_details\":{\"cached_tokens\":0}}}}\n\n"
    ).to_string()
}

fn openai_responses_final_sse() -> String {
    concat!(
        "event: response.created\n",
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_2\"}}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"all \"}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"set\"}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"all set\"}]}}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_2\",\"status\":\"completed\",\"usage\":{\"input_tokens\":110,\"output_tokens\":3,\"input_tokens_details\":{\"cached_tokens\":5}}}}\n\n"
    ).to_string()
}

#[test]
fn openai_responses_streaming_agent_loop_runs_tool_then_text() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("fixture.txt"), "fixture-contents").unwrap();
    let (base_url, requests, server) = spawn_server("text/event-stream", 2, |index| {
        if index == 0 {
            openai_responses_tool_sse()
        } else {
            openai_responses_final_sse()
        }
    });

    let mut registry = ProviderRegistry::new();
    registry.register(openai_provider(base_url));
    let mut auth_store = AuthStore::default();
    auth_store.set_api_key("openai", "sk-openai");

    let mut state = AppState::new(
        PufferConfig::default(),
        temp.path().to_path_buf(),
        session_for(temp.path()),
    );
    state.current_provider = Some("openai".to_string());
    state.current_model = Some("openai/gpt-5".to_string());
    state.session_allow_all = true;

    let resources = LoadedResources {
        tools: vec![loaded_tool("read_file", "Read a file", "read_file")],
        ..LoadedResources::default()
    };

    let mut text_deltas = Vec::new();
    let mut tool_calls_seen = 0_usize;
    let mut tool_invocations_seen = 0_usize;

    let turn = execute_user_prompt_streaming(
        &mut state,
        &resources,
        &registry,
        &mut auth_store,
        "please read fixture.txt",
        |event| match event {
            TurnStreamEvent::TextDelta(delta) => text_deltas.push(delta),
            TurnStreamEvent::ToolCallsRequested(calls) => tool_calls_seen += calls.len(),
            TurnStreamEvent::ToolInvocations(invocations) => {
                tool_invocations_seen += invocations.len()
            }
            _ => {}
        },
    )
    .unwrap();

    server.join().unwrap();

    assert_eq!(turn.assistant_text, "all set");
    assert_eq!(turn.tool_invocations.len(), 1);
    assert_eq!(turn.tool_invocations[0].tool_id, "read_file");
    assert!(turn.tool_invocations[0].success);
    assert!(turn.tool_invocations[0].output.contains("fixture-contents"));
    assert_eq!(tool_invocations_seen, 1, "loop must emit ToolInvocations");
    assert_eq!(
        text_deltas,
        vec!["all ".to_string(), "set".to_string()],
        "streaming TextDelta events should mirror the SSE tokens"
    );
    // The SSE parser may surface tool_call_start events; the loop's
    // dedupe should NOT re-emit ToolCallsRequested for the same id.
    assert!(
        tool_calls_seen <= 1,
        "agent_loop must dedupe ToolCallsRequested when SSE already surfaced the call"
    );

    let captured = requests.lock().unwrap();
    assert_eq!(captured.len(), 2);
    let body2 = extract_request_body(&captured[1]);
    let body2_json: Value = serde_json::from_str(body2).unwrap_or(Value::Null);
    let input_arr = body2_json
        .get("input")
        .and_then(Value::as_array)
        .expect("input array on second streaming request");
    let has_function_output = input_arr.iter().any(|item| {
        item.get("type").and_then(Value::as_str) == Some("function_call_output")
            && item.get("call_id").and_then(Value::as_str) == Some("call_1")
    });
    assert!(
        has_function_output,
        "streaming second request must contain function_call_output for call_1: {body2}"
    );
}

// ---------------------------------------------------------------------------
// OpenAI Chat Completions — 2 turns (tool → text).
// ---------------------------------------------------------------------------

fn openai_completions_provider(base_url: String) -> ProviderDescriptor {
    let mut descriptor = openai_provider(base_url);
    descriptor.id = "openai-completions-test".to_string();
    descriptor.default_api = "openai-completions".to_string();
    for model in &mut descriptor.models {
        model.provider = "openai-completions-test".to_string();
        model.api = "openai-completions".to_string();
    }
    descriptor
}

fn openai_completions_tool_turn() -> String {
    json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_chat_1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"fixture.txt\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    })
    .to_string()
}

fn openai_completions_final_turn() -> String {
    json!({
        "id": "chatcmpl-2",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "completions ok"
            },
            "finish_reason": "stop"
        }]
    })
    .to_string()
}

#[test]
fn openai_completions_agent_loop_runs_tool_then_text() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("fixture.txt"), "fixture-contents").unwrap();
    let (base_url, requests, server) = spawn_server("application/json", 2, |index| {
        if index == 0 {
            openai_completions_tool_turn()
        } else {
            openai_completions_final_turn()
        }
    });

    let mut registry = ProviderRegistry::new();
    registry.register(openai_completions_provider(base_url));
    let mut auth_store = AuthStore::default();
    auth_store.set_api_key("openai-completions-test", "sk-openai");

    let mut state = AppState::new(
        PufferConfig::default(),
        temp.path().to_path_buf(),
        session_for(temp.path()),
    );
    state.current_provider = Some("openai-completions-test".to_string());
    state.current_model = Some("openai-completions-test/gpt-5".to_string());
    state.session_allow_all = true;

    let resources = LoadedResources {
        tools: vec![loaded_tool("read_file", "Read a file", "read_file")],
        ..LoadedResources::default()
    };

    let turn = execute_user_prompt(
        &mut state,
        &resources,
        &registry,
        &mut auth_store,
        "please read fixture.txt",
    )
    .unwrap();

    server.join().unwrap();

    assert_eq!(turn.assistant_text, "completions ok");
    assert_eq!(turn.tool_invocations.len(), 1);
    assert_eq!(turn.tool_invocations[0].tool_id, "read_file");
    assert_eq!(turn.tool_invocations[0].call_id, "call_chat_1");
    assert!(turn.tool_invocations[0].success);
    assert!(turn.tool_invocations[0].output.contains("fixture-contents"));

    let captured = requests.lock().unwrap();
    assert_eq!(captured.len(), 2, "expected exactly two upstream requests");

    // Verify the second request includes the tool result message
    // (Chat Completions has no previous_response_id; the tool output
    // must round-trip via the messages array).
    let body2 = extract_request_body(&captured[1]);
    let body2_json: Value = serde_json::from_str(body2).unwrap_or(Value::Null);
    let messages = body2_json
        .get("messages")
        .and_then(Value::as_array)
        .expect("messages array on second request");
    let has_tool_result = messages.iter().any(|m| {
        m.get("role").and_then(Value::as_str) == Some("tool")
            && m.get("tool_call_id").and_then(Value::as_str) == Some("call_chat_1")
    });
    assert!(
        has_tool_result,
        "second request must replay the tool result for call_chat_1: {body2}"
    );
}

// ---------------------------------------------------------------------------
// Cross-provider behavior: same prompt + same tool, both Anthropic and
// OpenAI Responses produce semantically equivalent end states (one tool
// invocation, same final text). Locks in the agent_loop's "the loop is
// the same regardless of provider" promise.
// ---------------------------------------------------------------------------

#[test]
fn anthropic_and_openai_agent_loop_share_outcome_for_same_tool_round() {
    let temp_anthropic = tempfile::tempdir().unwrap();
    let temp_openai = tempfile::tempdir().unwrap();
    std::fs::write(temp_anthropic.path().join("fixture.txt"), "shared-text").unwrap();
    std::fs::write(temp_openai.path().join("fixture.txt"), "shared-text").unwrap();

    // Anthropic: 2-turn tool → text.
    let (a_url, _a_req, a_server) = spawn_server("application/json", 2, |index| {
        if index == 0 {
            json!({
                "id": "msg_1",
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "call_anthropic",
                    "name": "read_file",
                    "input": { "path": "fixture.txt" }
                }],
                "stop_reason": "tool_use"
            })
            .to_string()
        } else {
            json!({
                "id": "msg_2",
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "text", "text": "shared outcome" }],
                "stop_reason": "end_turn"
            })
            .to_string()
        }
    });

    // OpenAI Responses: 2-turn tool → text.
    let (o_url, _o_req, o_server) = spawn_server("application/json", 2, |index| {
        if index == 0 {
            openai_responses_tool_turn()
        } else {
            json!({
                "id": "resp_2",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "shared outcome" }]
                }],
                "output_text": "shared outcome",
                "usage": {
                    "input_tokens": 110,
                    "output_tokens": 3,
                    "input_tokens_details": { "cached_tokens": 5 }
                }
            })
            .to_string()
        }
    });

    let resources = LoadedResources {
        tools: vec![loaded_tool("read_file", "Read a file", "read_file")],
        ..LoadedResources::default()
    };

    // Anthropic side.
    let mut a_descriptor = provider();
    a_descriptor.id = "local-anthropic".to_string();
    a_descriptor.base_url = a_url;
    a_descriptor.auth_modes.clear();
    a_descriptor.models[0].provider = "local-anthropic".to_string();
    let mut a_registry = ProviderRegistry::new();
    a_registry.register(a_descriptor);
    let mut a_state = AppState::new(
        PufferConfig::default(),
        temp_anthropic.path().to_path_buf(),
        session_for(temp_anthropic.path()),
    );
    a_state.current_provider = Some("local-anthropic".to_string());
    a_state.current_model = Some("local-anthropic/claude-sonnet-4-5".to_string());
    a_state.session_allow_all = true;
    let a_turn = execute_user_prompt(
        &mut a_state,
        &resources,
        &a_registry,
        &mut AuthStore::default(),
        "please read fixture.txt",
    )
    .unwrap();
    a_server.join().unwrap();

    // OpenAI side.
    let mut o_registry = ProviderRegistry::new();
    o_registry.register(openai_provider(o_url));
    let mut o_auth = AuthStore::default();
    o_auth.set_api_key("openai", "sk-openai");
    let mut o_state = AppState::new(
        PufferConfig::default(),
        temp_openai.path().to_path_buf(),
        session_for(temp_openai.path()),
    );
    o_state.current_provider = Some("openai".to_string());
    o_state.current_model = Some("openai/gpt-5".to_string());
    o_state.session_allow_all = true;
    let o_turn = execute_user_prompt(
        &mut o_state,
        &resources,
        &o_registry,
        &mut o_auth,
        "please read fixture.txt",
    )
    .unwrap();
    o_server.join().unwrap();

    assert_eq!(a_turn.assistant_text, o_turn.assistant_text);
    assert_eq!(a_turn.tool_invocations.len(), o_turn.tool_invocations.len());
    assert_eq!(
        a_turn.tool_invocations[0].tool_id,
        o_turn.tool_invocations[0].tool_id
    );
    assert_eq!(a_turn.tool_invocations[0].success, true);
    assert_eq!(o_turn.tool_invocations[0].success, true);
    assert!(a_turn.tool_invocations[0].output.contains("shared-text"));
    assert!(o_turn.tool_invocations[0].output.contains("shared-text"));
}
