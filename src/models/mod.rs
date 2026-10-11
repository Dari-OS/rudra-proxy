pub mod mimo;
pub mod muse;
pub mod reasoning_effort;
pub mod standard;
pub mod systemone;

use crate::registry::metadata::{ModelMetadata, ModelProtocol};
use crate::upstream::payload::OpenAiChatRequest;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Extracted delta from an SSE chunk or event from the upstream gateway.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamChunkDelta {
    /// Incremental content delta for assistant message.
    pub content: Option<String>,
    /// Incremental reasoning or thought delta.
    pub reasoning_content: Option<String>,
    /// Incremental or complete tool call definitions.
    pub tool_calls: Vec<ExtractedToolCallChunk>,
    /// Finish reason if this chunk indicates completion (e.g. "stop", "length", "tool_calls").
    pub finish_reason: Option<String>,
    /// Whether an upstream error occurred in this chunk.
    pub is_error: bool,
    /// Error message string if present.
    pub error_message: Option<String>,
}

/// An incremental or newly added tool call chunk within a stream event.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtractedToolCallChunk {
    pub index: usize,
    pub id: Option<String>,
    pub tool_type: Option<String>,
    pub name: Option<String>,
    pub arguments_delta: Option<String>,
}

/// Accumulator for assembling non-streaming responses from Server-Sent Events.
#[derive(Clone, Debug, Default)]
pub struct NonStreamingAccumulator {
    pub text: String,
    pub reasoning: String,
    /// index -> (call_id, tool_type, name, arguments)
    pub tool_calls: BTreeMap<usize, (String, String, String, String)>,
    pub reported_finish_reason: Option<String>,
}

/// Model-specific quirks and behavioral requirements.
#[derive(Clone, Debug, Default)]
pub struct ModelQuirks {
    /// Minimum allowed output token limit (e.g. 16 for Muse Responses API).
    pub min_output_tokens: Option<i64>,
    /// Whether the model requires dummy tools (bash + read) to pass contributor verification.
    pub requires_dummy_tools: bool,
    /// Whether reasoning_effort parameter must be omitted to prevent upstream schema rejection.
    pub suppress_reasoning_effort: bool,
}

/// Pluggable interface for model-specific quirks, protocol adaptations, and stream parsing.
pub trait ModelAdapter: Send + Sync {
    /// Human-readable adapter identifier (e.g. "muse", "mimo", "systemone", "reasoning_effort", "standard").
    fn name(&self) -> &'static str;

    /// Checks if this adapter applies to the given model metadata.
    fn matches(&self, meta: &ModelMetadata) -> bool;

    /// Wire protocol required by this model for upstream dispatch.
    fn protocol(&self) -> ModelProtocol;

    /// Model-specific behavioral constraints.
    fn quirks(&self) -> ModelQuirks {
        ModelQuirks::default()
    }

    /// Normalizes a tool definition from downstream client format into upstream wire format.
    fn normalize_tool(&self, tool: &Value) -> (Value, String);

    /// Injects mandatory gateway tools (e.g. bash + read) if required.
    fn inject_mandatory_tools(&self, tools: &mut Vec<Value>, has_bash: bool, has_read: bool);

    /// Formats the downstream request into the exact upstream JSON payload for OpenCode Zen.
    fn build_payload(
        &self,
        req: &OpenAiChatRequest,
        meta: &ModelMetadata,
        override_effort: Option<&str>,
    ) -> Value;

    /// Parses an SSE event received from upstream and extracts text/reasoning/tool deltas.
    fn parse_stream_event(&self, event_type: &str, data: &Value) -> Option<StreamChunkDelta>;

    /// Accumulates an event into the non-streaming response collector.
    fn accumulate_non_streaming(
        &self,
        event_type: &str,
        data: &Value,
        acc: &mut NonStreamingAccumulator,
    );
}

