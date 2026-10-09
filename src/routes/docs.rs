use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Serves the interactive Scalar API documentation UI.
pub async fn scalar_docs_handler() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html>
  <head>
    <title>Rudra Proxy - API Reference</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
      body {
        margin: 0;
        padding: 0;
      }
    </style>
  </head>
  <body>
    <script
      id="api-reference"
      data-url="/openapi.json"
      data-configuration='{"theme":"purple","layout":"modern"}'></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
  </body>
</html>"#,
    )
}

/// Serves the OpenAPI 3.1 schema definition for Rudra Proxy.
pub async fn openapi_spec_handler() -> Response {
    let spec = json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Rudra Proxy API",
            "version": "0.1.0",
            "description": "Universal AI Proxy Bridge & Drop-in Replacement for OpenAI, Ollama, and TypeSafe AI System One APIs, backed by OpenCode Zen's free frontier models."
        },
        "servers": [
            {
                "url": "http://localhost:11434",
                "description": "Local Rudra Proxy server"
            }
        ],
        "paths": {
            "/v1/chat/completions": {
                "post": {
                    "summary": "Create chat completion",
                    "description": "Executes streaming SSE or standard JSON chat completion across OpenCode Zen free models with automatic protocol dispatch (Chat Completions, Responses API, and System One).",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/ChatCompletionRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Successful completion (or Server-Sent Events stream when stream=true)"
                        }
                    }
                }
            },
            "/v1/models": {
                "get": {
                    "summary": "List available models",
                    "description": "Returns list of active OpenCode Zen models and user aliases in OpenAI-compatible format.",
                    "responses": {
                        "200": {
                            "description": "Model catalog list"
                        }
                    }
                }
            },
            "/v1/models/{model}": {
                "get": {
                    "summary": "Retrieve model metadata",
                    "parameters": [
                        {
                            "name": "model",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Model metadata object"
                        }
                    }
                }
            },
            "/v1/models/sync": {
                "post": {
                    "summary": "Trigger model catalog synchronization",
                    "description": "Immediately reloads active models and metadata from OpenCode Zen gateway.",
                    "responses": {
                        "200": {
                            "description": "Catalog reload status"
                        }
                    }
                }
            },
            "/v1/systemone": {
                "post": {
                    "summary": "TypeSafe AI System One decision engine",
                    "description": "Direct evaluation endpoint for structured questions (noul, choice, score) on jev-1.13.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["state", "questions"],
                                    "properties": {
                                        "model": { "type": "string", "default": "jev-1.13-free" },
                                        "state": { "type": "object" },
                                        "questions": { "type": "object" }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Decision evaluation results"
                        }
                    }
                }
            },
            "/api/chat": {
                "post": {
                    "summary": "Ollama chat endpoint",
                    "description": "Executes NDJSON streaming chat completions compatible with Ollama CLI and Open WebUI.",
                    "responses": {
                        "200": { "description": "Ollama chat stream" }
                    }
                }
            },
            "/api/generate": {
                "post": {
                    "summary": "Ollama generate endpoint",
                    "description": "Translates prompt completions to chat completions in Ollama format.",
                    "responses": {
                        "200": { "description": "Ollama generate response" }
                    }
                }
            },
            "/api/tags": {
                "get": {
                    "summary": "List Ollama models",
                    "description": "Returns available models formatted as Ollama tags.",
                    "responses": {
                        "200": { "description": "Tags list" }
                    }
                }
            },
            "/api/version": {
                "get": {
                    "summary": "Ollama version check",
                    "responses": {
                        "200": { "description": "Version object" }
                    }
                }
            },
            "/health": {
                "get": {
                    "summary": "Service health check",
                    "responses": {
                        "200": { "description": "Health status" }
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "ChatCompletionRequest": {
                    "type": "object",
                    "required": ["messages"],
                    "properties": {
                        "model": {
                            "type": "string",
                            "default": "mimo-v2.6-flash-free",
                            "description": "Model ID or configured alias (e.g. 'default', 'mimo-v2.6-flash-free', 'muse-spark-1.3-contributor-free', 'nemotron-3-ultra-free', 'space-bunny-free')."
                        },
                        "messages": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["role", "content"],
                                "properties": {
                                    "role": { "type": "string", "enum": ["system", "user", "assistant", "tool"] },
                                    "content": { "type": "string" }
                                }
                            },
                            "description": "Conversation message history."
                        },
                        "stream": {
                            "type": "boolean",
                            "default": false,
                            "description": "Whether to stream back partial message deltas via Server-Sent Events (SSE)."
                        },
                        "temperature": {
                            "type": "number",
                            "minimum": 0.0,
                            "maximum": 2.0,
                            "description": "Sampling temperature. Higher values make output more random, lower values more focused and deterministic."
                        },
                        "top_p": {
                            "type": "number",
                            "minimum": 0.0,
                            "maximum": 1.0,
                            "description": "Nucleus sampling probability mass threshold."
                        },
                        "max_tokens": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Maximum number of tokens to generate."
                        },
                        "max_completion_tokens": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Modern OpenAI specification parameter for max output tokens (takes precedence over max_tokens)."
                        },
                        "stop": {
                            "oneOf": [
                                { "type": "string" },
                                { "type": "array", "items": { "type": "string" } }
                            ],
                            "description": "Up to 4 sequences where the API will stop generating further tokens."
                        },
                        "presence_penalty": {
                            "type": "number",
                            "minimum": -2.0,
                            "maximum": 2.0,
                            "description": "Penalizes new tokens based on whether they appear in the text so far."
                        },
                        "frequency_penalty": {
                            "type": "number",
                            "minimum": -2.0,
                            "maximum": 2.0,
                            "description": "Penalizes new tokens based on their existing frequency in the text so far."
                        },
                        "seed": {
                            "type": "integer",
                            "description": "If specified, upstream sampling will make a best effort to sample deterministically."
                        },
                        "response_format": {
                            "type": "object",
                            "description": "Structured output specification (e.g. {'type': 'json_object'})."
                        },
                        "reasoning_effort": {
                            "type": "string",
                            "enum": ["minimal", "low", "medium", "high", "xhigh"],
                            "description": "Constrains reasoning effort for thinking models (muse-*, space-bunny-free, step-5, exo-free)."
                        },
                        "tools": {
                            "type": "array",
                            "items": { "type": "object" },
                            "description": "List of client tools available to the model."
                        },
                        "tool_choice": {
                            "description": "Controls which (if any) tool is called by the model ('none', 'auto', 'required', or tool object)."
                        },
                        "user": {
                            "type": "string",
                            "description": "Unique identifier representing your end-user to help monitor and detect abuse."
                        }
                    }
                }
            }
        }
    });

    Json(spec).into_response()
}
