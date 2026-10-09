use crate::config::{AppConfig, CliArgs, ListArgs, ListFormat};
use crate::registry::baseline::get_baseline_models;
use crate::registry::metadata::{ModelMetadata, ModelProtocol, ReasoningType};
use crate::registry::sync::{fetch_and_sync_models, OPENCODE_ZEN_MODELS_URL};
use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
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

    let (models, live_latency): (HashMap<String, ModelMetadata>, Option<u64>) = if args.live {
        let start = Instant::now();
        match fetch_and_sync_models(&client, &config.upstream_api_key).await {
            Ok(live_models) => {
                let duration = start.elapsed().as_millis() as u64;
                (live_models, Some(duration))
            }
            Err(e) => {
                eprintln!("[!] Live upstream sync failed: {e}. Falling back to baseline catalog.");
                (get_baseline_models(), None)
            }
        }
    } else {
        (get_baseline_models(), None)
    };

    let mut model_list: Vec<ModelStatusOutput> = models
        .into_values()
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
            let status_str = if live_latency.is_some() {
                "online".to_string()
            } else {
                "available".to_string()
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
                latency_ms: live_latency,
            }
        })
        .collect();

    model_list.sort_by(|a, b| a.id.cmp(&b.id));

    match args.format {
        ListFormat::Json => {
            let output = FullStatusOutput {
                updated_at: Utc::now().to_rfc3339(),
                status: if live_latency.is_some() || !args.live {
                    "operational".to_string()
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
