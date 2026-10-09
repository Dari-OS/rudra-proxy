use crate::routes::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// GET /v1/models
pub async fn list_models(State(state): State<AppState>) -> impl IntoResponse {
    let models = state.registry.list_models().await;
    let aliases = state.registry.get_aliases().await;

    let mut data: Vec<_> = models
        .into_iter()
        .map(|m| {
            json!({
                "id": m.id,
                "object": "model",
                "created": m.created,
                "owned_by": m.owned_by
            })
        })
        .collect();

    // Include aliases in the list for client discovery
    for (alias, target) in aliases {
        data.push(json!({
            "id": alias,
            "object": "model",
            "created": 1728424000,
            "owned_by": format!("alias -> {target}")
        }));
    }

    Json(json!({
        "object": "list",
        "data": data
    }))
}

/// GET /v1/models/{id}
pub async fn get_model(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Response {
    let meta = state.registry.resolve_model(&model_id).await;
    (
        StatusCode::OK,
        Json(json!({
            "id": meta.id,
            "object": "model",
            "created": meta.created,
            "owned_by": meta.owned_by
        })),
    )
        .into_response()
}

/// POST /v1/models/sync
pub async fn sync_models_endpoint(State(state): State<AppState>) -> impl IntoResponse {
    match state.registry.sync().await {
        Ok(models) => (
            StatusCode::OK,
            Json(json!({
                "status": "synced",
                "count": models.len(),
                "models": models
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "status": "error",
                "message": err
            })),
        ),
    }
}
