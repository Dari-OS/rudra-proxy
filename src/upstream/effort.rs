use crate::registry::metadata::{ModelMetadata, ModelProtocol, ReasoningType};
use serde_json::{json, Value};

/// Maps and clamps reasoning effort for Responses API models (e.g. muse-*)
pub fn resolve_responses_reasoning(
    client_effort: Option<&str>,
    override_effort: Option<&str>,
) -> Value {
    const VALID_EFFORTS: &[&str] = &["minimal", "low", "medium", "high", "xhigh"];

    let requested = override_effort
        .or(client_effort)
        .unwrap_or("medium")
        .to_ascii_lowercase();

    let clamped = match requested.as_str() {
        "minimal" | "min" => "minimal",
        "low" => "low",
        "medium" | "med" => "medium",
        "high" => "high",
        "xhigh" | "max" => "xhigh",
        _ => {
            if VALID_EFFORTS.contains(&requested.as_str()) {
                &requested
            } else {
                "medium"
            }
        }
    };

    json!({ "effort": clamped })
}

/// Resolves reasoning_effort parameter for ChatCompletions models.
/// Returns Some(effort_string) only if the model supports effort levels.
/// If the model uses interleaved reasoning, toggle, or doesn't support reasoning, returns None.
pub fn resolve_chat_reasoning_effort(
    model: &ModelMetadata,
    client_effort: Option<&str>,
    override_effort: Option<&str>,
) -> Option<String> {
    match &model.reasoning {
        ReasoningType::Effort { values } => {
            if values.is_empty() {
                return None;
            }

            let requested = override_effort
                .or(client_effort)
                .unwrap_or("medium")
                .to_ascii_lowercase();

            // Direct match
            if let Some(matched) = values
                .iter()
                .find(|v| v.eq_ignore_ascii_case(&requested))
            {
                return Some(matched.clone());
            }

            // Approximate matching
            if requested.contains("low") || requested.contains("min") {
                if let Some(v) = values.first() {
                    return Some(v.clone());
                }
            } else if (requested.contains("high") || requested.contains("max"))
                && let Some(v) = values.last()
            {
                return Some(v.clone());
            }

            // Fallback to "medium" if present, else first available
            if let Some(med) = values.iter().find(|v| v.eq_ignore_ascii_case("medium")) {
                Some(med.clone())
            } else {
                values.first().cloned()
            }
        }
        ReasoningType::Toggle | ReasoningType::Interleaved | ReasoningType::None => {
            // Do NOT inject reasoning_effort into request body for models without effort support
            None
        }
    }
}

/// Applies appropriate reasoning configuration to the request JSON payload.
pub fn apply_reasoning(
    payload: &mut Value,
    model: &ModelMetadata,
    client_effort: Option<&str>,
    override_effort: Option<&str>,
) {
    match model.protocol {
        ModelProtocol::Responses => {
            let reasoning_obj = resolve_responses_reasoning(client_effort, override_effort);
            payload["reasoning"] = reasoning_obj;
        }
        ModelProtocol::ChatCompletions => {
            if let Some(effort) =
                resolve_chat_reasoning_effort(model, client_effort, override_effort)
            {
                payload["reasoning_effort"] = json!(effort);
            }
        }
        ModelProtocol::SystemOne => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_responses_reasoning_clamping() {
        assert_eq!(
            resolve_responses_reasoning(Some("min"), None),
            json!({"effort": "minimal"})
        );
        assert_eq!(
            resolve_responses_reasoning(Some("max"), None),
            json!({"effort": "xhigh"})
        );
        assert_eq!(
            resolve_responses_reasoning(Some("unknown"), None),
            json!({"effort": "medium"})
        );
    }
}
