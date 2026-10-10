use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Constant-time comparison between two string slices to prevent timing side-channel attacks.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() != b_bytes.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a_bytes.iter().zip(b_bytes.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

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

    // Public health check and documentation routes
    let path = req.uri().path();
    if path == "/" || path == "/health" || path == "/docs" || path == "/openapi.json" {
        return next.run(req).await;
    }

    // Check Authorization: Bearer <key> (case-insensitive prefix)
    if let Some(auth_header) = req.headers().get("authorization")
        && let Ok(auth_str) = auth_header.to_str()
        && let Some(token) = auth_str
            .strip_prefix("Bearer ")
            .or_else(|| auth_str.strip_prefix("bearer "))
        && constant_time_eq(token.trim(), expected)
    {
        return next.run(req).await;
    }

    // Check x-api-key header
    if let Some(key_header) = req.headers().get("x-api-key")
        && let Ok(key_str) = key_header.to_str()
        && constant_time_eq(key_str.trim(), expected)
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
