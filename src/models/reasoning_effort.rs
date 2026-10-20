use super::{
    ExtractedToolCallChunk, ModelAdapter, ModelQuirks, NonStreamingAccumulator, StreamChunkDelta,
};
use crate::registry::metadata::{ModelMetadata, ModelProtocol, ReasoningType};
use crate::upstream::effort::resolve_chat_reasoning_effort;
use crate::upstream::payload::{OpenAiChatRequest, normalize_tool_for_chat};
use serde_json::{Value, json};

/// Model adapter for models supporting discrete reasoning effort parameters
/// (e.g. `space-bunny-free`, `step-5-preview-free`, `exo-free`).
#[derive(Clone, Copy, Debug, Default)]
pub struct ReasoningEffortAdapter;

impl ModelAdapter for ReasoningEffortAdapter {
    fn name(&self) -> &'static str {
        "reasoning_effort"
    }

    fn matches(&self, meta: &ModelMetadata) -> bool {
        (matches!(meta.reasoning, ReasoningType::Effort { .. })
            || meta.id.contains("bunny")
            || meta.id.contains("beam")
            || meta.id.contains("step-5")
            || meta.id.contains("cat-jam")
            || meta.id.contains("deepseek")
            || meta.id.contains("glm")
            || meta.id.contains("minimax")
            || meta.id.contains("alpha")
            || meta.id.contains("super-vl")
            || meta.id.contains("exo"))
            && meta.protocol == ModelProtocol::ChatCompletions
    }

    fn protocol(&self) -> ModelProtocol {
        ModelProtocol::ChatCompletions
    }

    fn quirks(&self) -> ModelQuirks {
        ModelQuirks {
            min_output_tokens: None,
            requires_dummy_tools: true,
            suppress_reasoning_effort: false,
        }
    }

    fn normalize_tool(&self, tool: &Value) -> (Value, String) {
        normalize_tool_for_chat(tool)
    }

    fn inject_mandatory_tools(&self, tools: &mut Vec<Value>, has_bash: bool, has_read: bool) {
        if !has_bash {
            tools.push(json!({
                "type": "function",
                "function": {
                    "name": "bash",
                    "description": "bash",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": { "type": "string" }
                        },
                        "required": ["command"]
                    },
                    "strict": false
                }
            }));
        }
        if !has_read {
            tools.push(json!({
                "type": "function",
                "function": {
                    "name": "read",
                    "description": "read",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "filePath": { "type": "string" }
                        },
                        "required": ["filePath"]
                    },
                    "strict": false
                }
            }));
        }
    }

    fn build_payload(
        &self,
        req: &OpenAiChatRequest,
        model: &ModelMetadata,
        override_effort: Option<&str>,
    ) -> Value {
        let mut tools = Vec::new();
        let mut has_bash = false;
        let mut has_read = false;

        if let Some(ref client_tools) = req.tools {
            for t in client_tools {
                let (normalized, name) = self.normalize_tool(t);
                if name == "bash" {
                    has_bash = true;
                }
                if name == "read" {
                    has_read = true;
                }
                tools.push(normalized);
            }
        }

        self.inject_mandatory_tools(&mut tools, has_bash, has_read);

        let mut payload = json!({
            "model": model.id,
            "messages": req.messages,
            "stream": true, // Zen gateway strictly requires stream: true
            "tools": tools,
        });

        if let Some(temp) = req.temperature {
            payload["temperature"] = json!(temp);
        }
        if let Some(top_p) = req.top_p {
            payload["top_p"] = json!(top_p);
        }
        if let Some(max) = req.max_tokens_limit() {
            payload["max_tokens"] = json!(max);
        }
        if let Some(ref stop) = req.stop {
            payload["stop"] = stop.clone();
        }
        if let Some(pres) = req.presence_penalty {
            payload["presence_penalty"] = json!(pres);
        }
        if let Some(freq) = req.frequency_penalty {
            payload["frequency_penalty"] = json!(freq);
        }
        if let Some(seed) = req.seed {
            payload["seed"] = json!(seed);
        }
        if let Some(ref fmt) = req.response_format {
            payload["response_format"] = fmt.clone();
        }
        if let Some(ref choice) = req.tool_choice {
            payload["tool_choice"] = choice.clone();
        }
        if let Some(ref user) = req.user {
            payload["user"] = json!(user);
        }

        // Inject reasoning_effort validated against model-supported values
        if let Some(effort) =
            resolve_chat_reasoning_effort(model, req.reasoning_effort.as_deref(), override_effort)
        {
            payload["reasoning_effort"] = json!(effort);
        }

        payload
    }

    fn parse_stream_event(&self, _event_type: &str, data: &Value) -> Option<StreamChunkDelta> {
        let mut delta = StreamChunkDelta::default();

        if let Some(err) = data.get("error") {
            delta.is_error = true;
            delta.error_message = Some(err.to_string());
        }

        if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                    delta.finish_reason = Some(fr.to_string());
                }
                if let Some(delta_obj) = choice.get("delta") {
                    if let Some(c) = delta_obj.get("content").and_then(|c| c.as_str()) {
                        delta.content = Some(c.to_string());
                    }
                    if let Some(r) = delta_obj
                        .get("reasoning_content")
                        .or_else(|| delta_obj.get("reasoning"))
                        .and_then(|r| r.as_str())
                    {
                        delta.reasoning_content = Some(r.to_string());
                    }
                    if let Some(tcs) = delta_obj.get("tool_calls").and_then(|t| t.as_array()) {
                        for tc in tcs {
                            let idx =
                                tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                            let id = tc.get("id").and_then(|i| i.as_str()).map(|s| s.to_string());
                            let tool_type = tc
                                .get("type")
                                .and_then(|t| t.as_str())
                                .map(|s| s.to_string());
                            let name = tc
                                .get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str())
                                .map(|s| s.to_string());
                            let args_delta = tc
                                .get("function")
                                .and_then(|f| f.get("arguments"))
                                .and_then(|a| a.as_str())
                                .map(|s| s.to_string());
                            delta.tool_calls.push(ExtractedToolCallChunk {
                                index: idx,
                                id,
                                tool_type,
                                name,
                                arguments_delta: args_delta,
                            });
                        }
                    }
                }
            }
            Some(delta)
        } else if delta.is_error {
            Some(delta)
        } else {
            None
        }
    }

    fn accumulate_non_streaming(
        &self,
        _event_type: &str,
        data: &Value,
        acc: &mut NonStreamingAccumulator,
    ) {
        if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                    acc.reported_finish_reason = Some(fr.to_string());
                }
                if let Some(delta_obj) = choice.get("delta") {
                    if let Some(content) = delta_obj.get("content").and_then(|c| c.as_str()) {
                        acc.text.push_str(content);
                    }
                    if let Some(reasoning) = delta_obj
                        .get("reasoning_content")
                        .or_else(|| delta_obj.get("reasoning"))
                        .and_then(|r| r.as_str())
                    {
                        acc.reasoning.push_str(reasoning);
                    }
                    if let Some(tool_calls) = delta_obj.get("tool_calls").and_then(|t| t.as_array())
                    {
                        for tc in tool_calls {
                            let idx =
                                tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                            let entry = acc.tool_calls.entry(idx).or_insert((
                                String::new(),
                                "function".to_string(),
                                String::new(),
                                String::new(),
                            ));
                            if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                entry.0 = id.to_string();
                            }
                            if let Some(c_type) = tc.get("type").and_then(|t| t.as_str()) {
                                entry.1 = c_type.to_string();
                            }
                            if let Some(func) = tc.get("function") {
                                if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                    entry.2 = name.to_string();
                                }
                                if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                    entry.3.push_str(args);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
