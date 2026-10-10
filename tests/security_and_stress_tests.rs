use axum::body::Body;
use axum::http::{HeaderValue, Request, StatusCode};
use rudra_proxy::config::{AppConfig, CliArgs, ProxyConfig};
use rudra_proxy::middleware::auth::constant_time_eq;
use rudra_proxy::registry::metadata::{ModelMetadata, ModelProtocol, ReasoningType};
use rudra_proxy::registry::ModelRegistry;
use rudra_proxy::routes::{create_router, AppState};
use rudra_proxy::session::{SessionId, SessionManager};
use rudra_proxy::upstream::payload::{
    build_opencode_payload, convert_messages_to_responses_input, make_openai_terminal_chunk,
    make_openai_tool_chunk, OpenAiChatRequest,
};
use rudra_proxy::upstream::{ProxyPool, UpstreamClient};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

fn make_test_cli_args() -> CliArgs {
    CliArgs::default()
}

// ============================================================================
// 1. SECURITY & TIMING ATTACK RESISTANCE TESTS
// ============================================================================

#[test]
fn test_constant_time_eq_comprehensive() {
    // 1. Identical strings
    assert!(constant_time_eq("secret-token", "secret-token"));
    assert!(constant_time_eq("", ""));
    assert!(constant_time_eq("a", "a"));
    assert!(constant_time_eq("sk-proj-1234567890abcdef", "sk-proj-1234567890abcdef"));

    // 2. Different lengths (must fail without panic)
    assert!(!constant_time_eq("secret", "secret-longer"));
    assert!(!constant_time_eq("secret-longer", "secret"));
    assert!(!constant_time_eq("", "non-empty"));
    assert!(!constant_time_eq("non-empty", ""));

    // 3. Single bit / single character differences at various positions
    assert!(!constant_time_eq("Xecret-token", "secret-token")); // first char
    assert!(!constant_time_eq("secXet-token", "secret-token")); // middle char
    assert!(!constant_time_eq("secret-tokeX", "secret-token")); // last char

    // 4. Unicode & special characters
    assert!(constant_time_eq("🔑-token-räksmörgås", "🔑-token-räksmörgås"));
    assert!(!constant_time_eq("🔑-token-räksmörgås", "🔒-token-räksmörgås"));

    // 5. Very long tokens (e.g. 4096-char tokens)
    let long_a = "A".repeat(4096);
    let long_b = "A".repeat(4096);
    let mut long_c = "A".repeat(4095);
    long_c.push('B');
    assert!(constant_time_eq(&long_a, &long_b));
    assert!(!constant_time_eq(&long_a, &long_c));
}

#[tokio::test]
async fn test_auth_middleware_fuzzing_and_bypass_resistance() {
    let client = reqwest::Client::new();
    let config = AppConfig::load(make_test_cli_args());
    let registry = ModelRegistry::new(client.clone(), &config);
    let proxy_pool = ProxyPool::new(&config.proxy, client.clone());
    let upstream = UpstreamClient::new(proxy_pool, "public".to_string());
    let session_manager = SessionManager::new(10);

    let state = AppState {
        client,
        registry,
        upstream,
        session_manager,
    };

    let app = create_router(state, Some("super-secret-key-123".to_string()));

    // 1. Public paths must be allowed without auth
    for public_path in &["/", "/health", "/docs", "/openapi.json"] {
        let req = Request::builder()
            .uri(*public_path)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::OK,
            "Path {public_path} should be publicly accessible"
        );
    }

    // 2. Protected paths without auth must return 401
    for protected_path in &["/v1/models", "/api/tags", "/v1/chat/completions"] {
        let req = Request::builder()
            .uri(*protected_path)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "Protected path {protected_path} must reject unauthenticated requests"
        );
    }

    // 3. Fuzzed / malicious authorization headers must be rejected
    let bad_auth_headers = vec![
        "Bearer ",
        "Bearer",
        "Bearer wrong-key",
        "Bearer super-secret-key-12",       // 1 char short
        "Bearer super-secret-key-1234",      // 1 char extra
        "Bearer super-secret-key-123\0null", // Null byte injection
        "Bearer super-secret-key-123\r\n",   // CRLF
        "Basic c3VwZXItc2VjcmV0LWtleS0xMjM=",
        "Token super-secret-key-123",
        "super-secret-key-123",
        "Bearer   ",
    ];

    for bad in bad_auth_headers {
        if let Ok(hv) = HeaderValue::from_str(bad) {
            let req = Request::builder()
                .uri("/v1/models")
                .header("Authorization", hv)
                .body(Body::empty())
                .unwrap();
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                res.status(),
                StatusCode::UNAUTHORIZED,
                "Bad auth header '{bad}' should be rejected"
            );
        }
    }

    // 4. Case-insensitive bearer prefix must be accepted
    let valid_headers = vec![
        ("Authorization", "Bearer super-secret-key-123"),
        ("Authorization", "bearer super-secret-key-123"),
        ("x-api-key", "super-secret-key-123"),
    ];

    for (header_name, header_val) in valid_headers {
        let req = Request::builder()
            .uri("/v1/models")
            .header(header_name, header_val)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::OK,
            "Valid auth header {header_name}: {header_val} should be accepted"
        );
    }
}

