use crate::config::{AppConfig, CliArgs, RunArgs};
use crate::registry::ModelRegistry;
use crate::session::SessionId;
use crate::upstream::client::UpstreamClient;
use crate::upstream::payload::{build_opencode_payload, OpenAiChatRequest};
use crate::upstream::proxy_pool::ProxyPool;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde_json::Value;
use std::io::{self, Write};
use std::time::Instant;

pub async fn execute(args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
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

    if let Some(ref prompt) = args.prompt {
        // One-shot execution
        let mut messages = Vec::new();
        if let Some(ref sys) = args.system {
            messages.push(serde_json::json!({
                "role": "system",
                "content": sys
            }));
        }
        messages.push(serde_json::json!({
            "role": "user",
            "content": prompt
        }));

        let override_effort = registry.get_override_effort(&model_meta.id);
        let effective_effort = args
            .reasoning_effort
            .as_deref()
            .or(override_effort.as_deref());

        send_and_stream(
            &upstream_client,
            &model_meta,
            &session_id,
            &messages,
            args.temperature,
            effective_effort,
        )
        .await?;
        println!();
        return Ok(());
    }

    let override_effort = registry.get_override_effort(&model_meta.id);
    let effective_effort = args
        .reasoning_effort
        .as_deref()
        .or(override_effort.as_deref());

    // Interactive REPL Mode
    println!("=== Rudra Terminal REPL ===");
    println!("Model: {} ({})", model_meta.id, model_meta.name);
    println!("Protocol: {:?}", model_meta.protocol);
    println!("Type 'exit' or 'quit' to exit, '/clear' to reset chat history.\n");

    let mut messages = Vec::new();
    if let Some(ref sys) = args.system {
        messages.push(serde_json::json!({
            "role": "system",
            "content": sys
        }));
    }

    let stdin = io::stdin();
    loop {
        print!("rudra> ");
        io::stdout().flush()?;

        let mut input = String::new();
        if stdin.read_line(&mut input)? == 0 {
            // EOF
            println!("\nGoodbye!");
            break;
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.eq_ignore_ascii_case("exit") || trimmed.eq_ignore_ascii_case("quit") {
            println!("Goodbye!");
            break;
        }
        if trimmed == "/clear" {
            messages.clear();
            if let Some(ref sys) = args.system {
                messages.push(serde_json::json!({
                    "role": "system",
                    "content": sys
                }));
            }
            println!("[Chat history cleared]\n");
            continue;
        }

        messages.push(serde_json::json!({
            "role": "user",
            "content": trimmed
        }));

        print!("\n");
        let assistant_reply = send_and_stream(
            &upstream_client,
            &model_meta,
            &session_id,
            &messages,
            args.temperature,
            effective_effort,
        )
        .await?;
        println!("\n");

        if !assistant_reply.is_empty() {
            messages.push(serde_json::json!({
                "role": "assistant",
                "content": assistant_reply
            }));
        }
    }

    Ok(())
}

async fn send_and_stream(
    upstream_client: &UpstreamClient,
    model_meta: &crate::registry::metadata::ModelMetadata,
    session_id: &SessionId,
    messages: &[Value],
    temperature: Option<f64>,
    reasoning_effort: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let start_total = Instant::now();

    if model_meta.protocol == crate::registry::metadata::ModelProtocol::SystemOne {
        let user_query = messages
            .last()
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("");

        let sys_payload = serde_json::json!({
            "model": model_meta.id,
            "state": user_query,
            "questions": {
                "decision": {
                    "type": "noul",
                    "instructions": "Evaluate the state and provide structured analysis"
                }
            }
        });

        let res = match upstream_client.dispatch_systemone(session_id, &sys_payload).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[!] Connection error to upstream System One gateway: {e}");
                return Ok(String::new());
            }
        };

        if res.status().is_success() {
            let body: serde_json::Value = res.json().await?;
            let formatted = serde_json::to_string_pretty(&body)?;
            print!("{formatted}");
            io::stdout().flush()?;
            let total_elapsed = start_total.elapsed();
            let total_secs = total_elapsed.as_secs_f64();
            let out_tokens = body
                .get("usage")
                .and_then(|u| u.get("output_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(20);
            let tps = if total_secs > 0.0 {
                out_tokens as f64 / total_secs
            } else {
                0.0
            };
            println!(
                "\n\x1b[90m[{out_tokens} tokens | {:.1} tok/s | {:.2}s]\x1b[0m",
                tps, total_secs
            );
            return Ok(formatted);
        } else {
            let status = res.status();
            let err = res.text().await.unwrap_or_default();
            eprintln!("[!] System One upstream error {status}: {err}");
            return Ok(String::new());
        }
    }

    let chat_req = OpenAiChatRequest {
        model: model_meta.id.clone(),
        messages: messages.to_vec(),
        stream: true,
        temperature,
        top_p: None,
        max_tokens: None,
        reasoning_effort: reasoning_effort.map(|s| s.to_string()),
        tools: None,
        tool_choice: None,
        extra: serde_json::Map::new(),
    };

    let payload = build_opencode_payload(&chat_req, model_meta, reasoning_effort);

    let response = match upstream_client
        .dispatch(model_meta.protocol, session_id, &payload)
        .await
    {
        Ok(res) => res,
        Err(e) => {
            eprintln!("[!] Connection error to upstream gateway: {e}");
            return Ok(String::new());
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!("[!] Upstream error {status}: {body}");
        return Ok(String::new());
    }

    let byte_stream = response.bytes_stream();
    let mut event_stream = byte_stream.eventsource();
    let mut full_response = String::new();
    let mut ttft: Option<u128> = None;
    let mut token_chunks = 0usize;
    let mut generated_chars = 0usize;

    while let Some(event_res) = event_stream.next().await {
        match event_res {
            Ok(event) => {
                if event.data == "[DONE]" {
                    break;
                }
                if let Ok(json) = serde_json::from_str::<Value>(&event.data) {
                    if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                        if let Some(first) = choices.first() {
                            if let Some(delta) = first.get("delta") {
                                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                    if ttft.is_none() {
                                        ttft = Some(start_total.elapsed().as_millis());
                                    }
                                    token_chunks += 1;
                                    generated_chars += content.len();
                                    print!("{content}");
                                    io::stdout().flush()?;
                                    full_response.push_str(content);
                                }
                            }
                        }
                    } else if json.get("type").and_then(|t| t.as_str()) == Some("response.output_text.delta") {
                        if let Some(delta) = json.get("delta").and_then(|d| d.as_str()) {
                            if ttft.is_none() {
                                ttft = Some(start_total.elapsed().as_millis());
                            }
                            token_chunks += 1;
                            generated_chars += delta.len();
                            print!("{delta}");
                            io::stdout().flush()?;
                            full_response.push_str(delta);
                        }
                    } else if let Some(delta) = json.get("delta").and_then(|d| d.as_str()) {
                        if ttft.is_none() {
                            ttft = Some(start_total.elapsed().as_millis());
                        }
                        token_chunks += 1;
                        generated_chars += delta.len();
                        print!("{delta}");
                        io::stdout().flush()?;
                        full_response.push_str(delta);
                    }
                }
            }
            Err(_) => break,
        }
    }

    let total_elapsed = start_total.elapsed();
    let ttft_ms = ttft.unwrap_or(0);
    let approx_tokens = token_chunks.max((generated_chars + 3) / 4);
    let gen_duration = (total_elapsed.as_millis().saturating_sub(ttft_ms) as f64) / 1000.0;
    let tps = if gen_duration > 0.02 {
        approx_tokens as f64 / gen_duration
    } else if total_elapsed.as_secs_f64() > 0.0 {
        approx_tokens as f64 / total_elapsed.as_secs_f64()
    } else {
        0.0
    };

    if approx_tokens > 0 {
        println!(
            "\n\x1b[90m[{approx_tokens} tokens | {:.1} tok/s | TTFT: {ttft_ms}ms | Total: {:.2}s]\x1b[0m",
            tps,
            total_elapsed.as_secs_f64()
        );
    }

    Ok(full_response)
}
