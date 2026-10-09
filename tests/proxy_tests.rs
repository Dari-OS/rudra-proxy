use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use rudra_proxy::config::{AppConfig, CliArgs, ProxyConfig, ProxyNodeConfig};
use rudra_proxy::registry::metadata::ModelProtocol;
use rudra_proxy::registry::ModelRegistry;
use rudra_proxy::routes::{create_router, AppState};
use rudra_proxy::session::SessionManager;
use rudra_proxy::upstream::payload::{
    build_opencode_payload, make_openai_completion, make_openai_terminal_chunk,
    make_openai_tool_chunk, OpenAiChatRequest,
};
use rudra_proxy::upstream::{ProxyPool, UpstreamClient};
use serde_json::{json, Value};
use tower::ServiceExt;

fn make_test_cli_args() -> CliArgs {
    CliArgs::default()
}

#[tokio::test]
async fn test_model_resolution_and_aliases() {
    let client = reqwest::Client::new();
    let config = AppConfig::load(make_test_cli_args());
    let registry = ModelRegistry::new(client, &config);

    // Baseline free model
    let muse = registry.resolve_model("muse-spark-1.3-contributor-free").await;
    assert_eq!(muse.protocol, ModelProtocol::Responses);

    // Stripping :latest
    let muse_latest = registry.resolve_model("muse-spark-1.3-contributor-free:latest").await;
    assert_eq!(muse_latest.id, "muse-spark-1.3-contributor-free");

    // Default alias gpt-4o -> muse-spark-1.3-contributor-free
    let gpt4o = registry.resolve_model("gpt-4o").await;
    assert_eq!(gpt4o.id, "muse-spark-1.3-contributor-free");
    assert_eq!(gpt4o.protocol, ModelProtocol::Responses);

    // Chat completion model
    let mimo = registry.resolve_model("mimo-v2.6-flash-free").await;
    assert_eq!(mimo.protocol, ModelProtocol::ChatCompletions);

    // SystemOne model
    let jev = registry.resolve_model("jev-1.13-free").await;
    assert_eq!(jev.protocol, ModelProtocol::SystemOne);
}

#[tokio::test]
async fn test_payload_builder_dummy_tools_and_streaming() {
    let client = reqwest::Client::new();
    let config = AppConfig::load(make_test_cli_args());
    let registry = ModelRegistry::new(client, &config);

    // 1. Responses API payload
    let muse_meta = registry.resolve_model("muse-spark-1.3-contributor-free").await;
    let req = OpenAiChatRequest {
        model: "muse-spark-1.3-contributor-free".to_string(),
        messages: vec![json!({"role": "user", "content": "hello"})],
        stream: false, // client requests false
        temperature: Some(0.7),
        max_completion_tokens: Some(8), // should be clamped to 16 for Responses API
        reasoning_effort: Some("high".to_string()),
        ..Default::default()
    };

    let payload = build_opencode_payload(&req, &muse_meta, None);
    // Mandatory stream: true
    assert_eq!(payload["stream"], true);
    // Wire format uses input
    assert!(payload.get("input").is_some());
    // Dummy tools injected
    let tools = payload["tools"].as_array().expect("tools array");
    assert!(tools.iter().any(|t| t["name"] == "bash"));
    assert!(tools.iter().any(|t| t["name"] == "read"));
    // Reasoning effort formatted as reasoning: { effort: "high" }
    assert_eq!(payload["reasoning"]["effort"], "high");
    // max_completion_tokens translated to max_output_tokens with floor of 16
    assert_eq!(payload["max_output_tokens"], 16);

    // 2. Chat Completions API payload with advanced parameters
    let mimo_meta = registry.resolve_model("mimo-v2.6-flash-free").await;
    let mimo_req = OpenAiChatRequest {
        model: "mimo-v2.6-flash-free".to_string(),
        messages: vec![json!({"role": "user", "content": "ping"})],
        stream: false,
        temperature: Some(0.3),
        top_p: Some(0.9),
        max_tokens: Some(150),
        stop: Some(json!(["STOP", "END"])),
        presence_penalty: Some(0.4),
        frequency_penalty: Some(0.6),
        seed: Some(12345),
        response_format: Some(json!({"type": "json_object"})),
        ..Default::default()
    };

    let mimo_payload = build_opencode_payload(&mimo_req, &mimo_meta, None);
    assert_eq!(mimo_payload["stream"], true);
    assert!(mimo_payload.get("messages").is_some());
    let mimo_tools = mimo_payload["tools"].as_array().expect("tools array");
    assert!(mimo_tools.iter().any(|t| t["function"]["name"] == "bash"));
    assert!(mimo_tools.iter().any(|t| t["function"]["name"] == "read"));
    // Parameters forwarded
    assert_eq!(mimo_payload["temperature"], 0.3);
    assert_eq!(mimo_payload["top_p"], 0.9);
    assert_eq!(mimo_payload["max_tokens"], 150);
    assert_eq!(mimo_payload["stop"], json!(["STOP", "END"]));
    assert_eq!(mimo_payload["presence_penalty"], 0.4);
    assert_eq!(mimo_payload["frequency_penalty"], 0.6);
    assert_eq!(mimo_payload["seed"], 12345);
    assert_eq!(mimo_payload["response_format"], json!({"type": "json_object"}));
    // mimo-v2.6-flash-free uses interleaved reasoning, no reasoning_effort parameter should be injected
    assert!(mimo_payload.get("reasoning_effort").is_none());
}

