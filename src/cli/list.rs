use crate::config::{AppConfig, CliArgs, ListArgs, ListFormat};
use crate::registry::baseline::get_baseline_models;
use crate::registry::metadata::{ModelMetadata, ModelProtocol, ReasoningType};
use crate::registry::sync::{fetch_and_sync_models, OPENCODE_ZEN_MODELS_URL};
use crate::session::SessionId;
use crate::upstream::client::UpstreamClient;
use crate::upstream::payload::{build_opencode_payload, OpenAiChatRequest};
use crate::upstream::proxy_pool::ProxyPool;
use chrono::Utc;
use futures::future::join_all;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

#[derive(Serialize)]
pub struct ModelStatusOutput {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub protocol: String,
    pub reasoning: String,
    pub context_limit: u64,
    pub output_limit: u64,
    pub status: String,
    pub latency_ms: Option<u64>,
}

#[derive(Serialize)]
pub struct FullStatusOutput {
    pub updated_at: String,
    pub status: String,
    pub gateway: String,
    pub models_count: usize,
    pub models: Vec<ModelStatusOutput>,
}

async fn probe_single_model(
    upstream_client: &UpstreamClient,
    model: &ModelMetadata,
) -> (String, Option<u64>) {
    let session_id = SessionId::generate();
    let start = Instant::now();

    if model.protocol == ModelProtocol::SystemOne {
        let payload = serde_json::json!({
            "model": model.id,
            "state": "ping",
            "questions": {
                "decision": {
                    "type": "noul",
                    "instructions": "ping"
                }
            }
        });
        match upstream_client.dispatch_systemone(&session_id, &payload).await {
            Ok(res) if res.status().is_success() => {
                let latency = start.elapsed().as_millis() as u64;
                ("online".to_string(), Some(latency))
            }
            Ok(res) => {
                let status_code = res.status();
                (format!("error ({status_code})"), None)
            }
            Err(_) => ("offline".to_string(), None),
        }
    } else {
        let chat_req = OpenAiChatRequest {
            model: model.id.clone(),
            messages: vec![serde_json::json!({
                "role": "user",
                "content": "ping"
            })],
            stream: true,
            temperature: Some(0.1),
            top_p: None,
            max_tokens: None,
            reasoning_effort: None,
            tools: None,
            tool_choice: None,
            extra: serde_json::Map::new(),
        };
        let payload = build_opencode_payload(&chat_req, model, None);
        match upstream_client.dispatch(model.protocol, &session_id, &payload).await {
            Ok(res) if res.status().is_success() => {
                let latency = start.elapsed().as_millis() as u64;
                ("online".to_string(), Some(latency))
            }
            Ok(res) => {
                let status_code = res.status();
                (format!("error ({status_code})"), None)
            }
            Err(_) => ("offline".to_string(), None),
        }
    }
}

pub async fn execute(args: ListArgs) -> Result<(), Box<dyn std::error::Error>> {
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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let (models, catalog_synced): (HashMap<String, ModelMetadata>, bool) = if args.live {
        match fetch_and_sync_models(&client, &config.upstream_api_key).await {
            Ok(live_models) => (live_models, true),
            Err(e) => {
                eprintln!("[!] Live upstream sync failed: {e}. Falling back to baseline catalog.");
                (get_baseline_models(), false)
            }
        }
    } else {
        (get_baseline_models(), false)
    };

    let proxy_pool = ProxyPool::new(&config.proxy, client.clone());
    let upstream_client = Arc::new(UpstreamClient::new(proxy_pool, config.upstream_api_key.clone()));

    let model_values: Vec<ModelMetadata> = models.into_values().collect();
    let mut probe_results: HashMap<String, (String, Option<u64>)> = HashMap::new();

    if args.live {
        let probe_futures = model_values.iter().map(|m| {
            let client = Arc::clone(&upstream_client);
            let model = m.clone();
            async move {
                let res = probe_single_model(&client, &model).await;
                (model.id, res)
            }
        });
        let results = join_all(probe_futures).await;
        for (id, res) in results {
            probe_results.insert(id, res);
        }
    }

    let mut model_list: Vec<ModelStatusOutput> = model_values
        .into_iter()
        .map(|m| {
            let protocol_str = match m.protocol {
                ModelProtocol::Responses => "responses",
                ModelProtocol::ChatCompletions => "chat_completions",
                ModelProtocol::SystemOne => "systemone",
            };
            let reasoning_str = match m.reasoning {
                ReasoningType::Effort { values } => format!("effort ({})", values.len()),
                ReasoningType::Toggle => "toggle".to_string(),
                ReasoningType::Interleaved => "interleaved".to_string(),
                ReasoningType::None => "none".to_string(),
            };
            let (status_str, latency) = if args.live {
                probe_results
                    .get(&m.id)
                    .cloned()
                    .unwrap_or_else(|| ("unknown".to_string(), None))
            } else {
                ("available".to_string(), None)
            };

            ModelStatusOutput {
                id: m.id,
                name: m.name,
                provider: m.family,
                protocol: protocol_str.to_string(),
                reasoning: reasoning_str,
                context_limit: m.context_limit,
                output_limit: m.output_limit,
                status: status_str,
                latency_ms: latency,
            }
        })
        .collect();

    model_list.sort_by(|a, b| a.id.cmp(&b.id));

    match args.format {
        ListFormat::Json => {
            let any_online = model_list.iter().any(|m| m.status == "online");
            let output = FullStatusOutput {
                updated_at: Utc::now().to_rfc3339(),
                status: if any_online || (!args.live && catalog_synced) {
                    "operational".to_string()
                } else if !args.live {
                    "available".to_string()
                } else {
                    "degraded".to_string()
                },
                gateway: OPENCODE_ZEN_MODELS_URL.to_string(),
                models_count: model_list.len(),
                models: model_list,
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        ListFormat::Markdown => {
            println!("# Rudra Active Models");
            println!();
            println!(
                "| Model ID | Provider | Protocol | Reasoning | Context | Status | Latency |"
            );
            println!(
                "|---|---|---|---|---|---|---|"
            );
            for m in &model_list {
                let latency_display = m
                    .latency_ms
                    .map(|l| format!("{l}ms"))
                    .unwrap_or_else(|| "-".to_string());
                println!(
                    "| `{}` | {} | {} | {} | {}k | {} | {} |",
                    m.id,
                    m.provider,
                    m.protocol,
                    m.reasoning,
                    m.context_limit / 1000,
                    m.status,
                    latency_display
                );
            }
        }
        ListFormat::Table => {
            println!(
                "{:<34} {:<12} {:<18} {:<14} {:<9} {:<9} {:<8}",
                "MODEL ID", "PROVIDER", "PROTOCOL", "REASONING", "CONTEXT", "STATUS", "LATENCY"
            );
            println!("{}", "-".repeat(110));
            for m in &model_list {
                let latency_display = m
                    .latency_ms
                    .map(|l| format!("{l}ms"))
                    .unwrap_or_else(|| "-".to_string());
                println!(
                    "{:<34} {:<12} {:<18} {:<14} {:<9} {:<9} {:<8}",
                    m.id,
                    m.provider,
                    m.protocol,
                    m.reasoning,
                    format!("{}k", m.context_limit / 1000),
                    m.status,
                    latency_display
                );
            }
            println!("{}", "-".repeat(110));
            println!("Total: {} models available", model_list.len());
        }
    }

    Ok(())
}
