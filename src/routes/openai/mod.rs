pub mod chat;
pub mod models;

use crate::routes::AppState;
use axum::routing::{get, post};
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/models", get(models::list_models))
        .route("/models/{id}", get(models::get_model))
        .route("/models/sync", post(models::sync_models_endpoint))
        .route("/chat/completions", post(chat::chat_completions))
}