#[tokio::test]
async fn test_routes_health_and_version() {
    let client = reqwest::Client::new();
    let config = AppConfig::load(make_test_cli_args());
    let registry = ModelRegistry::new(client.clone(), &config);
    let proxy_pool = ProxyPool::new(&config.proxy, client.clone());
    let upstream = UpstreamClient::new(proxy_pool, "public".to_string());
    let session_manager = SessionManager::new(1);
    let state = AppState {
        client,
        registry,
        upstream,
        session_manager,
    };
    let app = create_router(state, None);

    // GET /health
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "rudra-proxy");

    // GET /api/version
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["version"], "0.1.32");

    // GET /api/tags
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tags")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["models"].as_array().unwrap().len() >= 12);

    // GET /docs (Scalar Documentation UI)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/docs")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("@scalar/api-reference"));

    // GET /openapi.json
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let spec: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(spec["openapi"], "3.1.0");
    assert!(spec["paths"]["/v1/chat/completions"].is_object());

    // GET /v1/models
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["object"], "list");
    assert!(json["data"].as_array().unwrap().len() >= 12);
}

#[tokio::test]
async fn test_auth_middleware() {
    let client = reqwest::Client::new();
    let mut args = make_test_cli_args();
    args.api_key = Some("secret123".to_string());
    let config = AppConfig::load(args);
    let registry = ModelRegistry::new(client.clone(), &config);
    let proxy_pool = ProxyPool::new(&config.proxy, client.clone());
    let upstream = UpstreamClient::new(proxy_pool, "public".to_string());
    let session_manager = SessionManager::new(1);
    let state = AppState {
        client,
        registry,
        upstream,
        session_manager,
    };
    let app = create_router(state, Some("secret123".to_string()));

    // /health should be accessible without auth
    let health_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health_res.status(), StatusCode::OK);

    // /v1/models without auth should be 401 Unauthorized
    let unauth_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth_res.status(), StatusCode::UNAUTHORIZED);

    // /v1/models with valid Bearer token should be 200 OK
    let auth_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .header("Authorization", "Bearer secret123")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(auth_res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_session_rotation() {
    let sm = SessionManager::new(2);

    let s1 = sm.get_or_rotate_session(None).await;
    let s2 = sm.get_or_rotate_session(None).await;
    // Same session ID for 2 requests
    assert_eq!(s1.as_str(), s2.as_str());

    // 3rd request triggers rotation
    let s3 = sm.get_or_rotate_session(None).await;
    assert_ne!(s1.as_str(), s3.as_str());
    assert!(s3.as_str().starts_with("ses_"));

    // Client provided valid header is respected
    let custom = "ses_ee2a1858bea8OeFGXjAMxiyjz3";
    let s_custom = sm.get_or_rotate_session(Some(custom)).await;
    assert_eq!(s_custom.as_str(), custom);
}

#[tokio::test]
async fn test_proxy_pool_rotation() {
    let default_client = reqwest::Client::new();
    let proxy_config = ProxyConfig {
        enabled: true,
        switch_after_requests: 2,
        proxies: vec![
            "http://127.0.0.1:8001".to_string(),
            "http://127.0.0.1:8002".to_string(),
        ],
        list: vec![
            ProxyNodeConfig {
                url: "http://127.0.0.1:8000".to_string(),
                requests: 1,
            },
        ],
    };

    let pool = ProxyPool::new(&proxy_config, default_client);
    assert!(pool.is_enabled());
    assert_eq!(pool.active_proxies_count(), 3);

    // Request 1: hits node 0 (requests = 1)
    let (_, p1) = pool.get_client();
    assert_eq!(p1.as_deref(), Some("http://127.0.0.1:8000"));

    // Request 2: rotated to node 1 (requests = 2)
    let (_, p2) = pool.get_client();
    assert_eq!(p2.as_deref(), Some("http://127.0.0.1:8001"));

    // Request 3: still node 1 (second request)
    let (_, p3) = pool.get_client();
    assert_eq!(p3.as_deref(), Some("http://127.0.0.1:8001"));

    // Request 4: rotated to node 2
    let (_, p4) = pool.get_client();
    assert_eq!(p4.as_deref(), Some("http://127.0.0.1:8002"));
}

#[tokio::test]
async fn test_reasoning_effort_clamping_and_chat_resolution() {
    let client = reqwest::Client::new();
    let config = AppConfig::load(make_test_cli_args());
    let registry = ModelRegistry::new(client, &config);

    // 1. Responses model (muse-*) clamps invalid/approximate strings
    let muse = registry.resolve_model("muse-spark-1.3-contributor-free").await;
    let req_min = OpenAiChatRequest {
        model: muse.id.clone(),
        messages: vec![json!({"role": "user", "content": "test"})],
        stream: true,
        reasoning_effort: Some("min".to_string()),
        ..Default::default()
    };
    let payload_min = build_opencode_payload(&req_min, &muse, None);
    assert_eq!(payload_min["reasoning"]["effort"], "minimal");

    let req_max = OpenAiChatRequest {
        reasoning_effort: Some("max".to_string()),
        ..req_min.clone()
    };
    let payload_max = build_opencode_payload(&req_max, &muse, None);
    assert_eq!(payload_max["reasoning"]["effort"], "xhigh");

    // 2. Chat model supporting effort (space-bunny-free)
    let bunny = registry.resolve_model("space-bunny-free").await;
    let req_bunny = OpenAiChatRequest {
        model: bunny.id.clone(),
        messages: vec![json!({"role": "user", "content": "test"})],
        stream: true,
        reasoning_effort: Some("high".to_string()),
        ..Default::default()
    };
    let payload_bunny = build_opencode_payload(&req_bunny, &bunny, None);
    assert_eq!(payload_bunny["reasoning_effort"], "high");

    // 3. Model override takes precedence over client reasoning effort
    let payload_override = build_opencode_payload(&req_bunny, &bunny, Some("low"));
    assert_eq!(payload_override["reasoning_effort"], "low");

    // 4. Interleaved reasoning models (mimo, nemotron) omit reasoning_effort parameter
    let mimo = registry.resolve_model("mimo-v2.6-flash-free").await;
    let req_mimo = OpenAiChatRequest {
        model: mimo.id.clone(),
        messages: vec![json!({"role": "user", "content": "test"})],
        stream: true,
        reasoning_effort: Some("high".to_string()),
        ..Default::default()
    };
    let payload_mimo = build_opencode_payload(&req_mimo, &mimo, None);
    assert!(payload_mimo.get("reasoning_effort").is_none());
}

#[tokio::test]
async fn test_default_and_jev_aliases() {
    let client = reqwest::Client::new();
    let config = AppConfig::load(make_test_cli_args());
    let registry = ModelRegistry::new(client, &config);

    // "default" resolves to mimo-v2.6-flash-free
    let def = registry.resolve_model("default").await;
    assert_eq!(def.id, "mimo-v2.6-flash-free");
    assert_eq!(def.protocol, ModelProtocol::ChatCompletions);

    // "jev" resolves to jev-1.13-free and SystemOne protocol
    let jev = registry.resolve_model("jev").await;
    assert_eq!(jev.id, "jev-1.13-free");
    assert_eq!(jev.protocol, ModelProtocol::SystemOne);
}

#[test]
fn test_config_cmd_overrides_and_aliases() {
    use rudra_proxy::cli::config_cmd;
    use rudra_proxy::config::{ConfigAction, ConfigArgs};
    use std::fs;

    let tmp_path = std::env::temp_dir().join(format!("rudra_test_{}.toml", rand::random::<u32>()));

    // 1. Set alias
    let set_alias = ConfigArgs {
        config: Some(tmp_path.clone()),
        action: ConfigAction::Set {
            key: "aliases.my-alias".to_string(),
            value: "space-bunny-free".to_string(),
        },
    };
    assert!(config_cmd::execute(set_alias).is_ok());

    // 2. Set reasoning effort override
    let set_effort = ConfigArgs {
        config: Some(tmp_path.clone()),
        action: ConfigAction::Set {
            key: "overrides.space-bunny-free.reasoning_effort".to_string(),
            value: "xhigh".to_string(),
        },
    };
    assert!(config_cmd::execute(set_effort).is_ok());

    // 3. Verify content
    let content = fs::read_to_string(&tmp_path).expect("read file");
    assert!(content.contains("my-alias"));
    assert!(content.contains("space-bunny-free"));
    assert!(content.contains("xhigh"));

    // Cleanup
    let _ = fs::remove_file(tmp_path);
}

#[tokio::test]
async fn test_tool_calls_payload_injection_and_conversion() {
    let client = reqwest::Client::new();
    let config = AppConfig::load(make_test_cli_args());
    let registry = ModelRegistry::new(client, &config);

    let muse_meta = registry.resolve_model("muse-spark-1.3-contributor-free").await;

    // Multi-turn conversation with assistant tool_calls and tool result
    let req = OpenAiChatRequest {
        model: "muse-spark-1.3-contributor-free".to_string(),
        messages: vec![
            json!({"role": "user", "content": "What is the weather?"}),
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"city\":\"Berlin\"}"
                        }
                    }
                ]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_123",
                "content": "{\"temp\": 20}"
            }),
        ],
        tools: Some(vec![json!({
            "type": "function",
            "name": "get_weather",
            "description": "Get weather",
            "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
        })]),
        tool_choice: Some(json!("auto")),
        ..Default::default()
    };

    let payload = build_opencode_payload(&req, &muse_meta, None);

    // Verify tools contain bash, read, AND get_weather
    let tools = payload["tools"].as_array().expect("tools array");
    assert!(tools.iter().any(|t| t["name"] == "bash"));
    assert!(tools.iter().any(|t| t["name"] == "read"));
    assert!(tools.iter().any(|t| t["name"] == "get_weather"));
    assert_eq!(payload["tool_choice"], "auto");

    // Verify Responses API input conversion
    let input = payload["input"].as_array().expect("input array");
    assert_eq!(input[0]["role"], "user");
    assert_eq!(input[1]["type"], "function_call");
    assert_eq!(input[1]["call_id"], "call_123");
    assert_eq!(input[1]["name"], "get_weather");
    assert_eq!(input[2]["type"], "function_call_output");
    assert_eq!(input[2]["call_id"], "call_123");
}

