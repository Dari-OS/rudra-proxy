use super::{
    ExtractedToolCallChunk, ModelAdapter, ModelQuirks, NonStreamingAccumulator, StreamChunkDelta,
};
use crate::registry::metadata::{ModelMetadata, ModelProtocol};
use crate::upstream::effort::resolve_responses_reasoning;
use crate::upstream::payload::{convert_messages_to_responses_input, normalize_tool_for_responses, OpenAiChatRequest};
use serde_json::{json, Value};

/// Model adapter for Muse Spark family models (`muse-*`) operating over
/// the OpenAI Responses API protocol (`/zen/v1/responses`).
#[derive(Clone, Copy, Debug, Default)]
pub struct MuseAdapter;

impl ModelAdapter for MuseAdapter {
    fn name(&self) -> &'static str {
        "muse"
    }

    fn matches(&self, meta: &ModelMetadata) -> bool {
        meta.protocol == ModelProtocol::Responses
            || meta.id.starts_with("muse")
            || meta.family.eq_ignore_ascii_case("muse")
            || meta.description.to_ascii_lowercase().contains("responses api")
    }

    fn protocol(&self) -> ModelProtocol {
        ModelProtocol::Responses
    }

    fn quirks(&self) -> ModelQuirks {
        ModelQuirks {
            min_output_tokens: Some(16),
            requires_dummy_tools: true,
            suppress_reasoning_effort: false,
        }
    }

    fn normalize_tool(&self, tool: &Value) -> (Value, String) {
        normalize_tool_for_responses(tool)
    }

    fn inject_mandatory_tools(&self, tools: &mut Vec<Value>, has_bash: bool, has_read: bool) {
        if !has_bash {
            tools.push(json!({
                "type": "function",
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
            }));
        }
        if !has_read {
            tools.push(json!({
                "type": "function",
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

        let converted_input = convert_messages_to_responses_input(&req.messages);
        let mut payload = json!({
            "model": model.id,
            "input": converted_input,
            "stream": true,
            "store": false,
            "tools": tools,
        });

        if let Some(temp) = req.temperature {
            payload["temperature"] = json!(temp);
        }
        if let Some(top_p) = req.top_p {
            payload["top_p"] = json!(top_p);
        }

        // Responses API protocol specifies max_output_tokens, and upstream requires >= 16
        if let Some(max) = req.max_tokens_limit() {
            let clamped_max = max.max(16);
            payload["max_output_tokens"] = json!(clamped_max);
        }

        // Flatten tool_choice if nested function.name is provided
        if let Some(ref choice) = req.tool_choice {
            if let Some(func) = choice.get("function") {
                if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                    payload["tool_choice"] = json!({
                        "type": "function",
                        "name": name
                    });
                } else {
                    payload["tool_choice"] = choice.clone();
                }
            } else {
                payload["tool_choice"] = choice.clone();
            }
        }

        if let Some(ref user) = req.user {
            payload["user"] = json!(user);
        }

        let reasoning_obj =
            resolve_responses_reasoning(req.reasoning_effort.as_deref(), override_effort);
        payload["reasoning"] = reasoning_obj;

        payload
    }

    fn parse_stream_event(&self, event_type: &str, data: &Value) -> Option<StreamChunkDelta> {
        let mut delta = StreamChunkDelta::default();

        if let Some(err) = data.get("error") {
            delta.is_error = true;
            delta.error_message = Some(err.to_string());
        }

        match event_type {
            "response.output_text.delta" => {
                if let Some(t) = data.get("delta").and_then(|d| d.as_str()) {
                    delta.content = Some(t.to_string());
                }
                Some(delta)
            }
            "response.output_item.added" => {
                if let Some(item) = data.get("item")
                    && item.get("type").and_then(|t| t.as_str()) == Some("function_call")
                {
                    let idx = data.get("output_index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    let call_id = item.get("call_id").and_then(|c| c.as_str()).map(|s| s.to_string());
                    let name = item.get("name").and_then(|n| n.as_str()).map(|s| s.to_string());
                    delta.tool_calls.push(ExtractedToolCallChunk {
                        index: idx,
                        id: call_id,
                        tool_type: Some("function".to_string()),
                        name,
                        arguments_delta: Some(String::new()),
                    });
                    Some(delta)
                } else {
                    None
                }
            }
            "response.function_call_arguments.delta" => {
                let idx = data.get("output_index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                let arg_delta = data.get("delta").and_then(|d| d.as_str()).unwrap_or("");
                delta.tool_calls.push(ExtractedToolCallChunk {
                    index: idx,
                    id: None,
                    tool_type: None,
                    name: None,
                    arguments_delta: Some(arg_delta.to_string()),
                });
                Some(delta)
            }
            "response.completed" => {
                delta.finish_reason = Some("stop".to_string());
                Some(delta)
            }
            "response.incomplete" => {
                let reason = data
                    .get("response")
                    .and_then(|r| r.get("incomplete_details"))
                    .and_then(|d| d.get("reason"))
                    .and_then(|r| r.as_str());
                let finish = if reason == Some("max_output_tokens") {
                    "length"
                } else {
                    "stop"
                };
                delta.finish_reason = Some(finish.to_string());
                Some(delta)
            }
            _ => {
                if delta.is_error {
                    Some(delta)
                } else {
                    None
                }
            }
        }
    }

    fn accumulate_non_streaming(
        &self,
        event_type: &str,
        data: &Value,
        acc: &mut NonStreamingAccumulator,
    ) {
        match event_type {
            "response.output_text.delta" => {
                if let Some(delta) = data.get("delta").and_then(|d| d.as_str()) {
                    acc.text.push_str(delta);
                }
            }
            "response.output_item.added" => {
                if let Some(item) = data.get("item")
                    && item.get("type").and_then(|t| t.as_str()) == Some("function_call")
                {
                    let idx = data.get("output_index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    let call_id = item.get("call_id").and_then(|c| c.as_str()).unwrap_or("").to_string();
                    let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                    let args = item.get("arguments").and_then(|a| a.as_str()).unwrap_or("").to_string();
                    let entry = acc.tool_calls.entry(idx).or_insert((
                        call_id.clone(),
                        "function".to_string(),
                        name.clone(),
                        String::new(),
                    ));
                    if !call_id.is_empty() {
                        entry.0 = call_id;
                    }
                    if !name.is_empty() {
                        entry.2 = name;
                    }
                    if !args.is_empty() {
                        entry.3.push_str(&args);
                    }
                }
            }
            "response.function_call_arguments.delta" => {
                let idx = data.get("output_index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                if let Some(delta) = data.get("delta").and_then(|d| d.as_str()) {
                    let entry = acc.tool_calls.entry(idx).or_insert((
                        String::new(),
                        "function".to_string(),
                        String::new(),
                        String::new(),
                    ));
                    entry.3.push_str(delta);
                }
            }
            "response.completed" => {
                if let Some(resp_obj) = data.get("response")
                    && let Some(outputs) = resp_obj.get("output").and_then(|o| o.as_array())
                {
                    for (idx, out_item) in outputs.iter().enumerate() {
                        if out_item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
                            let call_id = out_item.get("call_id").and_then(|c| c.as_str()).unwrap_or("").to_string();
                            let name = out_item.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                            let args = out_item.get("arguments").and_then(|a| a.as_str()).unwrap_or("").to_string();
                            let entry = acc.tool_calls.entry(idx).or_insert((
                                call_id.clone(),
                                "function".to_string(),
                                name.clone(),
                                String::new(),
                            ));
                            if !call_id.is_empty() {
                                entry.0 = call_id;
                            }
                            if !name.is_empty() {
                                entry.2 = name;
                            }
                            if entry.3.is_empty() && !args.is_empty() {
                                entry.3 = args;
                            }
                        }
                    }
                }
            }
            "response.incomplete" => {
                let reason = data
                    .get("response")
                    .and_then(|r| r.get("incomplete_details"))
                    .and_then(|d| d.get("reason"))
                    .and_then(|r| r.as_str());
                let finish = if reason == Some("max_output_tokens") {
                    "length"
                } else {
                    "stop"
                };
                acc.reported_finish_reason = Some(finish.to_string());
            }
            _ => {}
        }
    }
}
