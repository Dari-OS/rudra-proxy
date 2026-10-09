pub mod chat;
pub mod generate;
pub mod tags;

use crate::routes::AppState;
use axum::routing::{get, post};
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tags", get(tags::list_tags))
        .route("/version", get(tags::version))
        .route("/show", post(tags::show_model))
        .route("/chat", post(chat::chat))
        .route("/generate", post(generate::generate))
}
