pub mod baseline;
pub mod metadata;
pub mod sync;

use crate::config::{AppConfig, ModelOverrideConfig};
use crate::registry::baseline::get_baseline_models;
use crate::registry::metadata::{ModelMetadata, ModelProtocol, ReasoningType};
use crate::registry::sync::fetch_and_sync_models;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info};

#[derive(Clone)]
pub struct ModelRegistry {
    models: Arc<RwLock<HashMap<String, ModelMetadata>>>,
    aliases: Arc<RwLock<HashMap<String, String>>>,
    overrides: HashMap<String, ModelOverrideConfig>,
    client: reqwest::Client,
    upstream_api_key: String,
}

impl ModelRegistry {
    pub fn new(client: reqwest::Client, config: &AppConfig) -> Self {
        let baseline = get_baseline_models();

        Self {
            models: Arc::new(RwLock::new(baseline)),
            aliases: Arc::new(RwLock::new(config.aliases.clone())),
            overrides: config.overrides.clone(),
            client,
            upstream_api_key: config.upstream_api_key.clone(),
        }
    }

    /// Triggers an immediate model catalog sync with upstream OpenCode Zen.
    pub async fn sync(&self) -> Result<Vec<String>, String> {
        let updated = fetch_and_sync_models(&self.client, &self.upstream_api_key).await?;
        let keys: Vec<String> = updated.keys().cloned().collect();

        {
            let mut lock = self.models.write().await;
            *lock = updated;
        }

        info!(
            count = keys.len(),
            "Successfully synced model catalog from OpenCode Zen"
        );
        Ok(keys)
    }

    /// Resolves the requested model ID (supporting aliases, :latest suffix, and user overrides)
    pub async fn resolve_model(&self, requested: &str) -> ModelMetadata {
        // Strip :latest if present (common in Ollama clients)
        let normalized = requested.strip_suffix(":latest").unwrap_or(requested);

        // Check if alias exists
        let target_id = {
            let aliases = self.aliases.read().await;
            aliases
                .get(normalized)
                .or_else(|| aliases.get(requested))
                .cloned()
                .unwrap_or_else(|| normalized.to_string())
        };

        // Look up target ID in registry
        let mut meta = {
            let lock = self.models.read().await;
            lock.get(&target_id)
                .cloned()
                .or_else(|| get_baseline_models().remove(&target_id))
                .unwrap_or_else(|| {
                    let protocol = ModelProtocol::detect(&target_id, None, None, None);

                    ModelMetadata {
                        id: target_id.clone(),
                        name: target_id.clone(),
                        description: String::new(),
                        family: "unknown".to_string(),
                        protocol,
                        reasoning: ReasoningType::None,
                        context_limit: 131072,
                        output_limit: 16384,
                        is_free: target_id.ends_with("-free")
                            || target_id.contains("contributor-free"),
                        created: 1728424000,
                        owned_by: "opencode".to_string(),
                    }
                })
        };

        // Apply any user configuration overrides
        if let Some(ov) = self.overrides.get(&meta.id)
            && let Some(ref proto) = ov.protocol
        {
            if proto.eq_ignore_ascii_case("responses") {
                meta.protocol = ModelProtocol::Responses;
            } else if proto.eq_ignore_ascii_case("systemone")
                || proto.eq_ignore_ascii_case("system_one")
            {
                meta.protocol = ModelProtocol::SystemOne;
            } else if proto.eq_ignore_ascii_case("chat_completions") {
                meta.protocol = ModelProtocol::ChatCompletions;
            }
        }

        meta
    }

    /// List all currently active models.
    pub async fn list_models(&self) -> Vec<ModelMetadata> {
        let lock = self.models.read().await;
        lock.values().cloned().collect()
    }

    /// Get current model aliases.
    pub async fn get_aliases(&self) -> HashMap<String, String> {
        self.aliases.read().await.clone()
    }

    /// Get reasoning effort override configured for a specific model ID.
    pub fn get_override_effort(&self, model_id: &str) -> Option<String> {
        self.overrides
            .get(model_id)
            .and_then(|ov| ov.reasoning_effort.clone())
    }

    /// Spawns a background Tokio task to refresh the model catalog every `interval`.
    pub fn start_background_sync(self: Arc<Self>, interval: Duration) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // Skip immediate tick because initial sync is performed at startup
            ticker.tick().await;

            loop {
                ticker.tick().await;
                info!("Running scheduled background sync of model catalog...");
                if let Err(e) = self.sync().await {
                    error!("Scheduled background sync failed: {e}");
                }
            }
        });
    }
}
