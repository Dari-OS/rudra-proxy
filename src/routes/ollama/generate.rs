use crate::registry::metadata::ModelProtocol;
use crate::routes::AppState;
use crate::upstream::payload::{build_opencode_payload, OpenAiChatRequest};
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info};

#[derive(Debug, Deserialize)]
pub struct OllamaGenerateRequest {
    pub model: String,
    pub prompt: String,
    pub system: Option<String>,
    #[serde(default = "default_stream")]
    pub stream: Option<bool>,
    pub options: Option<super::chat::OllamaOptions>,
}

fn default_stream() -> Option<bool> {
    Some(true)
}

/// POST /api/generate
pub async fn generate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<OllamaGenerateRequest>,
) -> Response {
    let session_id = state
        .session_manager
        .get_or_rotate_session(headers.get("x-opencode-session").and_then(|v| v.to_str().ok()))
        .await;

    let model_meta = state.registry.resolve_model(&payload.model).await;
    let stream_mode = payload.stream.unwrap_or(true);

    info!(
        requested_model = %payload.model,
        resolved_model = %model_meta.id,
        stream = stream_mode,
        session_id = %session_id,
        "Dispatching Ollama generate completion"
    );

    // Bridge for System One models
    if model_meta.protocol == ModelProtocol::SystemOne {
        let sys_payload = json!({
            "model": model_meta.id,
            "state": payload.prompt,
            "questions": {
                "decision": {
                    "type": "noul",
                    "instructions": "Evaluate the state against the query"
                }
            }
        });

        let response = match state.upstream.dispatch_systemone(&session_id, &sys_payload).await {
            Ok(res) => res,
            Err(err) => return (StatusCode::BAD_GATEWAY, Json(json!({"error": err.to_string()}))).into_response(),
        };

        let status = response.status();
        let body_bytes = response.bytes().await.unwrap_or_default();
        if !status.is_success() {
            let err_text = String::from_utf8_lossy(&body_bytes);
            return (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), Json(json!({"error": err_text}))).into_response();
        }

        let sys_result: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_else(|_| json!({}));
        let answer_obj = sys_result.get("answers").unwrap_or(&sys_result);
        let formatted_text = serde_json::to_string_pretty(answer_obj).unwrap_or_default();

        let resp_json = json!({
            "model": payload.model,
            "created_at": Utc::now().to_rfc3339(),
            "response": formatted_text,
            "done": true,
            "total_duration": 1200000000u64,
        });

        return Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-opencode-session", session_id.as_str())
            .body(Body::from(resp_json.to_string()))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "JSON error").into_response());
    }

    let mut messages = Vec::new();
    if let Some(sys) = payload.system
        && !sys.is_empty()
    {
        messages.push(json!({ "role": "system", "content": sys }));
    }
    messages.push(json!({ "role": "user", "content": payload.prompt }));

    let opts = payload.options.unwrap_or_default();
    let openai_req = OpenAiChatRequest {
        model: model_meta.id.clone(),
        messages,
        stream: true,
        temperature: opts.temperature,
        top_p: opts.top_p,
        max_tokens: opts.num_predict,
        stop: opts.stop,
        presence_penalty: opts.presence_penalty,
        frequency_penalty: opts.frequency_penalty,
        seed: opts.seed,
        ..Default::default()
    };

    let override_effort = state.registry.get_override_effort(&model_meta.id);
    let upstream_payload = build_opencode_payload(&openai_req, &model_meta, override_effort.as_deref());

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
        let protocol = model_meta.protocol;

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
                                    "response": "",
                                    "done": true,
                                    "total_duration": 1200000000u64,
                                });
                                let _ = tx.send(Ok(format!("{final_chunk}\n"))).await;
                                sent_done = true;
                                break;
                            }
                            continue;
                        }

                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&event.data) {
                            if v.get("error").is_some() {
                                error!(error = %event.data, "Upstream stream returned error chunk in Ollama generate");
                                had_error = true;
                            }
                            let delta_text = match protocol {
                                ModelProtocol::Responses => {
                                    if v.get("type").and_then(|t| t.as_str())
                                        == Some("response.output_text.delta")
                                    {
                                        v.get("delta").and_then(|d| d.as_str()).unwrap_or("")
                                    } else {
                                        ""
                                    }
                                }
                                ModelProtocol::ChatCompletions => v
                                    .get("choices")
                                    .and_then(|c| c.as_array())
                                    .and_then(|arr| arr.first())
                                    .and_then(|first| first.get("delta"))
                                    .and_then(|d| d.get("content"))
                                    .and_then(|c| c.as_str())
                                    .unwrap_or(""),
                                ModelProtocol::SystemOne => "",
                            };

                            if !delta_text.is_empty() {
                                let chunk = json!({
                                    "model": model_tag,
                                    "created_at": Utc::now().to_rfc3339(),
                                    "response": delta_text,
                                    "done": false
                                });
                                if tx.send(Ok(format!("{chunk}\n"))).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        error!(error = %err, "Upstream event stream disconnected with error in Ollama generate");
                        had_error = true;
                        break;
                    }
                }
            }

            if !sent_done && !had_error {
                let final_chunk = json!({
                    "model": model_tag,
                    "created_at": Utc::now().to_rfc3339(),
                    "response": "",
                    "done": true,
                    "total_duration": 1200000000u64,
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
            .body(Body::from_stream(body_stream))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Stream error").into_response())
    } else {
        // Non-streaming mode
        let mut event_stream = response.bytes_stream().eventsource();
        let mut full_text = String::new();

        while let Some(event_res) = event_stream.next().await {
            if let Ok(event) = event_res {
                if event.data.is_empty() || event.data == "[DONE]" {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&event.data) {
                    match model_meta.protocol {
                        ModelProtocol::Responses => {
                            if let Some(delta) = v.get("delta").and_then(|d| d.as_str()) {
                                full_text.push_str(delta);
                            }
                        }
                        ModelProtocol::ChatCompletions => {
                            if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
                                for choice in choices {
                                    if let Some(content) = choice
                                        .get("delta")
                                        .and_then(|d| d.get("content"))
                                        .and_then(|c| c.as_str())
                                    {
                                        full_text.push_str(content);
                                    }
                                }
                            }
                        }
                        ModelProtocol::SystemOne => {}
                    }
                }
            }
        }

        let resp_json = json!({
            "model": model_tag,
            "created_at": Utc::now().to_rfc3339(),
            "response": full_text,
            "done": true,
            "total_duration": 1200000000u64,
        });

        Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-opencode-session", session_id.as_str())
            .body(Body::from(resp_json.to_string()))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "JSON error").into_response())
    }
}
