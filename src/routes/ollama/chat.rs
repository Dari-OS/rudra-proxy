use crate::registry::metadata::ModelProtocol;
use crate::routes::AppState;
use crate::upstream::payload::{OpenAiChatRequest, build_opencode_payload};
use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info};

#[derive(Debug, Deserialize)]
pub struct OllamaChatRequest {
    pub model: String,
    pub messages: Vec<Value>,
    #[serde(default = "default_stream")]
    pub stream: Option<bool>,
    pub options: Option<OllamaOptions>,
    pub tools: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct OllamaOptions {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub num_predict: Option<i64>,
    pub stop: Option<Value>,
    pub presence_penalty: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub seed: Option<i64>,
}

fn default_stream() -> Option<bool> {
    Some(true)
}

/// POST /api/chat
pub async fn chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<OllamaChatRequest>,
) -> Response {
    let raw_session = headers
        .get("x-opencode-session-id")
        .or_else(|| headers.get("x-opencode-session"))
        .and_then(|v| v.to_str().ok());
    let session_id = state
        .session_manager
        .get_or_rotate_session(raw_session)
        .await;

    let model_meta = state.registry.resolve_model(&payload.model).await;
    let stream_mode = payload.stream.unwrap_or(true);

    info!(
        requested_model = %payload.model,
        resolved_model = %model_meta.id,
        stream = stream_mode,
        session_id = %session_id,
        "Dispatching Ollama chat completion"
    );

    // Bridge for System One models
    if model_meta.protocol == ModelProtocol::SystemOne {
        let sys_adapter = crate::models::systemone::SystemOneAdapter;
        let sys_payload =
            sys_adapter.build_chat_bridge_payload(&model_meta.id, &payload.messages, None);

        let response = match state
            .upstream
            .dispatch_systemone(&session_id, &sys_payload)
            .await
        {
            Ok(res) => res,
            Err(err) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": err.to_string()})),
                )
                    .into_response();
            }
        };

        let status = response.status();
        let body_bytes = response.bytes().await.unwrap_or_default();
        if !status.is_success() {
            let err_text = String::from_utf8_lossy(&body_bytes);
            return (
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(json!({"error": err_text})),
            )
                .into_response();
        }

        let formatted_text = sys_adapter
            .format_evaluation_response(&body_bytes)
            .unwrap_or_default();

        let resp_json = json!({
            "model": payload.model,
            "created_at": Utc::now().to_rfc3339(),
            "message": { "role": "assistant", "content": formatted_text },
            "done": true,
            "total_duration": 1200000000u64,
            "prompt_eval_count": 10,
            "eval_count": 20
        });

        return Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-opencode-session", session_id.as_str())
            .header("x-opencode-session-id", session_id.as_str())
            .body(Body::from(resp_json.to_string()))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "JSON error").into_response());
    }

    let opts = payload.options.unwrap_or_default();
    let openai_req = OpenAiChatRequest {
        model: model_meta.id.clone(),
        messages: payload.messages,
        stream: true,
        temperature: opts.temperature,
        top_p: opts.top_p,
        max_tokens: opts.num_predict,
        stop: opts.stop,
        presence_penalty: opts.presence_penalty,
        frequency_penalty: opts.frequency_penalty,
        seed: opts.seed,
        tools: payload.tools,
        ..Default::default()
    };

    let override_effort = state.registry.get_override_effort(&model_meta.id);
    let upstream_payload =
        build_opencode_payload(&openai_req, &model_meta, override_effort.as_deref());

    let response = match state
        .upstream
        .dispatch(model_meta.protocol, &session_id, &upstream_payload)
        .await
    {
        Ok(res) => res,
        Err(err) => {
            error!("Failed to reach OpenCode Zen gateway: {err}");
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": err.to_string()})),
            )
                .into_response();
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let err_text = response.text().await.unwrap_or_default();
        return (
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({"error": err_text})),
        )
            .into_response();
    }

    let model_tag = payload.model.clone();

    if stream_mode {
        // NDJSON Streaming mode
        let (tx, rx) = mpsc::channel::<Result<String, std::convert::Infallible>>(128);
        let mut event_stream = response.bytes_stream().eventsource();
        let adapter = state.adapters.get_adapter(&model_meta);

        tokio::spawn(async move {
            let mut sent_done = false;
            let mut had_error = false;

            while let Some(event_res) = event_stream.next().await {
                match event_res {
                    Ok(event) => {
                        if event.data.is_empty() || event.data == "[DONE]" {
                            if event.data == "[DONE]" {
                                let final_chunk = json!({
                                    "model": model_tag,
                                    "created_at": Utc::now().to_rfc3339(),
                                    "message": { "role": "assistant", "content": "" },
                                    "done": true,
                                    "total_duration": 1200000000u64,
                                    "prompt_eval_count": 10,
                                    "eval_count": 20
                                });
                                let _ = tx.send(Ok(format!("{final_chunk}\n"))).await;
                                sent_done = true;
                                break;
                            }
                            continue;
                        }

                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&event.data) {
                            if v.get("error").is_some() {
                                error!(error = %event.data, "Upstream stream returned error chunk in Ollama chat");
                                had_error = true;
                            }
                            let event_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            let delta_text = adapter
                                .parse_stream_event(event_type, &v)
                                .and_then(|d| d.content)
                                .unwrap_or_default();

                            if !delta_text.is_empty() {
                                let chunk = json!({
                                    "model": model_tag,
                                    "created_at": Utc::now().to_rfc3339(),
                                    "message": {
                                        "role": "assistant",
                                        "content": delta_text
                                    },
                                    "done": false
                                });
                                if tx.send(Ok(format!("{chunk}\n"))).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        error!(error = %err, "Upstream event stream disconnected with error in Ollama chat");
                        had_error = true;
                        break;
                    }
                }
            }

            if !sent_done && !had_error {
                let final_chunk = json!({
                    "model": model_tag,
                    "created_at": Utc::now().to_rfc3339(),
                    "message": { "role": "assistant", "content": "" },
                    "done": true,
                    "total_duration": 1200000000u64,
                    "prompt_eval_count": 10,
                    "eval_count": 20
                });
                let _ = tx.send(Ok(format!("{final_chunk}\n"))).await;
            }
        });

        let body_stream = ReceiverStream::new(rx);
        Response::builder()
            .header(header::CONTENT_TYPE, "application/x-ndjson")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .header("x-opencode-session", session_id.as_str())
            .header("x-opencode-session-id", session_id.as_str())
            .body(Body::from_stream(body_stream))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Stream error").into_response())
    } else {
        // Non-streaming mode
        let mut event_stream = response.bytes_stream().eventsource();
        let adapter = state.adapters.get_adapter(&model_meta);
        let mut acc = crate::models::NonStreamingAccumulator::default();

        while let Some(event_res) = event_stream.next().await {
            if let Ok(event) = event_res {
                if event.data.is_empty() || event.data == "[DONE]" {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&event.data) {
                    let event_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    adapter.accumulate_non_streaming(event_type, &v, &mut acc);
                }
            }
        }

        let mut msg_obj = json!({
            "role": "assistant",
            "content": acc.text
        });

        if !acc.tool_calls.is_empty() {
            let ollama_tool_calls: Vec<Value> = acc
                .tool_calls
                .into_values()
                .map(|(_id, _type, name, args)| {
                    let args_val =
                        serde_json::from_str::<Value>(&args).unwrap_or_else(|_| json!({}));
                    json!({
                        "function": {
                            "name": name,
                            "arguments": args_val
                        }
                    })
                })
                .collect();
            msg_obj["tool_calls"] = json!(ollama_tool_calls);
        }

        let finish_reason = acc.reported_finish_reason.as_deref().unwrap_or("stop");
        let resp_json = json!({
            "model": model_tag,
            "created_at": Utc::now().to_rfc3339(),
            "message": msg_obj,
            "done": true,
            "done_reason": finish_reason,
            "total_duration": 1200000000u64,
            "prompt_eval_count": 10,
            "eval_count": 20
        });

        Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-opencode-session", session_id.as_str())
            .header("x-opencode-session-id", session_id.as_str())
            .body(Body::from(resp_json.to_string()))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "JSON error").into_response())
    }
}
