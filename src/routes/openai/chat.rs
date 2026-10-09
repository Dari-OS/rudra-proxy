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
        let mut user_prompt = String::new();
        for msg in &payload.messages {
            if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                if !user_prompt.is_empty() {
                    user_prompt.push_str("\n\n");
                }
                user_prompt.push_str(content);
            }
        }

        let questions = if let Some(q) = payload.extra.get("questions") {
            q.clone()
        } else {
            json!({
                "decision": {
                    "type": "noul",
                    "instructions": "Evaluate the state against the query"
                }
            })
        };

        let sys_payload = json!({
            "model": model_meta.id,
            "state": user_prompt,
            "questions": questions
        });

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

        let sys_result: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_else(|_| json!({}));
        let completion_id = generate_completion_id();
        let answer_obj = sys_result.get("answers").unwrap_or(&sys_result);
        let formatted_text = serde_json::to_string_pretty(answer_obj).unwrap_or_default();

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
        let protocol = model_meta.protocol;

        tokio::spawn(async move {
            let mut sent_terminal = false;
            let mut has_tool_calls = false;

            while let Some(event_res) = event_stream.next().await {
                match event_res {
                    Ok(event) => {
                        if event.data.is_empty() {
                            continue;
                        }
                        if event.data == "[DONE]" {
                            let finish = if has_tool_calls { Some("tool_calls") } else { Some("stop") };
                            let chunk = make_openai_terminal_chunk(&completion_id, &model_name, finish);
                            let _ = tx.send(Ok(chunk)).await;
                            sent_terminal = true;
                            break;
                        }

                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&event.data) {
                            match protocol {
                                ModelProtocol::Responses => {
                                    let event_type =
                                        v.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                    match event_type {
                                        "response.output_text.delta" => {
                                            if let Some(delta) =
                                                v.get("delta").and_then(|d| d.as_str())
                                            {
                                                let chunk = make_openai_chunk(
                                                    &completion_id,
                                                    &model_name,
                                                    Some(delta),
                                                    None,
                                                );
                                                if tx.send(Ok(chunk)).await.is_err() {
                                                    break;
                                                }
                                            }
                                        }
                                        "response.output_item.added" => {
                                            if let Some(item) = v.get("item")
                                                && item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
                                                    has_tool_calls = true;
                                                    let idx = v.get("output_index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                                    let call_id = item.get("call_id").and_then(|c| c.as_str());
                                                    let name = item.get("name").and_then(|n| n.as_str());
                                                    let chunk = make_openai_tool_chunk(
                                                        &completion_id,
                                                        &model_name,
                                                        idx,
                                                        call_id,
                                                        name,
                                                        Some(""),
                                                    );
                                                    if tx.send(Ok(chunk)).await.is_err() {
                                                        break;
                                                    }
                                                }
                                        }
                                        "response.function_call_arguments.delta" => {
                                            has_tool_calls = true;
                                            let idx = v.get("output_index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                            let delta = v.get("delta").and_then(|d| d.as_str()).unwrap_or("");
                                            let chunk = make_openai_tool_chunk(
                                                &completion_id,
                                                &model_name,
                                                idx,
                                                None,
                                                None,
                                                Some(delta),
                                            );
                                            if tx.send(Ok(chunk)).await.is_err() {
                                                break;
                                            }
                                        }
                                        "response.completed" => {
                                            let finish = if has_tool_calls { Some("tool_calls") } else { Some("stop") };
                                            let chunk = make_openai_terminal_chunk(
                                                &completion_id,
                                                &model_name,
                                                finish,
                                            );
                                            let _ = tx.send(Ok(chunk)).await;
                                            sent_terminal = true;
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                                ModelProtocol::ChatCompletions => {
                                    // Forward standard OpenAI chat chunks directly
                                    let chunk = format!("data: {}\n\n", event.data);
                                    if tx.send(Ok(chunk)).await.is_err() {
                                        break;
                                    }
                                }
                                ModelProtocol::SystemOne => {}
                            }
                        }
                    }
                    Err(_) => break,
                }
            }

            if !sent_terminal {
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
        let mut full_text = String::new();
        let mut full_reasoning = String::new();
        let mut collected_tool_calls: std::collections::BTreeMap<usize, (String, String, String, String)> =
            std::collections::BTreeMap::new();
        let mut reported_finish_reason: Option<String> = None;

        while let Some(event_res) = event_stream.next().await {
            if let Ok(event) = event_res {
                if event.data.is_empty() || event.data == "[DONE]" {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&event.data) {
                    match model_meta.protocol {
                        ModelProtocol::Responses => {
                            let event_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            match event_type {
                                "response.output_text.delta" => {
                                    if let Some(delta) = v.get("delta").and_then(|d| d.as_str()) {
                                        full_text.push_str(delta);
                                    }
                                }
                                "response.output_item.added" => {
                                    if let Some(item) = v.get("item")
                                        && item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
                                            let idx = v.get("output_index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                            let call_id = item.get("call_id").and_then(|c| c.as_str()).unwrap_or("").to_string();
                                            let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                                            let args = item.get("arguments").and_then(|a| a.as_str()).unwrap_or("").to_string();
                                            let entry = collected_tool_calls.entry(idx).or_insert((call_id.clone(), "function".to_string(), name.clone(), String::new()));
                                            if !call_id.is_empty() { entry.0 = call_id; }
                                            if !name.is_empty() { entry.2 = name; }
                                            if !args.is_empty() { entry.3.push_str(&args); }
                                        }
                                }
                                "response.function_call_arguments.delta" => {
                                    let idx = v.get("output_index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                    if let Some(delta) = v.get("delta").and_then(|d| d.as_str()) {
                                        let entry = collected_tool_calls.entry(idx).or_insert((String::new(), "function".to_string(), String::new(), String::new()));
                                        entry.3.push_str(delta);
                                    }
                                }
                                "response.completed" => {
                                    if let Some(resp_obj) = v.get("response")
                                        && let Some(outputs) = resp_obj.get("output").and_then(|o| o.as_array()) {
                                            for (idx, out_item) in outputs.iter().enumerate() {
                                                if out_item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
                                                    let call_id = out_item.get("call_id").and_then(|c| c.as_str()).unwrap_or("").to_string();
                                                    let name = out_item.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                                                    let args = out_item.get("arguments").and_then(|a| a.as_str()).unwrap_or("").to_string();
                                                    let entry = collected_tool_calls.entry(idx).or_insert((call_id.clone(), "function".to_string(), name.clone(), String::new()));
                                                    if !call_id.is_empty() { entry.0 = call_id; }
                                                    if !name.is_empty() { entry.2 = name; }
                                                    if entry.3.is_empty() && !args.is_empty() { entry.3 = args; }
                                                }
                                            }
                                        }
                                }
                                _ => {}
                            }
                        }
                        ModelProtocol::ChatCompletions => {
                            if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
                                for choice in choices {
                                    if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                                        reported_finish_reason = Some(fr.to_string());
                                    }
                                    if let Some(delta_obj) = choice.get("delta") {
                                        if let Some(content) =
                                            delta_obj.get("content").and_then(|c| c.as_str())
                                        {
                                            full_text.push_str(content);
                                        }
                                        if let Some(reasoning) = delta_obj
                                            .get("reasoning_content")
                                            .or_else(|| delta_obj.get("reasoning"))
                                            .and_then(|r| r.as_str())
                                        {
                                            full_reasoning.push_str(reasoning);
                                        }
                                        if let Some(tool_calls) = delta_obj.get("tool_calls").and_then(|t| t.as_array()) {
                                            for tc in tool_calls {
                                                let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                                let entry = collected_tool_calls.entry(idx).or_insert((String::new(), "function".to_string(), String::new(), String::new()));
                                                if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                                    entry.0 = id.to_string();
                                                }
                                                if let Some(c_type) = tc.get("type").and_then(|t| t.as_str()) {
                                                    entry.1 = c_type.to_string();
                                                }
                                                if let Some(func) = tc.get("function") {
                                                    if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                                        entry.2 = name.to_string();
                                                    }
                                                    if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                                        entry.3.push_str(args);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        ModelProtocol::SystemOne => {}
                    }
                }
            }
        }

        let reasoning_opt = if full_reasoning.is_empty() {
            None
        } else {
            Some(full_reasoning.as_str())
        };

        let tool_calls_vec = if collected_tool_calls.is_empty() {
            None
        } else {
            Some(
                collected_tool_calls
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
            &full_text,
            reasoning_opt,
            tool_calls_vec,
            reported_finish_reason.as_deref(),
        );

        Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-opencode-session", session_id.as_str())
            .body(Body::from(completion_json.to_string()))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "JSON error").into_response())
    }
}
