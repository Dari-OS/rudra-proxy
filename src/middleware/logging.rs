use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::time::Instant;
use tracing::info;

/// Request logging middleware recording method, path, status, and duration
pub async fn logging_middleware(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();

    let response = next.run(req).await;

    let latency = start.elapsed();
    let status = response.status();

    info!(
        method = %method,
        uri = %uri,
        status = %status.as_u16(),
        latency_ms = %latency.as_millis(),
        "Request processed"
    );

    response
}
