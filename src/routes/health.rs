use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

pub async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "rudra-proxy",
        "version": env!("CARGO_PKG_VERSION"),
        "provider": "opencode-zen"
    }))
}
