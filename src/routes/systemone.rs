use crate::routes::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{error, info};

#[derive(Debug, Deserialize, Serialize)]
pub struct SystemOneRequest {
    pub model: Option<String>,
    pub state: Value,
    pub questions: Value,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// POST /v1/systemone, /v1/system-one, /api/systemone
pub async fn evaluate_systemone(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SystemOneRequest>,
) -> Response {
    let session_id = state
        .session_manager
        .get_or_rotate_session(headers.get("x-opencode-session").and_then(|v| v.to_str().ok()))
        .await;

    let requested_model = payload
        .model
        .clone()
        .unwrap_or_else(|| "jev-1.13-free".to_string());

    let model_meta = state.registry.resolve_model(&requested_model).await;

    info!(
        requested_model = %requested_model,
        resolved_model = %model_meta.id,
        session_id = %session_id,
        "Dispatching System One evaluation"
    );

    let mut upstream_payload = json!({
        "model": model_meta.id,
        "state": payload.state,
        "questions": payload.questions,
    });

    for (k, v) in payload.extra {
        if k != "model" && k != "state" && k != "questions" {
            upstream_payload[k] = v;
        }
    }

    let response = match state
        .upstream
        .dispatch_systemone(&session_id, &upstream_payload)
        .await
    {
        Ok(res) => res,
        Err(err) => {
            error!("Failed to reach OpenCode Zen systemone gateway: {err}");
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
        error!(status = %status, body = %err_text, "OpenCode Zen systemone error");
        return (
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({"error": err_text})),
        )
            .into_response();
    }

    let result_json: Value = serde_json::from_slice(&body_bytes).unwrap_or_else(|_| {
        json!({"raw": String::from_utf8_lossy(&body_bytes)})
    });

    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-opencode-session", session_id.as_str())
        .status(StatusCode::OK)
        .body(axum::body::Body::from(result_json.to_string()))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Response build error").into_response())
}
