use super::{
    ModelAdapter, ModelQuirks, NonStreamingAccumulator, StreamChunkDelta,
};
use crate::registry::metadata::{ModelMetadata, ModelProtocol};
use crate::upstream::payload::OpenAiChatRequest;
use serde_json::{json, Value};

/// Model adapter for TypeSafe AI System One decision models (`jev-1.13`, `jev-1.13-free`)
/// targeting the `/zen/v1/systemone` evaluation endpoint and chat bridge.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemOneAdapter;

impl SystemOneAdapter {
    /// Builds a System One structured evaluation payload from standard chat messages.
    pub fn build_chat_bridge_payload(
        &self,
        model_id: &str,
        messages: &[Value],
        extra_questions: Option<&Value>,
    ) -> Value {
        let mut user_prompt = String::new();
        for msg in messages {
            if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                if !user_prompt.is_empty() {
                    user_prompt.push_str("\n\n");
                }
                user_prompt.push_str(content);
            }
        }

        let questions = if let Some(q) = extra_questions {
            q.clone()
        } else {
            json!({
                "decision": {
                    "type": "noul",
                    "instructions": "Evaluate the state against the query"
                }
            })
        };

        json!({
            "model": model_id,
            "state": user_prompt,
            "questions": questions
        })
    }

    /// Extracts and pretty-prints the decision answers from upstream System One JSON bytes.
    pub fn format_evaluation_response(&self, body_bytes: &[u8]) -> Result<String, String> {
        let sys_result: Value = serde_json::from_slice(body_bytes)
            .map_err(|e| format!("Failed to parse System One response: {e}"))?;
        let answer_obj = sys_result.get("answers").unwrap_or(&sys_result);
        serde_json::to_string_pretty(answer_obj)
            .map_err(|e| format!("Failed to format System One answer: {e}"))
    }
}

impl ModelAdapter for SystemOneAdapter {
    fn name(&self) -> &'static str {
        "systemone"
    }

    fn matches(&self, meta: &ModelMetadata) -> bool {
        meta.protocol == ModelProtocol::SystemOne
            || meta.id.starts_with("jev")
            || meta.family.eq_ignore_ascii_case("jev")
    }

    fn protocol(&self) -> ModelProtocol {
        ModelProtocol::SystemOne
    }

    fn quirks(&self) -> ModelQuirks {
        ModelQuirks {
            min_output_tokens: None,
            requires_dummy_tools: false,
            suppress_reasoning_effort: true,
        }
    }

    fn normalize_tool(&self, tool: &Value) -> (Value, String) {
        (tool.clone(), String::new())
    }

    fn inject_mandatory_tools(&self, _tools: &mut Vec<Value>, _has_bash: bool, _has_read: bool) {
        // System One does not use function tools
    }

    fn build_payload(
        &self,
        req: &OpenAiChatRequest,
        model: &ModelMetadata,
        _override_effort: Option<&str>,
    ) -> Value {
        self.build_chat_bridge_payload(&model.id, &req.messages, req.extra.get("questions"))
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
