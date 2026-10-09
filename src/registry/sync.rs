use crate::registry::baseline::get_baseline_models;
use crate::registry::metadata::{ModelMetadata, ModelProtocol, ReasoningType};
use serde::Deserialize;
use std::collections::HashMap;
use tracing::{info, warn};

pub const OPENCODE_ZEN_MODELS_URL: &str = "https://opencode.ai/zen/v1/models";
pub const OPENCODE_CATALOG_URL: &str = "https://models.opencode.ai/api.json";
pub const DEFAULT_USER_AGENT: &str = "opencode/1.18.35";

#[derive(Deserialize)]
struct ZenModelsResponse {
    #[serde(default)]
    data: Vec<ZenModelItem>,
}

#[derive(Deserialize)]
struct ZenModelItem {
    id: String,
    created: Option<u64>,
    owned_by: Option<String>,
}

#[derive(Deserialize)]
struct CatalogResponse {
    opencode: Option<CatalogProvider>,
}

#[derive(Deserialize)]
struct CatalogProvider {
    #[serde(default)]
    models: HashMap<String, CatalogModel>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct CatalogModel {
    id: Option<String>,
    name: Option<String>,
    description: Option<String>,
    family: Option<String>,
    reasoning: Option<bool>,
    reasoning_options: Option<Vec<CatalogReasoningOption>>,
    interleaved: Option<serde_json::Value>,
    limit: Option<CatalogLimit>,
    cost: Option<CatalogCost>,
}

#[derive(Deserialize)]
struct CatalogReasoningOption {
    #[serde(rename = "type")]
    option_type: String,
    values: Option<Vec<Option<String>>>,
}

#[derive(Deserialize)]
struct CatalogLimit {
    context: Option<u64>,
    output: Option<u64>,
}

#[derive(Deserialize)]
struct CatalogCost {
    input: Option<f64>,
    output: Option<f64>,
}

/// Fetches active models from OpenCode Zen and enriches metadata from models.opencode.ai
pub async fn fetch_and_sync_models(
    client: &reqwest::Client,
    upstream_api_key: &str,
) -> Result<HashMap<String, ModelMetadata>, String> {
    info!("Fetching active models from {OPENCODE_ZEN_MODELS_URL}...");

    let auth_header = format!("Bearer {upstream_api_key}");

    let zen_res = client
        .get(OPENCODE_ZEN_MODELS_URL)
        .header("User-Agent", DEFAULT_USER_AGENT)
        .header("Authorization", &auth_header)
        .send()
        .await
        .map_err(|e| format!("Failed to connect to Zen gateway: {e}"))?;

    if !zen_res.status().is_success() {
        let status = zen_res.status();
        let body = zen_res.text().await.unwrap_or_default();
        return Err(format!("Zen gateway returned status {status}: {body}"));
    }

    let zen_data: ZenModelsResponse = zen_res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Zen models response: {e}"))?;

    // Attempt to fetch models.opencode.ai catalog for metadata enrichment
    let catalog_map = match client
        .get(OPENCODE_CATALOG_URL)
        .header("User-Agent", DEFAULT_USER_AGENT)
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => {
            match res.json::<CatalogResponse>().await {
                Ok(parsed) => parsed.opencode.map(|p| p.models).unwrap_or_default(),
                Err(e) => {
                    warn!("Failed to parse models.opencode.ai catalog: {e}");
                    HashMap::new()
                }
            }
        }
        Ok(res) => {
            warn!("models.opencode.ai returned status {}", res.status());
            HashMap::new()
        }
        Err(e) => {
            warn!("Failed to fetch models.opencode.ai catalog: {e}");
            HashMap::new()
        }
    };

    let baseline = get_baseline_models();
    let is_keyless = upstream_api_key == "public" || upstream_api_key.is_empty();

    let mut result = HashMap::new();

    for item in zen_data.data {
        let id = item.id;

        // Check if model in catalog
        let cat_entry = catalog_map.get(&id);

        let is_cost_free = cat_entry
            .and_then(|c| c.cost.as_ref())
            .map(|c| c.input.unwrap_or(1.0) == 0.0 && c.output.unwrap_or(1.0) == 0.0)
            .unwrap_or(false);

        let is_name_free = id.ends_with("-free") || id.contains("contributor-free");

        // If keyless mode, only include free models; otherwise include all accessible models
        if is_keyless && !is_cost_free && !is_name_free {
            continue;
        }

        let family_hint = cat_entry
            .and_then(|c| c.family.as_deref())
            .or_else(|| baseline.get(&id).map(|b| b.family.as_str()));
        let desc_hint = cat_entry.and_then(|c| c.description.as_deref());
        let owner_hint = item.owned_by.as_deref();

        // Determine protocol with automatic detection for jev / systemone / responses
        let protocol = ModelProtocol::detect(&id, family_hint, desc_hint, owner_hint);

        // Determine reasoning configuration
        let reasoning = if let Some(cat) = cat_entry {
            if let Some(opts) = &cat.reasoning_options {
                let effort_opt = opts.iter().find(|o| o.option_type == "effort");
                if let Some(opt) = effort_opt {
                    let vals: Vec<String> = opt
                        .values
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .flatten()
                        .collect();
                    ReasoningType::Effort { values: vals }
                } else if opts.iter().any(|o| o.option_type == "toggle") {
                    ReasoningType::Toggle
                } else {
                    ReasoningType::None
                }
            } else if cat.interleaved.is_some() {
                ReasoningType::Interleaved
            } else if cat.reasoning.unwrap_or(false) {
                ReasoningType::Toggle
            } else {
                ReasoningType::None
            }
        } else if let Some(base) = baseline.get(&id) {
            base.reasoning.clone()
        } else if id.contains("mimo") || id.contains("nemotron") {
            ReasoningType::Interleaved
        } else {
            ReasoningType::None
        };

        let name = cat_entry
            .and_then(|c| c.name.clone())
            .unwrap_or_else(|| id.clone());

        let description = cat_entry
            .and_then(|c| c.description.clone())
            .unwrap_or_default();

        let family = cat_entry
            .and_then(|c| c.family.clone())
            .or_else(|| baseline.get(&id).map(|b| b.family.clone()))
            .unwrap_or_else(|| "opencode".to_string());

        let context_limit = cat_entry
            .and_then(|c| c.limit.as_ref())
            .and_then(|l| l.context)
            .or_else(|| baseline.get(&id).map(|b| b.context_limit))
            .unwrap_or(131072);

        let output_limit = cat_entry
            .and_then(|c| c.limit.as_ref())
            .and_then(|l| l.output)
            .or_else(|| baseline.get(&id).map(|b| b.output_limit))
            .unwrap_or(16384);

        let created = item.created.unwrap_or(1728424000);
        let owned_by = item.owned_by.unwrap_or_else(|| "opencode".to_string());

        result.insert(
            id.clone(),
            ModelMetadata {
                id,
                name,
                description,
                family,
                protocol,
                reasoning,
                context_limit,
                output_limit,
                is_free: is_cost_free || is_name_free,
                created,
                owned_by,
            },
        );
    }

    Ok(result)
}
