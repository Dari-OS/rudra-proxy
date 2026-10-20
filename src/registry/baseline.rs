use crate::registry::metadata::{ModelMetadata, ModelProtocol, ReasoningType};
use std::collections::HashMap;

/// Returns the embedded offline fallback catalog of the 12 verified OpenCode Zen free models.
pub fn get_baseline_models() -> HashMap<String, ModelMetadata> {
    let mut map = HashMap::new();

    let models = vec![
        ModelMetadata {
            id: "beam-free".to_string(),
            name: "Beam Free".to_string(),
            description: "Sparse mixture-of-experts reasoning model with 23B active parameters for coding and agentic workflows".to_string(),
            family: "beam".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            reasoning: ReasoningType::Effort {
                values: vec![
                    "low".to_string(),
                    "medium".to_string(),
                    "high".to_string(),
                    "xhigh".to_string(),
                    "max".to_string(),
                ],
            },
            context_limit: 1048576,
            output_limit: 131072,
            is_free: true,
            created: 1728424000,
            owned_by: "opencode".to_string(),
        },
        ModelMetadata {
            id: "big-pickle".to_string(),
            name: "Big Pickle".to_string(),
            description: "Reasoning model for deliberate analysis, multi-step problem solving, and tool use".to_string(),
            family: "big-pickle".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            reasoning: ReasoningType::Interleaved,
            context_limit: 200000,
            output_limit: 32000,
            is_free: true,
            created: 1728424000,
            owned_by: "opencode".to_string(),
        },
        ModelMetadata {
            id: "deepseek-v4.1-flash-free".to_string(),
            name: "DeepSeek V4.1 Flash Free".to_string(),
            description: "Fast DeepSeek reasoning model with multi-tier effort".to_string(),
            family: "deepseek-flash".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            reasoning: ReasoningType::Effort {
                values: vec![
                    "low".to_string(),
                    "high".to_string(),
                    "max".to_string(),
                ],
            },
            context_limit: 200000,
            output_limit: 32000,
            is_free: true,
            created: 1728424000,
            owned_by: "opencode".to_string(),
        },
        ModelMetadata {
            id: "glm-5.3-flash-free".to_string(),
            name: "GLM-5.3-Flash Free".to_string(),
            description: "Zhipu AI high-speed reasoning model".to_string(),
            family: "glm-flash".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            reasoning: ReasoningType::Effort {
                values: vec![
                    "low".to_string(),
                    "high".to_string(),
                    "max".to_string(),
                ],
            },
            context_limit: 200000,
            output_limit: 32000,
            is_free: true,
            created: 1728424000,
            owned_by: "opencode".to_string(),
        },
        ModelMetadata {
            id: "jev-1.13-free".to_string(),
            name: "Jev 1.13 Free (System One)".to_string(),
            description: "TypeSafe AI System One model for fast, structured decisions".to_string(),
            family: "jev".to_string(),
            protocol: ModelProtocol::SystemOne,
            reasoning: ReasoningType::None,
            context_limit: 131072,
            output_limit: 16384,
            is_free: true,
            created: 1728424000,
            owned_by: "opencode".to_string(),
        },
        ModelMetadata {
            id: "longcat-2.5-preview-free".to_string(),
            name: "Longcat 2.5 Preview Free".to_string(),
            description: "Extended context model with toggleable reasoning".to_string(),
            family: "longcat".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            reasoning: ReasoningType::Toggle,
            context_limit: 1000000,
            output_limit: 131072,
            is_free: true,
            created: 1728424000,
            owned_by: "opencode".to_string(),
        },
        ModelMetadata {
            id: "mimo-v2.6-flash-free".to_string(),
            name: "Mimo v2.6 Flash Free".to_string(),
            description: "Ultra-fast multimodal reasoning model with interleaved thinking".to_string(),
            family: "mimo".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            reasoning: ReasoningType::Interleaved,
            context_limit: 200000,
            output_limit: 32000,
            is_free: true,
            created: 1728424000,
            owned_by: "opencode".to_string(),
        },
        ModelMetadata {
            id: "nemotron-3.5-lightning-free".to_string(),
            name: "Nemotron 3.5 Lightning Free".to_string(),
            description: "NVIDIA fast reasoning model with interleaved thinking".to_string(),
            family: "nemotron".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            reasoning: ReasoningType::Interleaved,
            context_limit: 262144,
            output_limit: 262144,
            is_free: true,
            created: 1728424000,
            owned_by: "opencode".to_string(),
        },
        ModelMetadata {
            id: "nemotron-3.5-super-vl-free".to_string(),
            name: "Nemotron 3.5 Super VL Free".to_string(),
            description: "NVIDIA high-performance reasoning vision-language model".to_string(),
            family: "nemotron".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            reasoning: ReasoningType::Effort {
                values: vec![
                    "none".to_string(),
                    "minimal".to_string(),
                    "low".to_string(),
                    "medium".to_string(),
                    "high".to_string(),
                    "xhigh".to_string(),
                ],
            },
            context_limit: 1048576,
            output_limit: 131072,
            is_free: true,
            created: 1728424000,
            owned_by: "opencode".to_string(),
        },
        ModelMetadata {
            id: "minimax-m3.1-flash-free".to_string(),
            name: "MiniMax M3.1 Flash Free".to_string(),
            description: "Frontier multimodal coding model with 1M context window and tunable thinking depth".to_string(),
            family: "minimax".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            reasoning: ReasoningType::Effort {
                values: vec![
                    "low".to_string(),
                    "medium".to_string(),
                    "high".to_string(),
                    "xhigh".to_string(),
                    "max".to_string(),
                ],
            },
            context_limit: 1000000,
            output_limit: 512000,
            is_free: true,
            created: 1728424000,
            owned_by: "opencode".to_string(),
        },
    ];

    for m in models {
        map.insert(m.id.clone(), m);
    }

    map
}
