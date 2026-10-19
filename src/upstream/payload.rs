use crate::registry::metadata::ModelMetadata;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

/// Standard OpenAI chat completion request payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatRequest {
    #[serde(default = "default_model")]
    pub model: String,
    pub messages: Vec<Value>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<i64>,
    #[serde(default)]
    pub max_completion_tokens: Option<i64>,
    #[serde(default)]
    pub stop: Option<Value>,
    #[serde(default)]
    pub presence_penalty: Option<f64>,
    #[serde(default)]
    pub frequency_penalty: Option<f64>,
    #[serde(default)]
    pub seed: Option<i64>,
    #[serde(default)]
    pub response_format: Option<Value>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<Value>>,
    #[serde(default)]
    pub tool_choice: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for OpenAiChatRequest {
    fn default() -> Self {
        Self {
            model: default_model(),
            messages: Vec::new(),
            stream: false,
            temperature: None,
            top_p: None,
            max_tokens: None,
            max_completion_tokens: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            seed: None,
            response_format: None,
            user: None,
            reasoning_effort: None,
            tools: None,
            tool_choice: None,
            extra: Map::new(),
        }
    }
}

impl OpenAiChatRequest {
    /// Returns the effective output token limit (`max_completion_tokens` or `max_tokens`).
    pub fn max_tokens_limit(&self) -> Option<i64> {
        self.max_completion_tokens.or(self.max_tokens)
    }
}

fn default_model() -> String {
    "mimo-v2.6-flash-free".to_string()
}

/// Translates standard OpenAI messages (including tool calls and tool outputs)
/// into the format required by the OpenAI Responses API (`/zen/v1/responses`).
pub fn convert_messages_to_responses_input(messages: &[Value]) -> Vec<Value> {
    let mut input = Vec::new();
    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if role == "tool" {
            let call_id = msg
                .get("tool_call_id")
                .and_then(|id| id.as_str())
                .unwrap_or("");
            let content = msg
                .get("content")
                .cloned()
                .unwrap_or(Value::String(String::new()));
            let output_str = if let Some(s) = content.as_str() {
                s.to_string()
            } else {
                content.to_string()
            };
            input.push(json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": output_str
            }));
            continue;
        }

        if role == "assistant"
            && let Some(tool_calls) = msg.get("tool_calls").and_then(|t| t.as_array())
        {
            if let Some(content) = msg.get("content").and_then(|c| c.as_str())
                && !content.is_empty()
            {
                input.push(json!({
                    "role": "assistant",
                    "content": content
                }));
            }
            for tc in tool_calls {
                let call_id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let arguments = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("");
                input.push(json!({
                    "type": "function_call",
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments
                }));
            }
            continue;
        }

        input.push(msg.clone());
    }
    input
}

/// Normalizes tool definitions for the Responses API (`/zen/v1/responses`),
/// where function tools must have `name`, `description`, `parameters` flattened
/// directly under the tool object rather than wrapped in a `function` object.
pub fn normalize_tool_for_responses(t: &Value) -> (Value, String) {
    if let Some(func) = t.get("function") {
        let name = func
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        let desc = func
            .get("description")
            .cloned()
            .unwrap_or(Value::String(String::new()));
        let params = func
            .get("parameters")
            .cloned()
            .unwrap_or(json!({"type": "object", "properties": {}}));
        let mut converted = json!({
            "type": "function",
            "name": name,
            "description": desc,
            "parameters": params
        });
        if let Some(strict) = func.get("strict") {
            converted["strict"] = strict.clone();
        }
        (converted, name)
    } else {
        let name = t
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        (t.clone(), name)
    }
}

/// Normalizes tool definitions for standard OpenAI Chat Completions API (`/zen/v1/chat/completions`),
/// where tools are wrapped as `{"type": "function", "function": { ... }}`.
pub fn normalize_tool_for_chat(t: &Value) -> (Value, String) {
    if let Some(func) = t.get("function") {
        let name = func
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        (t.clone(), name)
    } else if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
        let desc = t
            .get("description")
            .cloned()
            .unwrap_or(Value::String(String::new()));
        let params = t
            .get("parameters")
            .cloned()
            .unwrap_or(json!({"type": "object", "properties": {}}));
        let mut wrapped = json!({
            "type": "function",
            "function": {
                "name": name,
                "description": desc,
                "parameters": params
            }
        });
        if let Some(strict) = t.get("strict") {
            wrapped["function"]["strict"] = strict.clone();
        }
        (wrapped, name.to_string())
    } else {
        (t.clone(), String::new())
    }
}

