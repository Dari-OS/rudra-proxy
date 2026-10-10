use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProtocol {
    /// OpenAI Responses API (e.g. muse-* models -> /zen/v1/responses)
    Responses,
    /// OpenAI Chat Completions API (e.g. mimo-*, nemotron-*, etc. -> /zen/v1/chat/completions)
    ChatCompletions,
    /// TypeSafe AI System One structured evaluation API (e.g. jev-* models -> /zen/v1/systemone)
    SystemOne,
}

impl ModelProtocol {
    /// Checks whether a model belongs to TypeSafe AI's System One decision engine.
    pub fn is_system_one(
        id: &str,
        family: Option<&str>,
        description: Option<&str>,
        owned_by: Option<&str>,
    ) -> bool {
        let id_lower = id.to_ascii_lowercase();
        if id_lower.starts_with("jev")
            || id_lower.contains("jev-")
            || id_lower.contains("-jev")
            || id_lower.contains("systemone")
            || id_lower.contains("system-one")
        {
            return true;
        }

        if let Some(fam) = family {
            let fam_lower = fam.to_ascii_lowercase();
            if fam_lower.contains("typesafe")
                || fam_lower.contains("systemone")
                || fam_lower.contains("system-one")
                || fam_lower == "jev"
            {
                return true;
            }
        }

        if let Some(owner) = owned_by {
            let owner_lower = owner.to_ascii_lowercase();
            if owner_lower.contains("typesafe") || owner_lower.contains("systemone") {
                return true;
            }
        }

        if let Some(desc) = description {
            let desc_lower = desc.to_ascii_lowercase();
            if desc_lower.contains("system one")
                || desc_lower.contains("typesafe ai")
                || desc_lower.contains("decision engine")
                || desc_lower.contains("decision model")
            {
                return true;
            }
        }

        false
    }

    /// Checks whether a model belongs to the OpenAI Responses API family (e.g. Muse).
    pub fn is_responses(
        id: &str,
        family: Option<&str>,
        description: Option<&str>,
    ) -> bool {
        let id_lower = id.to_ascii_lowercase();
        if id_lower.starts_with("muse")
            || id_lower.contains("muse-")
            || id_lower.contains("-muse")
            || id_lower.contains("responses")
        {
            return true;
        }

        if let Some(fam) = family {
            let fam_lower = fam.to_ascii_lowercase();
            if fam_lower.contains("muse") || fam_lower.contains("responses") {
                return true;
            }
        }

        if let Some(desc) = description {
            let desc_lower = desc.to_ascii_lowercase();
            if desc_lower.contains("responses api") || desc_lower.contains("muse") {
                return true;
            }
        }

        false
    }

    /// Automatically detects the protocol wire format based on model identifiers and metadata.
    pub fn detect(
        id: &str,
        family: Option<&str>,
        description: Option<&str>,
        owned_by: Option<&str>,
    ) -> Self {
        if Self::is_responses(id, family, description) {
            ModelProtocol::Responses
        } else if Self::is_system_one(id, family, description, owned_by) {
            ModelProtocol::SystemOne
        } else {
            ModelProtocol::ChatCompletions
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningType {
    /// Supports effort levels, e.g. ["minimal", "low", "medium", "high", "xhigh"]
    Effort { values: Vec<String> },
    /// Toggle boolean
    Toggle,
    /// Interleaved reasoning output (e.g. returns delta.reasoning / delta.reasoning_content)
    Interleaved,
    /// No reasoning support
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub family: String,
    pub protocol: ModelProtocol,
    pub reasoning: ReasoningType,
    pub context_limit: u64,
    pub output_limit: u64,
    pub is_free: bool,
    pub created: u64,
    pub owned_by: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_one_model_auto_detection() {
        // ID based detection
        assert_eq!(ModelProtocol::detect("jev-1.13", None, None, None), ModelProtocol::SystemOne);
        assert_eq!(ModelProtocol::detect("jev-1.13-free", None, None, None), ModelProtocol::SystemOne);
        assert_eq!(ModelProtocol::detect("jev-1.14", None, None, None), ModelProtocol::SystemOne);
        assert_eq!(ModelProtocol::detect("jev-2.0-preview", None, None, None), ModelProtocol::SystemOne);
        assert_eq!(ModelProtocol::detect("typesafe-jev-free", None, None, None), ModelProtocol::SystemOne);
        assert_eq!(ModelProtocol::detect("systemone-reasoner", None, None, None), ModelProtocol::SystemOne);

        // Family / provider based detection
        assert_eq!(ModelProtocol::detect("custom-model", Some("typesafe"), None, None), ModelProtocol::SystemOne);
        assert_eq!(ModelProtocol::detect("nextgen-decision", Some("system-one"), None, None), ModelProtocol::SystemOne);

        // Description based detection
        assert_eq!(
            ModelProtocol::detect("unnamed-model", None, Some("TypeSafe AI decision engine"), None),
            ModelProtocol::SystemOne
        );

        // Other protocols
        assert_eq!(ModelProtocol::detect("muse-spark-1.3", None, None, None), ModelProtocol::Responses);
        assert_eq!(ModelProtocol::detect("mimo-v2.6-flash-free", None, None, None), ModelProtocol::ChatCompletions);
    }

    #[test]
    fn test_responses_model_auto_detection() {
        assert_eq!(ModelProtocol::detect("muse-spark-1.3-contributor-free", None, None, None), ModelProtocol::Responses);
        assert_eq!(ModelProtocol::detect("muse-v2", None, None, None), ModelProtocol::Responses);
        assert_eq!(ModelProtocol::detect("contributor-muse-preview", None, None, None), ModelProtocol::Responses);
        assert_eq!(ModelProtocol::detect("experimental-model", Some("muse"), None, None), ModelProtocol::Responses);
        assert_eq!(ModelProtocol::detect("custom-llm", None, Some("OpenAI responses api model"), None), ModelProtocol::Responses);
    }
}