/// Registry of pluggable model adapters, evaluated in priority order.
#[derive(Clone)]
pub struct AdapterRegistry {
    adapters: Vec<Arc<dyn ModelAdapter>>,
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AdapterRegistry {
    /// Creates a registry initialized with all built-in model adapters in priority order.
    pub fn new() -> Self {
        Self {
            adapters: vec![
                Arc::new(muse::MuseAdapter),
                Arc::new(systemone::SystemOneAdapter),
                Arc::new(mimo::MimoAdapter),
                Arc::new(reasoning_effort::ReasoningEffortAdapter),
                Arc::new(standard::StandardAdapter),
            ],
        }
    }

    /// Finds the first adapter that matches the given model metadata.
    /// Falls back to StandardAdapter if no specialized adapter matches.
    pub fn get_adapter(&self, meta: &ModelMetadata) -> Arc<dyn ModelAdapter> {
        for adapter in &self.adapters {
            if adapter.matches(meta) {
                return Arc::clone(adapter);
            }
        }
        Arc::new(standard::StandardAdapter)
    }

    /// Registers a custom model adapter before the standard fallback adapter.
    pub fn register(&mut self, adapter: Arc<dyn ModelAdapter>) {
        if self.adapters.len() > 1 {
            self.adapters.insert(self.adapters.len() - 1, adapter);
        } else {
            self.adapters.insert(0, adapter);
        }
    }

    /// Lists the names of all currently registered adapters.
    pub fn list_adapters(&self) -> Vec<&'static str> {
        self.adapters.iter().map(|a| a.name()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::metadata::{ModelMetadata, ModelProtocol};
    use serde_json::json;

    #[test]
    fn test_adapter_registry_matching() {
        let registry = AdapterRegistry::default();

        let muse_meta = ModelMetadata {
            id: "muse-spark-1.3-contributor-free".to_string(),
            protocol: ModelProtocol::Responses,
            family: "muse".to_string(),
            ..Default::default()
        };
        assert_eq!(registry.get_adapter(&muse_meta).name(), "muse");

        let mimo_meta = ModelMetadata {
            id: "mimo-v2.6-flash-free".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            family: "mimo".to_string(),
            ..Default::default()
        };
        assert_eq!(registry.get_adapter(&mimo_meta).name(), "mimo");

        let bunny_meta = ModelMetadata {
            id: "space-bunny-free".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            family: "bunny".to_string(),
            ..Default::default()
        };
        assert_eq!(registry.get_adapter(&bunny_meta).name(), "reasoning_effort");

        let jev_meta = ModelMetadata {
            id: "jev-1.13-free".to_string(),
            protocol: ModelProtocol::SystemOne,
            family: "jev".to_string(),
            ..Default::default()
        };
        assert_eq!(registry.get_adapter(&jev_meta).name(), "systemone");

        let unknown_meta = ModelMetadata {
            id: "unknown-future-model".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            family: "future".to_string(),
            ..Default::default()
        };
        assert_eq!(registry.get_adapter(&unknown_meta).name(), "standard");
    }

    #[test]
    fn test_custom_adapter_registration() {
        struct CustomAdapter;
        impl ModelAdapter for CustomAdapter {
            fn name(&self) -> &'static str {
                "custom"
            }
            fn matches(&self, meta: &ModelMetadata) -> bool {
                meta.id == "my-custom-model"
            }
            fn protocol(&self) -> ModelProtocol {
                ModelProtocol::ChatCompletions
            }
            fn normalize_tool(&self, tool: &Value) -> (Value, String) {
                (tool.clone(), String::new())
            }
            fn inject_mandatory_tools(&self, _tools: &mut Vec<Value>, _has_bash: bool, _has_read: bool) {}
            fn build_payload(
                &self,
                _req: &OpenAiChatRequest,
                _meta: &ModelMetadata,
                _effort: Option<&str>,
            ) -> Value {
                json!({})
            }
            fn parse_stream_event(&self, _event_type: &str, _data: &Value) -> Option<StreamChunkDelta> {
                None
            }
            fn accumulate_non_streaming(
                &self,
                _event_type: &str,
                _data: &Value,
                _acc: &mut NonStreamingAccumulator,
            ) {
            }
        }

        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(CustomAdapter));

        let custom_meta = ModelMetadata {
            id: "my-custom-model".to_string(),
            ..Default::default()
        };
        assert_eq!(registry.get_adapter(&custom_meta).name(), "custom");
    }

    #[test]
    fn test_muse_payload_quirks_and_stream_parsing() {
        let registry = AdapterRegistry::default();
        let muse_meta = ModelMetadata {
            id: "muse-spark-1.3-contributor-free".to_string(),
            protocol: ModelProtocol::Responses,
            family: "muse".to_string(),
            ..Default::default()
        };
        let adapter = registry.get_adapter(&muse_meta);
        assert_eq!(adapter.quirks().min_output_tokens, Some(16));
        assert!(adapter.quirks().requires_dummy_tools);

        // Clamping min tokens
        let req = OpenAiChatRequest {
            model: muse_meta.id.clone(),
            messages: vec![json!({"role": "user", "content": "hi"})],
            max_tokens: Some(5),
            ..Default::default()
        };
        let payload = adapter.build_payload(&req, &muse_meta, None);
        assert_eq!(payload["max_output_tokens"], 16);

        // Dummy tools injected
        let tools = payload["tools"].as_array().expect("tools array");
        assert!(tools.iter().any(|t| t["name"] == "bash"));
        assert!(tools.iter().any(|t| t["name"] == "read"));

        // Stream event parsing
        let delta = adapter
            .parse_stream_event(
                "response.output_text.delta",
                &json!({"type": "response.output_text.delta", "delta": "hello "}),
            )
            .expect("parsed delta");
        assert_eq!(delta.content, Some("hello ".to_string()));

        // Non-streaming accumulation
        let mut acc = NonStreamingAccumulator::default();
        adapter.accumulate_non_streaming(
            "response.output_text.delta",
            &json!({"type": "response.output_text.delta", "delta": "world"}),
            &mut acc,
        );
        assert_eq!(acc.text, "world");
    }

    #[test]
    fn test_mimo_payload_quirks_and_reasoning_stream() {
        let registry = AdapterRegistry::default();
        let mimo_meta = ModelMetadata {
            id: "mimo-v2.6-flash-free".to_string(),
            protocol: ModelProtocol::ChatCompletions,
            family: "mimo".to_string(),
            ..Default::default()
        };
        let adapter = registry.get_adapter(&mimo_meta);
        assert!(adapter.quirks().suppress_reasoning_effort);

        let req = OpenAiChatRequest {
            model: mimo_meta.id.clone(),
            messages: vec![json!({"role": "user", "content": "solve this"})],
            reasoning_effort: Some("high".to_string()),
            ..Default::default()
        };
        let payload = adapter.build_payload(&req, &mimo_meta, None);
        assert!(payload.get("reasoning_effort").is_none());

        // Stream parsing of interleaved reasoning
        let event = json!({
            "choices": [{
                "delta": {
                    "content": "42",
                    "reasoning_content": "calculating..."
                }
            }]
        });
        let delta = adapter.parse_stream_event("", &event).expect("stream chunk");
        assert_eq!(delta.content, Some("42".to_string()));
        assert_eq!(delta.reasoning_content, Some("calculating...".to_string()));
    }
}