// ============================================================================
// 2. HIGH-CONCURRENCY STRESS TESTS
// ============================================================================

#[tokio::test]
async fn test_session_manager_high_concurrency_stress() {
    let rotate_after = 15;
    let manager = Arc::new(SessionManager::new(rotate_after));
    let num_tasks = 50;
    let requests_per_task = 30; // Total 1500 concurrent requests

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let m = Arc::clone(&manager);
        handles.push(tokio::spawn(async move {
            let mut sessions = Vec::new();
            for _ in 0..requests_per_task {
                let s = m.get_or_rotate_session(None).await;
                // Every generated session must be 100% valid
                assert_eq!(s.as_str().len(), 30);
                assert!(s.as_str().starts_with("ses_"));
                assert!(SessionId::parse(s.as_str()).is_ok());
                sessions.push(s);
            }
            sessions
        }));
    }

    for h in handles {
        let res = h.await;
        assert!(res.is_ok(), "Task should not panic");
    }
}

#[tokio::test]
async fn test_proxy_pool_concurrency_and_rotation_stress() {
    let config = ProxyConfig {
        enabled: true,
        switch_after_requests: 10,
        proxies: vec![
            "http://127.0.0.1:8001".to_string(),
            "http://127.0.0.1:8002".to_string(),
            "http://127.0.0.1:8003".to_string(),
        ],
        list: vec![],
    };

    let pool = Arc::new(ProxyPool::new(&config, reqwest::Client::new()));
    assert!(pool.is_enabled());
    assert_eq!(pool.active_proxies_count(), 3);

    let num_threads = 40;
    let requests_per_thread = 50; // Total 2000 concurrent proxy requests

    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let p = Arc::clone(&pool);
        handles.push(tokio::spawn(async move {
            for _ in 0..requests_per_thread {
                let (_client, proxy_url) = p.get_client();
                assert!(proxy_url.is_some(), "Proxy URL must be returned when enabled");
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test]
async fn test_model_registry_concurrent_resolution_stress() {
    let client = reqwest::Client::new();
    let config = AppConfig::load(make_test_cli_args());
    let registry = Arc::new(ModelRegistry::new(client, &config));

    let num_tasks = 50;
    let queries_per_task = 50; // 2500 concurrent model lookups

    let mut handles = Vec::new();
    let models_to_query = vec![
        "gpt-4o",
        "muse-spark-1.3-contributor-free",
        "mimo-v2.6-flash-free:latest",
        "step-5-preview-free",
        "jev-1.13",
        "unknown-custom-model",
    ];

    for _ in 0..num_tasks {
        let reg = Arc::clone(&registry);
        let list = models_to_query.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..queries_per_task {
                for m in &list {
                    let resolved = reg.resolve_model(m).await;
                    assert!(!resolved.id.is_empty());
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

// ============================================================================
// 3. INPUT FUZZING, MALFORMED DATA & EDGE CASES
// ============================================================================

#[test]
fn test_payload_builder_fuzzed_and_extreme_parameters() {
    let meta = ModelMetadata {
        id: "muse-spark-1.3-contributor-free".to_string(),
        name: "Muse Spark 1.3".to_string(),
        description: "Test".to_string(),
        family: "muse".to_string(),
        protocol: ModelProtocol::Responses,
        reasoning: ReasoningType::Effort {
            values: vec!["minimal".into(), "low".into(), "medium".into(), "high".into(), "xhigh".into()],
        },
        context_limit: 128000,
        output_limit: 8192,
        is_free: true,
        created: 1700000000,
        owned_by: "OpenCode".to_string(),
    };

    // 1. Extreme tokens: negative, zero, or tiny tokens must be clamped to >= 16 for Responses
    let extreme_req = OpenAiChatRequest {
        model: "muse-spark-1.3-contributor-free".to_string(),
        messages: vec![json!({"role": "user", "content": "hi"})],
        max_tokens: Some(-500),
        temperature: Some(999999.9),
        top_p: Some(-10.0),
        reasoning_effort: Some("unsupported_super_ultra".to_string()),
        ..Default::default()
    };

    let payload = build_opencode_payload(&extreme_req, &meta, None);
    // Mandatory floor of 16 output tokens for upstream Responses API
    assert_eq!(payload["max_output_tokens"], 16);
    // Unsupported reasoning effort clamped to default "medium"
    assert_eq!(payload["reasoning"]["effort"], "medium");
    // Tools must contain bash and read
    assert!(payload["tools"].as_array().unwrap().len() >= 2);

    // 2. Chat completion protocol with empty messages
    let chat_meta = ModelMetadata {
        id: "mimo-v2.6-flash-free".to_string(),
        name: "Mimo".to_string(),
        description: "Test".to_string(),
        family: "mimo".to_string(),
        protocol: ModelProtocol::ChatCompletions,
        reasoning: ReasoningType::Interleaved,
        context_limit: 64000,
        output_limit: 8192,
        is_free: true,
        created: 1700000000,
        owned_by: "OpenCode".to_string(),
    };

    let empty_req = OpenAiChatRequest::default();
    let chat_payload = build_opencode_payload(&empty_req, &chat_meta, None);
    assert_eq!(chat_payload["stream"], true);
    assert!(chat_payload["tools"].is_array());
}

#[test]
fn test_message_conversion_responses_input_complex_structures() {
    // Test conversion of tool calls, tool results, multiline code, unicode
    let messages = vec![
        json!({
            "role": "user",
            "content": "Write a file with emojis 🚀 and newlines \n\r\t and backslashes \\"
        }),
        json!({
            "role": "assistant",
            "content": "Sure, invoking tool:",
            "tool_calls": [
                {
                    "id": "call_abc123",
                    "type": "function",
                    "function": {
                        "name": "write",
                        "arguments": "{\"path\": \"src/lib.rs\", \"content\": \"pub fn hello() {}\"}"
                    }
                }
            ]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_abc123",
            "content": "File written successfully: 24 bytes"
        }),
    ];

    let converted = convert_messages_to_responses_input(&messages);
    // User message
    assert_eq!(converted[0]["role"], "user");
    // Assistant message text
    assert_eq!(converted[1]["role"], "assistant");
    assert_eq!(converted[1]["content"], "Sure, invoking tool:");
    // Function call wire object
    assert_eq!(converted[2]["type"], "function_call");
    assert_eq!(converted[2]["call_id"], "call_abc123");
    assert_eq!(converted[2]["name"], "write");
    // Function call output wire object
    assert_eq!(converted[3]["type"], "function_call_output");
    assert_eq!(converted[3]["call_id"], "call_abc123");
    assert_eq!(converted[3]["output"], "File written successfully: 24 bytes");
}

#[test]
fn test_openai_tool_chunk_and_terminal_chunk_robustness() {
    // Verify tool chunk formats
    let tool_chunk = make_openai_tool_chunk(
        "cmpl-999",
        "mimo-v2.6-flash-free",
        0,
        Some("call_test_1"),
        Some("write"),
        Some("{\"path\": \"test.rs\"}"),
    );
    assert!(tool_chunk.starts_with("data: {"));
    assert!(tool_chunk.ends_with("\n\n"));
    let json_part = tool_chunk.strip_prefix("data: ").unwrap().trim_end();
    let parsed: Value = serde_json::from_str(json_part).unwrap();
    assert_eq!(parsed["choices"][0]["delta"]["tool_calls"][0]["id"], "call_test_1");
    assert_eq!(parsed["choices"][0]["delta"]["tool_calls"][0]["function"]["name"], "write");

    // Verify terminal chunk without finish reason defaults to "stop"
    let term = make_openai_terminal_chunk("cmpl-999", "mimo-v2.6-flash-free", None);
    assert!(term.contains("\"finish_reason\":\"stop\""));
    assert!(term.ends_with("\n\ndata: [DONE]\n\n"));
}
