use crate::config::{AppConfig, BenchArgs, CliArgs};
use crate::registry::ModelRegistry;
use crate::session::SessionId;
use crate::upstream::client::UpstreamClient;
use crate::upstream::payload::{build_opencode_payload, OpenAiChatRequest};
use crate::upstream::proxy_pool::ProxyPool;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde_json::Value;
use std::time::Instant;

pub async fn execute(args: BenchArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Rudra Model Benchmark ===");
    println!("Target Model: {}", args.model);
    println!("Test Prompt: \"{}\"\n", args.prompt);

    let mut cli_copy = CliArgs {
        command: None,
        host: None,
        port: None,
        api_key: None,
        upstream_api_key: None,
        sync_interval_mins: None,
        config: args.config.clone(),
        proxy_enabled: None,
        proxies: None,
        proxy_switch_requests: None,
        session_rotate_requests: None,
        cors: None,
        workers: None,
        timeout: None,
        log_level: None,
        dry_run: false,
    };
    if let Some(c) = args.config {
        cli_copy.config = Some(c);
    }
    let config = AppConfig::load(cli_copy);

    let default_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .build()?;

    let proxy_pool = ProxyPool::new(&config.proxy, default_client.clone());
    let upstream_client = UpstreamClient::new(proxy_pool, config.upstream_api_key.clone());
    let registry = ModelRegistry::new(default_client, &config);

    let model_meta = registry.resolve_model(&args.model).await;
    let session_id = SessionId::generate();

    let chat_req = OpenAiChatRequest {
        model: model_meta.id.clone(),
        messages: vec![serde_json::json!({
            "role": "user",
            "content": args.prompt
        })],
        stream: true,
        temperature: Some(0.7),
        max_tokens: Some(512),
        ..Default::default()
    };

    let payload = build_opencode_payload(&chat_req, &model_meta, None);

    println!("Sending request to OpenCode Zen gateway...");
    let start_total = Instant::now();
    let response = upstream_client
        .dispatch(model_meta.protocol, &session_id, &payload)
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!("[!] Upstream error {status}: {body}");
        return Ok(());
    }

    let byte_stream = response.bytes_stream();
    let mut event_stream = byte_stream.eventsource();

    let mut ttft: Option<u128> = None;
    let mut token_chunks = 0usize;
    let mut generated_chars = 0usize;

    while let Some(event_res) = event_stream.next().await {
        match event_res {
            Ok(event) => {
                if event.data == "[DONE]" {
                    break;
                }
                if let Ok(json) = serde_json::from_str::<Value>(&event.data)
                    && let Some(choices) = json.get("choices").and_then(|c| c.as_array())
                        && let Some(first) = choices.first()
                            && let Some(delta) = first.get("delta")
                                && let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                    if ttft.is_none() {
                                        ttft = Some(start_total.elapsed().as_millis());
                                    }
                                    token_chunks += 1;
                                    generated_chars += content.len();
                                }
            }
            Err(_) => break,
        }
    }

    let total_elapsed = start_total.elapsed();
    let total_secs = total_elapsed.as_secs_f64();
    let ttft_ms = ttft.unwrap_or(0);
    let approx_tokens = token_chunks.max(generated_chars / 4);
    let generation_time_secs = (total_elapsed.as_millis().saturating_sub(ttft_ms) as f64) / 1000.0;
    let tps = if generation_time_secs > 0.0 {
        approx_tokens as f64 / generation_time_secs
    } else {
        0.0
    };

    println!("\n=== Benchmark Results ===");
    println!("Model:              {}", model_meta.id);
    println!("Protocol:           {:?}", model_meta.protocol);
    println!("Time-To-First-Token: {} ms", ttft_ms);
    println!("Total Duration:     {:.2} s", total_secs);
    println!("Estimated Tokens:   {}", approx_tokens);
    println!("Generation Speed:   {:.2} tokens/sec", tps);
    println!("Status:             Success");

    Ok(())
}