/// Builds the OpenCode Zen request payload appropriate for the model's protocol,
/// injecting mandatory dummy tools and proper reasoning configurations via the
/// modular adapter system.
pub fn build_opencode_payload(
    req: &OpenAiChatRequest,
    model: &ModelMetadata,
    override_effort: Option<&str>,
) -> Value {
    let adapter = crate::models::AdapterRegistry::default().get_adapter(model);
    adapter.build_payload(req, model, override_effort)
}

/// Creates a standard OpenAI `chat.completion.chunk` SSE event string.
pub fn make_openai_chunk(
    id: &str,
    model: &str,
    delta_text: Option<&str>,
    delta_reasoning: Option<&str>,
) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut delta = json!({});
    if let Some(text) = delta_text {
        delta["content"] = json!(text);
    }
    if let Some(reasoning) = delta_reasoning {
        delta["reasoning_content"] = json!(reasoning);
    }

    let chunk = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": now,
        "model": model,
        "choices": [
            {
                "index": 0,
                "delta": delta,
                "finish_reason": null
            }
        ]
    });

    format!("data: {chunk}\n\n")
}

/// Creates an OpenAI `chat.completion.chunk` SSE event for streaming tool calls.
pub fn make_openai_tool_chunk(
    id: &str,
    model: &str,
    tool_call_index: usize,
    call_id: Option<&str>,
    function_name: Option<&str>,
    function_arguments: Option<&str>,
) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut func_obj = json!({});
    if let Some(name) = function_name {
        func_obj["name"] = json!(name);
    }
    if let Some(args) = function_arguments {
        func_obj["arguments"] = json!(args);
    }

    let mut tc_obj = json!({
        "index": tool_call_index,
        "function": func_obj
    });
    if let Some(cid) = call_id {
        tc_obj["id"] = json!(cid);
        tc_obj["type"] = json!("function");
    }

    let chunk = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": now,
        "model": model,
        "choices": [
            {
                "index": 0,
                "delta": {
                    "tool_calls": [tc_obj]
                },
                "finish_reason": null
            }
        ]
    });

    format!("data: {chunk}\n\n")
}

/// Creates the terminal OpenAI `chat.completion.chunk` SSE event string.
pub fn make_openai_terminal_chunk(id: &str, model: &str, finish_reason: Option<&str>) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let chunk = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": now,
        "model": model,
        "choices": [
            {
                "index": 0,
                "delta": {},
                "finish_reason": finish_reason.unwrap_or("stop")
            }
        ]
    });

    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}

/// Creates a standard OpenAI non-streaming `chat.completion` response JSON object.
pub fn make_openai_completion(
    id: &str,
    model: &str,
    full_text: &str,
    reasoning: Option<&str>,
    tool_calls: Option<Vec<Value>>,
    finish_reason: Option<&str>,
) -> Value {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut message = json!({
        "role": "assistant"
    });

    let has_tools = tool_calls.as_ref().map(|t| !t.is_empty()).unwrap_or(false);

    if has_tools && full_text.is_empty() {
        message["content"] = Value::Null;
    } else {
        message["content"] = json!(full_text);
    }

    if let Some(r) = reasoning
        && !r.is_empty()
    {
        message["reasoning_content"] = json!(r);
    }

    let determined_finish = if let Some(fr) = finish_reason {
        fr.to_string()
    } else if has_tools {
        "tool_calls".to_string()
    } else {
        "stop".to_string()
    };

    if let Some(tc) = tool_calls
        && !tc.is_empty()
    {
        message["tool_calls"] = json!(tc);
    }

    json!({
        "id": id,
        "object": "chat.completion",
        "created": now,
        "model": model,
        "choices": [
            {
                "index": 0,
                "message": message,
                "finish_reason": determined_finish
            }
        ],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0
        }
    })
}