#[test]
fn test_make_openai_completion_with_tool_calls() {
    let tool_calls = vec![json!({
        "id": "call_abc123",
        "type": "function",
        "function": {
            "name": "calc",
            "arguments": "{\"expr\":\"2+2\"}"
        }
    })];

    let completion = make_openai_completion(
        "cmpl-1",
        "mimo-v2.6-flash-free",
        "",
        None,
        Some(tool_calls),
        None,
    );

    assert_eq!(completion["choices"][0]["finish_reason"], "tool_calls");
    assert_eq!(completion["choices"][0]["message"]["role"], "assistant");
    assert!(completion["choices"][0]["message"]["content"].is_null());
    assert_eq!(
        completion["choices"][0]["message"]["tool_calls"][0]["id"],
        "call_abc123"
    );
    assert_eq!(
        completion["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
        "calc"
    );
}

#[test]
fn test_make_openai_tool_chunk_format() {
    let chunk = make_openai_tool_chunk("cmpl-1", "mimo-v2.6-flash-free", 0, Some("call_999"), Some("bash"), Some("ls -la"));
    assert!(chunk.starts_with("data: "));
    assert!(chunk.contains("call_999"));
    assert!(chunk.contains("bash"));
    assert!(chunk.contains("ls -la"));

    let terminal = make_openai_terminal_chunk("cmpl-1", "mimo-v2.6-flash-free", Some("tool_calls"));
    assert!(terminal.contains("\"finish_reason\":\"tool_calls\""));
    assert!(terminal.ends_with("\n\ndata: [DONE]\n\n"));
    let events: Vec<&str> = terminal.split("\n\n").filter(|s| !s.is_empty()).collect();
    assert_eq!(events.len(), 2);
    assert!(events[0].starts_with("data: {"));
    assert_eq!(events[1], "data: [DONE]");
}

#[tokio::test]
async fn test_custom_harness_tools_preservation() {
    let client = reqwest::Client::new();
    let config = AppConfig::load(make_test_cli_args());
    let registry = ModelRegistry::new(client, &config);
    let mimo_meta = registry.resolve_model("mimo-v2.6-flash-free").await;

    // 1. Harness (like Claude Code / Hermes) supplies its own bash tool with custom schema
    let custom_bash = json!({
        "type": "function",
        "function": {
            "name": "bash",
            "description": "Execute arbitrary shell command in workspace",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeout": { "type": "integer" }
                },
                "required": ["command"]
            }
        }
    });

    let req_with_custom = OpenAiChatRequest {
        model: "mimo-v2.6-flash-free".to_string(),
        messages: vec![json!({"role": "user", "content": "list directory"})],
        tools: Some(vec![custom_bash]),
        ..Default::default()
    };

    let payload = build_opencode_payload(&req_with_custom, &mimo_meta, None);
    let tools = payload["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 2); // 1 custom bash + 1 injected dummy read

    let bash_tool = tools.iter().find(|t| t["function"]["name"] == "bash").expect("bash tool");
    assert_eq!(bash_tool["function"]["description"], "Execute arbitrary shell command in workspace");
    assert!(bash_tool["function"]["parameters"]["properties"].get("command").is_some());

    // 2. Client passes NO tools: proxy automatically injects dummy bash and read
    let req_no_tools = OpenAiChatRequest {
        model: "mimo-v2.6-flash-free".to_string(),
        messages: vec![json!({"role": "user", "content": "hello"})],
        tools: None,
        ..Default::default()
    };

    let payload_no_tools = build_opencode_payload(&req_no_tools, &mimo_meta, None);
    let auto_tools = payload_no_tools["tools"].as_array().expect("tools");
    assert_eq!(auto_tools.len(), 2);
    assert!(auto_tools.iter().any(|t| t["function"]["name"] == "bash"));
    assert!(auto_tools.iter().any(|t| t["function"]["name"] == "read"));
}


