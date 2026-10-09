use crate::registry::metadata::{ModelMetadata, ModelProtocol};
use crate::upstream::effort::apply_reasoning;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
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
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<Value>>,
    #[serde(default)]
    pub tool_choice: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn default_model() -> String {
    "mimo-v2.6-flash-free".to_string()
}

/// Builds the OpenCode Zen request payload appropriate for the model's protocol,
/// injecting mandatory dummy tools and proper reasoning configurations.
pub fn build_opencode_payload(
    req: &OpenAiChatRequest,
    model: &ModelMetadata,
    override_effort: Option<&str>,
) -> Value {
    let client_effort = req.reasoning_effort.as_deref();

    match model.protocol {
        ModelProtocol::Responses => {
            // Mandatory dummy tools for Responses API
            let dummy_bash = json!({
                "type": "function",
                "name": "bash",
                "description": "bash",
                "parameters": { "type": "object", "properties": {} }
            });
            let dummy_read = json!({
                "type": "function",
                "name": "read",
                "description": "read",
                "parameters": { "type": "object", "properties": {} }
            });

            let mut tools = vec![dummy_bash, dummy_read];
            if let Some(ref client_tools) = req.tools {
                for t in client_tools {
                    // Avoid duplicating bash or read
                    let name = t.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    if name != "bash" && name != "read" {
                        tools.push(t.clone());
                    }
                }
            }

            let mut payload = json!({
                "model": model.id,
                "input": req.messages,
                "stream": true, // Zen gateway strictly requires stream: true
                "tools": tools,
            });

            if let Some(temp) = req.temperature {
                payload["temperature"] = json!(temp);
            }
            if let Some(top_p) = req.top_p {
                payload["top_p"] = json!(top_p);
            }
            if let Some(max) = req.max_tokens {
                payload["max_tokens"] = json!(max);
            }

            apply_reasoning(&mut payload, model, client_effort, override_effort);
            payload
        }
        ModelProtocol::ChatCompletions => {
            // Mandatory dummy tools for Chat Completions API
            let dummy_bash = json!({
                "type": "function",
                "function": {
                    "name": "bash",
                    "description": "bash",
                    "parameters": { "type": "object", "properties": {} }
                }
            });
            let dummy_read = json!({
                "type": "function",
                "function": {
                    "name": "read",
                    "description": "read",
                    "parameters": { "type": "object", "properties": {} }
                }
            });

            let mut tools = vec![dummy_bash, dummy_read];
            if let Some(ref client_tools) = req.tools {
                for t in client_tools {
                    let name = t
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    if name != "bash" && name != "read" {
                        tools.push(t.clone());
                    }
                }
            }

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
            if let Some(max) = req.max_tokens {
                payload["max_tokens"] = json!(max);
            }

            apply_reasoning(&mut payload, model, client_effort, override_effort);
            payload
        }
        ModelProtocol::SystemOne => {
            json!({
                "model": model.id,
                "messages": req.messages,
                "stream": true,
            })
        }
    }
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

/// Creates the terminal OpenAI `chat.completion.chunk` SSE event string.
pub fn make_openai_terminal_chunk(id: &str, model: &str) -> String {
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
                "finish_reason": "stop"
            }
        ]
    });

    format!("data: {chunk}\ndata: [DONE]\n\n")
}

/// Creates a standard OpenAI non-streaming `chat.completion` response JSON object.
pub fn make_openai_completion(
    id: &str,
    model: &str,
    full_text: &str,
    reasoning: Option<&str>,
) -> Value {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut message = json!({
        "role": "assistant",
        "content": full_text
    });

    if let Some(r) = reasoning
        && !r.is_empty()
    {
        message["reasoning_content"] = json!(r);
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
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0
        }
    })
}
