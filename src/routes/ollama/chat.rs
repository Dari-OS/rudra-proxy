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
use serde_json::{json, Value};
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
        "Dispatching Ollama chat completion"
    );

    // Bridge for System One models
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

        let sys_payload = json!({
            "model": model_meta.id,
            "state": user_prompt,
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

        let sys_result: Value = serde_json::from_slice(&body_bytes).unwrap_or_else(|_| json!({}));
        let answer_obj = sys_result.get("answers").unwrap_or(&sys_result);
        let formatted_text = serde_json::to_string_pretty(answer_obj).unwrap_or_default();

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
            .body(Body::from_stream(body_stream))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Stream error").into_response())
    } else {
        // Non-streaming mode
        let mut event_stream = response.bytes_stream().eventsource();
        let mut full_text = String::new();
        let mut collected_tool_calls: std::collections::BTreeMap<usize, (String, String, String, String)> =
            std::collections::BTreeMap::new();
        let mut finish_reason = "stop";

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
                                            finish_reason = "tool_calls";
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
                                    finish_reason = "tool_calls";
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
                                                    finish_reason = "tool_calls";
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
                                    if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str())
                                        && fr == "tool_calls" {
                                            finish_reason = "tool_calls";
                                        }
                                    if let Some(delta_obj) = choice.get("delta") {
                                        if let Some(content) =
                                            delta_obj.get("content").and_then(|c| c.as_str())
                                        {
                                            full_text.push_str(content);
                                        }
                                        if let Some(tool_calls) = delta_obj.get("tool_calls").and_then(|t| t.as_array()) {
                                            finish_reason = "tool_calls";
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

        let mut msg_obj = json!({
            "role": "assistant",
            "content": full_text
        });

        if !collected_tool_calls.is_empty() {
            let ollama_tool_calls: Vec<Value> = collected_tool_calls
                .into_values()
                .map(|(_id, _type, name, args)| {
                    let args_val = serde_json::from_str::<Value>(&args)
                        .unwrap_or_else(|_| json!({}));
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
            .body(Body::from(resp_json.to_string()))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "JSON error").into_response())
    }
}
