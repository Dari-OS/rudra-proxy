use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Validates downstream client API key if configured.
pub async fn auth_middleware(
    State(expected_key): State<Option<String>>,
    req: Request,
    next: Next,
) -> Response {
    // If no downstream API key is configured, allow all requests
    let expected = match expected_key {
        Some(ref k) if !k.is_empty() => k,
        _ => return next.run(req).await,
    };

    // Public health check routes
    let path = req.uri().path();
    if path == "/" || path == "/health" {
        return next.run(req).await;
    }

    // Check Authorization: Bearer <key>
    if let Some(auth_header) = req.headers().get("authorization")
        && let Ok(auth_str) = auth_header.to_str()
        && let Some(token) = auth_str.strip_prefix("Bearer ")
        && token.trim() == expected
    {
        return next.run(req).await;
    }

    // Check x-api-key header
    if let Some(key_header) = req.headers().get("x-api-key")
        && let Ok(key_str) = key_header.to_str()
        && key_str.trim() == expected
    {
        return next.run(req).await;
    }

    // Unauthorized
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": {
                "message": "Unauthorized: Invalid or missing API key",
                "type": "invalid_request_error",
                "code": 401
            }
        })),
    )
        .into_response()
}
