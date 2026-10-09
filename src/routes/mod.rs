pub mod health;
pub mod ollama;
pub mod openai;
pub mod systemone;

use crate::middleware::{auth_middleware, logging_middleware};
use crate::registry::ModelRegistry;
use crate::session::SessionManager;
use crate::upstream::UpstreamClient;
use axum::middleware;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
    pub client: reqwest::Client,
    pub registry: ModelRegistry,
    pub upstream: UpstreamClient,
    pub session_manager: SessionManager,
}

pub fn create_router(state: AppState, api_key: Option<String>) -> Router {
    create_router_with_cors(state, api_key, "*")
}

pub fn create_router_with_cors(
    state: AppState,
    api_key: Option<String>,
    cors_origin: &str,
) -> Router {
    let auth_layer = middleware::from_fn_with_state(api_key, auth_middleware);
    let log_layer = middleware::from_fn(logging_middleware);

    let cors_layer = if cors_origin == "*" || cors_origin.is_empty() {
        CorsLayer::permissive()
    } else if let Ok(val) = cors_origin.parse::<axum::http::HeaderValue>() {
        CorsLayer::new()
            .allow_origin(val)
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any)
    } else {
        CorsLayer::permissive()
    };

    Router::new()
        .route("/", get(health::health_check))
        .route("/health", get(health::health_check))
        // System One / TypeSafe AI evaluation endpoints (jev-*)
        .route("/v1/systemone", post(systemone::evaluate_systemone))
        .route("/v1/system-one", post(systemone::evaluate_systemone))
        .route("/api/systemone", post(systemone::evaluate_systemone))
        .route("/api/system-one", post(systemone::evaluate_systemone))
        .nest("/v1", openai::router())
        .nest("/api", ollama::router())
        .layer(auth_layer)
        .layer(log_layer)
        .layer(cors_layer)
        .with_state(state)
}
