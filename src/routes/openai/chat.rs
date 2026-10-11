use crate::registry::metadata::ModelProtocol;
use crate::routes::AppState;
use crate::upstream::payload::{
    build_opencode_payload, make_openai_chunk, make_openai_completion, make_openai_terminal_chunk,
    make_openai_tool_chunk, OpenAiChatRequest,
};
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use rand::Rng;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info};

pub fn generate_completion_id() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("chatcmpl-{hex}")
}

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<OpenAiChatRequest>,
) -> Response {
    // 1. Resolve session ID (using SessionManager with configurable rotation)
    let session_id = state
        .session_manager
        .get_or_rotate_session(headers.get("x-opencode-session").and_then(|v| v.to_str().ok()))
        .await;

    // 2. Resolve target model metadata
    let model_meta = state.registry.resolve_model(&payload.model).await;

    info!(
        requested_model = %payload.model,
        resolved_model = %model_meta.id,
        protocol = ?model_meta.protocol,
        stream = payload.stream,
        session_id = %session_id,
        "Dispatching OpenAI chat completion"
    );

    // Bridge for System One evaluation models (e.g. jev-*)
    if model_meta.protocol == ModelProtocol::SystemOne {
        let sys_adapter = crate::models::systemone::SystemOneAdapter;
        let sys_payload = sys_adapter.build_chat_bridge_payload(
            &model_meta.id,
            &payload.messages,
            payload.extra.get("questions"),
        );

        let response = match state.upstream.dispatch_systemone(&session_id, &sys_payload).await {
            Ok(res) => res,
            Err(err) => {
                error!("Failed to reach System One gateway: {err}");
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": {"message": err.to_string(), "type": "bad_gateway"}})),
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
                Json(json!({"error": {"message": err_text}})),
            )
                .into_response();
        }

        let formatted_text = sys_adapter
            .format_evaluation_response(&body_bytes)
            .unwrap_or_default();
        let completion_id = generate_completion_id();

        return if payload.stream {
            let chunk1 = make_openai_chunk(&completion_id, &payload.model, Some(&formatted_text), None);
            let chunk2 = make_openai_terminal_chunk(&completion_id, &payload.model, None);
            Response::builder()
                .header(header::CONTENT_TYPE, "text/event-stream")
                .header("x-opencode-session", session_id.as_str())
                .body(Body::from(format!("{chunk1}{chunk2}")))
                .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Stream error").into_response())
        } else {
            let completion_json = make_openai_completion(&completion_id, &payload.model, &formatted_text, None, None, None);
            Response::builder()
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-opencode-session", session_id.as_str())
                .body(Body::from(completion_json.to_string()))
                .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "JSON error").into_response())
        };
    }

    // 3. Build upstream payload with dummy tools and reasoning
    let override_effort = state.registry.get_override_effort(&model_meta.id);
    let upstream_payload = build_opencode_payload(&payload, &model_meta, override_effort.as_deref());

    // 4. Dispatch to OpenCode Zen gateway
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
                Json(json!({"error": {"message": err.to_string(), "type": "bad_gateway"}})),
            )
                .into_response();
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let err_text = response.text().await.unwrap_or_default();
        error!(status = %status, body = %err_text, "OpenCode Zen error response");
        return (
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({
                "error": {
                    "message": err_text,
                    "code": status.as_u16()
                }
            })),
        )
            .into_response();
    }

    let completion_id = generate_completion_id();
    let model_name = payload.model.clone();

    // 5. Streaming mode vs Non-streaming mode
    if payload.stream {
        let (tx, rx) = mpsc::channel::<Result<String, std::convert::Infallible>>(128);
        let mut event_stream = response.bytes_stream().eventsource();
        let adapter = state.adapters.get_adapter(&model_meta);

        tokio::spawn(async move {
            let mut sent_terminal = false;
            let mut has_tool_calls = false;
            let mut had_error = false;

            while let Some(event_res) = event_stream.next().await {
                match event_res {
                    Ok(event) => {
                        if event.data.is_empty() {
                            continue;
                        }
                        if event.data == "[DONE]" {
                            if adapter.protocol() == ModelProtocol::ChatCompletions {
                                let _ = tx.send(Ok("data: [DONE]\n\n".to_string())).await;
                            } else {
                                let finish = if has_tool_calls { Some("tool_calls") } else { Some("stop") };
                                let chunk = make_openai_terminal_chunk(&completion_id, &model_name, finish);
                                let _ = tx.send(Ok(chunk)).await;
                            }
                            sent_terminal = true;
                            break;
                        }

                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&event.data) {
                            if v.get("error").is_some() {
                                error!(error = %event.data, "Upstream stream returned error chunk");
                                had_error = true;
                            }

                            if adapter.protocol() == ModelProtocol::Responses {
                                let event_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                if let Some(delta) = adapter.parse_stream_event(event_type, &v) {
                                    if let Some(content) = delta.content {
                                        let chunk = make_openai_chunk(&completion_id, &model_name, Some(&content), None);
                                        if tx.send(Ok(chunk)).await.is_err() {
                                            break;
                                        }
                                    }
                                    for tc in delta.tool_calls {
                                        has_tool_calls = true;
                                        let chunk = make_openai_tool_chunk(
                                            &completion_id,
                                            &model_name,
                                            tc.index,
                                            tc.id.as_deref(),
                                            tc.name.as_deref(),
                                            tc.arguments_delta.as_deref(),
                                        );
                                        if tx.send(Ok(chunk)).await.is_err() {
                                            break;
                                        }
                                    }
                                    if let Some(finish) = delta.finish_reason {
                                        let real_finish = if has_tool_calls && finish == "stop" {
                                            "tool_calls"
                                        } else {
                                            finish.as_str()
                                        };
                                        let chunk = make_openai_terminal_chunk(&completion_id, &model_name, Some(real_finish));
                                        let _ = tx.send(Ok(chunk)).await;
                                        sent_terminal = true;
                                        break;
                                    }
                                }
                            } else {
                                // Forward standard OpenAI chat chunks directly
                                let chunk = format!("data: {}\n\n", event.data);
                                if tx.send(Ok(chunk)).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        error!(error = %err, "Upstream event stream disconnected with error");
                        had_error = true;
                        break;
                    }
                }
            }

            if !sent_terminal && !had_error && adapter.protocol() == ModelProtocol::Responses {
                let finish = if has_tool_calls { Some("tool_calls") } else { Some("stop") };
                let chunk = make_openai_terminal_chunk(&completion_id, &model_name, finish);
                let _ = tx.send(Ok(chunk)).await;
            }
        });

        let body_stream = ReceiverStream::new(rx);
        Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .header("x-opencode-session", session_id.as_str())
            .body(Body::from_stream(body_stream))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Stream error").into_response())
    } else {
        // NON-STREAMING: Aggregate upstream SSE stream into complete JSON response
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

        let reasoning_opt = if acc.reasoning.is_empty() {
            None
        } else {
            Some(acc.reasoning.as_str())
        };

        let tool_calls_vec = if acc.tool_calls.is_empty() {
            None
        } else {
            Some(
                acc.tool_calls
                    .into_iter()
                    .map(|(_idx, (id, c_type, name, arguments))| {
                        json!({
                            "id": id,
                            "type": c_type,
                            "function": {
                                "name": name,
                                "arguments": arguments
                            }
                        })
                    })
                    .collect(),
            )
        };

        let completion_json = make_openai_completion(
            &completion_id,
            &model_name,
            &acc.text,
            reasoning_opt,
            tool_calls_vec,
            acc.reported_finish_reason.as_deref(),
        );

        Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-opencode-session", session_id.as_str())
            .body(Body::from(completion_json.to_string()))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "JSON error").into_response())
    }
}
