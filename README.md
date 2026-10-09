<div align="center">

<p align="center">
  <svg width="120" height="120" viewBox="0 0 100 100" fill="none" xmlns="http://www.w3.org/2000/svg">
    <!-- Outer Octagon Shield -->
    <polygon points="50,4 85,18 96,50 85,82 50,96 15,82 4,50 15,18" stroke="#30363d" stroke-width="2" fill="#0d1117"/>
    <!-- Cyber Geometric Lattice -->
    <line x1="50" y1="4" x2="50" y2="96" stroke="#21262d" stroke-width="1.5"/>
    <line x1="4" y1="50" x2="96" y2="50" stroke="#21262d" stroke-width="1.5"/>
    <polygon points="50,15 85,50 50,85 15,50" stroke="#1f6feb" stroke-width="2" fill="none" opacity="0.6"/>
    <!-- Inner Core Nexus -->
    <polygon points="50,25 75,50 50,75 25,50" stroke="#58a6ff" stroke-width="3" fill="#161b22"/>
    <circle cx="50" cy="50" r="10" fill="#238636" stroke="#3fb950" stroke-width="2.5"/>
    <circle cx="50" cy="50" r="4" fill="#58a6ff"/>
    <!-- Glowing Orbital Nodes -->
    <circle cx="50" cy="15" r="3" fill="#58a6ff"/>
    <circle cx="85" cy="50" r="3" fill="#58a6ff"/>
    <circle cx="50" cy="85" r="3" fill="#58a6ff"/>
    <circle cx="15" cy="50" r="3" fill="#58a6ff"/>
  </svg>
</p>

# Rudra

### Universal AI proxy bridge routing to free frontier AI models for OpenAI and Ollama clients.

**Keyless contributor tier access. Verified frontier models with up to 1M context. Zero configuration.**

[![Gateway](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fraw.githubusercontent.com%2FDari-OS%2Frudra-proxy%2Fgh-pages%2Fstatus.json&query=%24.status&label=gateway&color=3fb950&style=flat-square)](https://dari-os.github.io/rudra-proxy/)
[![Active Models](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fraw.githubusercontent.com%2FDari-OS%2Frudra-proxy%2Fgh-pages%2Fstatus.json&query=%24.models_count&label=models&color=58a6ff&style=flat-square)](https://dari-os.github.io/rudra-proxy/)
[![CI](https://img.shields.io/github/actions/workflow/status/dari-os/rudra-proxy/healthcheck.yml?branch=main&label=ci&style=flat-square)](https://github.com/dari-os/rudra-proxy/actions/workflows/healthcheck.yml)
[![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-dea584?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Default Port](https://img.shields.io/badge/port-11434-79c0ff?style=flat-square)](https://ollama.com/)
[![License MIT](https://img.shields.io/badge/license-MIT-f4c430?style=flat-square)](LICENSE)

[Model Catalog](#model-catalog) · [Quickstart](#quickstart) · [Architecture](#architecture) · [Client Setup](#client-setup) · [CLI Reference](#cli-reference) · [Configuration](#configuration) · [Deployment](#deployment)

</div>

---

Rudra is a lightweight, high-performance proxy bridge written in Rust. It transparently routes standard OpenAI (`/v1`) and Ollama (`/api`) requests to OpenCode Zen's public contributor tier, providing keyless access to free frontier AI models without requiring API keys, paid accounts, or downstream code modifications.

Downstream clients—including Open WebUI, Cursor, Continue.dev, Aider, Hermes, and the official OpenAI Python SDK—can access free cloud LLM compute instantly.

### Core Capabilities

- **Authentication-Free Inference:** Routes requests through OpenCode Zen's contributor tier without requiring personal API tokens or credit cards.
- **Drop-in Wire Compatibility:** Exposes standard OpenAI Chat Completions, Responses API, Ollama (`/api/chat`, `/api/tags`, `/api/generate`), and TypeSafe AI System One decision endpoints.
- **Automated Upstream Compliance:** Handles client header spoofing, computes valid timestamp-encoded `ses_...` session identifiers, injects mandatory dummy tools (`bash`, `read`), and forces upstream streaming.
- **Dynamic Model Discovery:** Synchronizes active model manifests directly from OpenCode Zen with an embedded baseline catalog for offline reliability.
- **Outbound Proxy Pool:** Optional HTTP and SOCKS5 proxy chaining with atomic round-robin rotation, configurable request thresholds, and per-proxy quotas.
- **Reasoning Effort Control:** Maps downstream reasoning configurations to upstream models, supporting discrete effort levels, boolean toggles, and interleaved thought extraction.

---

## Model Catalog

The catalog below is dynamically monitored by our CI/CD healthcheck worker, sending live inference test prompts to verify responsiveness and record real response latencies and generation speed:

<!-- MODEL_TABLE_START -->
| Model Identifier | Provider | Protocol | Reasoning | Context Window | Status | Latency | Speed |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `big-pickle` | big-pickle | `chat_completions` | none | 200k | online | 2262 ms | 101.6 tok/s |
| `exo-free` | exo | `chat_completions` | effort (1) | 1048k | error (410 Gone) | - | - |
| `jev-1.13-free` | jev | `systemone` | none | 131k | online | 480 ms | 41.7 tok/s |
| `ling-3.0-flash-fin-free` | ling | `chat_completions` | toggle | 262k | error (400 Bad Request) | - | - |
| `ling-3.1-flash-free` | ling | `chat_completions` | toggle | 262k | online | 1097 ms | 48.5 tok/s |
| `longcat-2.5-preview-free` | longcat | `chat_completions` | toggle | 1000k | online | 1452 ms | 3.7 tok/s |
| `mimo-v2.6-flash-free` | mimo | `chat_completions` | none | 200k | online | 1551 ms | 18.4 tok/s |
| `muse-spark-1.2-contributor-free` | muse-free | `responses` | effort (5) | 1048k | online | 3761 ms | 24.4 tok/s |
| `muse-spark-1.3-contributor-free` | muse-free | `responses` | effort (5) | 1048k | online | 13250 ms | 18.5 tok/s |
| `nemotron-3-ultra-free` | nemotron-free | `chat_completions` | none | 1000k | online | 885 ms | 2.1 tok/s |
| `nemotron-3.5-lightning-free` | nemotron-free | `chat_completions` | none | 262k | online | 411 ms | 7.5 tok/s |
| `space-bunny-free` | space-bunny | `chat_completions` | effort (5) | 1048k | online | 658 ms | 66.7 tok/s |
| `step-5-preview-free` | step | `chat_completions` | effort (3) | 1000k | online | 4522 ms | 23.0 tok/s |
<!-- MODEL_TABLE_END -->

---

## Architecture

```
Downstream Clients (Open WebUI, Cursor, Continue.dev, Aider, Python SDK)
         │
         │ OpenAI: /v1/*  |  Ollama: /api/*  |  System One: /systemone
         ▼
    rudra-proxy (Listening on localhost:11434)
   ┌────────────────────────────────────────────────────────┐
   │ Header Spoofing • Session Rotation • Protocol Dispatch │
   │ Mandatory Dummy Tools • Reasoning Negotiation          │
   └────────────────────────────────────────────────────────┘
         │
         │ Direct connection or via HTTP/SOCKS5 Proxy Pool
         ▼
OpenCode Zen Gateway
   ├── Responses API        (muse-* models)
   ├── Chat Completions API (mimo, nemotron, bunny, step, exo, ling, longcat)
   └── System One API       (jev-1.13, jev-1.13-free)
```

---

## Quickstart

### Option A: Build and Run with Cargo

```bash
# Clone the repository
git clone https://github.com/dari-os/rudra-proxy.git
cd rudra-proxy

# Build and start the server (default port: 11434)
cargo run --release -- serve
```

### Option B: Run with Docker Compose

```bash
# Clone the repository
git clone https://github.com/dari-os/rudra-proxy.git
cd rudra-proxy

# Build and start the container in background
docker compose up -d
```

### Option C: Run Pre-built Container (GHCR)

Run instantly without local compilation:

```bash
docker run -d --name rudra-proxy -p 11434:11434 ghcr.io/dari-os/rudra-proxy:latest
```

### Option D: Manual Docker Build

```bash
docker build -t rudra-proxy .
docker run -d --name rudra-proxy -p 11434:11434 rudra-proxy
```

### Connectivity Verification

Run the built-in diagnostic suite to confirm port availability, network reachability, and session encoding integrity:

```bash
# Via Cargo
cargo run --release -- doctor

# Via Docker Compose
docker compose run --rm rudra-proxy doctor
```

```text
=== Rudra Health & Connectivity Doctor ===

[✓] Port 11434 is available for binding on 127.0.0.1
[✓] OpenCode Zen gateway reachable at https://opencode.ai/zen/v1/models (142ms, HTTP 200 OK)
[✓] OpenCode catalog metadata reachable at https://models.opencode.ai/api.json (88ms)
[✓] Session ID generator valid (generated: ses_..., length: 30 chars, timestamp encoded)

=== Doctor Summary: All essential systems operational! ===
```

---

## Client Setup

Because Rudra implements standard OpenAI and Ollama protocol interfaces, downstream integration requires only pointing client base URLs to `localhost:11434`.

### Open WebUI

1. Open **Settings** > **Connections**.
2. Under **Ollama API**, ensure the base URL is set to `http://localhost:11434` (or `http://host.docker.internal:11434` when running Open WebUI in a container).
3. Save settings. All active models populate the model selection interface automatically.

### Cursor / Continue.dev / Windsurf

Configure a custom OpenAI-compatible provider:

- **Base URL:** `http://localhost:11434/v1`
- **API Key:** `public` (or any non-empty string)
- **Model:** `default` or `mimo-v2.6-flash-free`

### Aider

```bash
aider --openai-api-base http://localhost:11434/v1 --openai-api-key public --model default
```

### Python (Official OpenAI SDK)

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:11434/v1",
    api_key="public"
)

response = client.chat.completions.create(
    model="default",
    messages=[{"role": "user", "content": "Explain atomic memory ordering in Rust."}],
    stream=True
)

for chunk in response:
    content = chunk.choices[0].delta.content or ""
    print(content, end="", flush=True)
```

### Terminal REPL

Query models directly from your terminal with conversation history and memory:

```bash
# Interactive multi-turn REPL
rudra run default

# One-shot command line evaluation
rudra run default "Explain how atomic operations work in Rust in 3 sentences."
```

---

## Tool Calling & Agent Harness Support

Rudra functions as a drop-in proxy bridge for autonomous agent harnesses including Claude Code, Hermes Agent, Pi harness, Codex, OpenCode, and Aider.

### Architecture & Responsibility Separation

- **Harness-Driven Execution:** The downstream harness defines tools and executes them locally on your workstation (running shell commands, modifying workspace files, or invoking web APIs). Rudra does not execute tools; it transparently negotiates and bridges tool schemas and calls between the harness and the models.
- **Harness Tool Schema Preservation:** When a harness submits tools (`tools: [...]`), Rudra preserves the harness's exact tool names, descriptions, and JSON parameter schemas without modification.
- **Gateway Validation Guard:** The OpenCode Zen gateway requires contributor-tier requests to declare tool definitions for `bash` and `read`. If a client makes a conversational request without tools (or provides custom tools that omit `bash` or `read`), Rudra automatically injects dummy definitions upstream so the gateway never rejects requests with `FreeTierError`.
- **Streaming & Non-Streaming Support:** Delivers real-time `delta.tool_calls` chunks when streaming (`stream: true`), and aggregates complete `message.tool_calls` objects with `finish_reason: "tool_calls"` when non-streaming (`stream: false`).
- **Multi-Turn Continuity:** When your harness executes a tool and returns the result in `{ role: "tool", tool_call_id: "...", content: "..." }`, Rudra transparently relays the conversation history upstream across all models (including automatic translation into `function_call_output` wire items for `muse-*` models on the Responses API).

### Tool Calling Example (Python SDK)

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:11434/v1",
    api_key="public"
)

tools = [{
    "type": "function",
    "function": {
        "name": "get_weather",
        "description": "Get current weather in a city",
        "parameters": {
            "type": "object",
            "properties": {"city": {"type": "string"}},
            "required": ["city"],
        },
    },
}]

response = client.chat.completions.create(
    model="default",
    messages=[{"role": "user", "content": "What is the weather in Tokyo?"}],
    tools=tools,
)

tool_call = response.choices[0].message.tool_calls[0]
print(f"Tool: {tool_call.function.name}, Arguments: {tool_call.function.arguments}")
```

---

## Inference & Sampling Parameters

Rudra implements the full OpenAI chat completion specification and seamlessly translates parameters across both standard LLMs and Responses API models:

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `temperature` | `float` | `null` (provider default) | Sampling temperature between `0.0` and `2.0`. Lower values are more deterministic. |
| `top_p` | `float` | `null` | Nucleus sampling probability threshold (`0.0` to `1.0`). |
| `max_tokens` | `integer` | `null` (unconstrained) | Upper bound on generated tokens. Model stops with `finish_reason: "length"`. |
| `max_completion_tokens` | `integer` | `null` | Modern OpenAI replacement for `max_tokens` (takes priority when present). |
| `stop` | `string` \| `string[]` | `null` | Stop sequence(s) where token generation halts with `finish_reason: "stop"`. |
| `presence_penalty` | `float` | `0.0` | Penalizes new tokens based on presence in text (`-2.0` to `2.0`). |
| `frequency_penalty` | `float` | `0.0` | Penalizes new tokens based on existing frequency (`-2.0` to `2.0`). |
| `seed` | `integer` | `null` | Best-effort deterministic pseudo-random sampling seed. |
| `response_format` | `object` | `null` | Enforces structured output format, such as `{"type": "json_object"}`. |
| `reasoning_effort` | `string` | `null` | Constrains thinking effort (`minimal`, `low`, `medium`, `high`, `xhigh`). |
| `tools` | `object[]` | `null` | Client-declared function schemas (automatically blended with gateway tools). |
| `tool_choice` | `string` \| `object` | `null` | Controls tool selection behavior (`"auto"`, `"none"`, or specific tool). |
| `user` | `string` | `null` | End-user identifier string for downstream tracking. |

> **Protocol Note:** For `muse-*` models utilizing the OpenAI Responses API, Rudra automatically translates `max_tokens` / `max_completion_tokens` into `"max_output_tokens"` and guarantees the upstream minimum floor of `16` to prevent schema rejection.

---

## Interactive API Documentation (Scalar)

You can explore the full OpenAPI 3.1 specification, parameter schemas, and request examples in two ways:

- **Online API Reference (GitHub Pages):** [`https://dari-os.github.io/rudra-proxy/docs/`](https://dari-os.github.io/rudra-proxy/docs/)  
  *Static reference viewer without needing a running backend. Testing buttons are disabled.*
- **Local Interactive UI (When server is running):** [`http://localhost:11434/docs`](http://localhost:11434/docs)  
- **OpenAPI 3.1 JSON Specification:** [`http://localhost:11434/openapi.json`](http://localhost:11434/openapi.json)

---

## CLI Reference

The `rudra` binary provides operational subcommands for server hosting, diagnostic testing, model querying, and configuration management:

### `rudra serve`

Starts the proxy server daemon.

```bash
rudra serve [OPTIONS]
```

- `--port <PORT>`: Socket bind port (default: `11434`).
- `--host <HOST>`: Socket bind address (default: `0.0.0.0`).
- `--cors <ORIGIN>`: Allowed CORS origin (default: `*`).
- `--config <PATH>`: Path to local `rudra.toml` configuration file.
- `--workers <NUM>`: Tokio runtime worker thread pool size.
- `--timeout <SECS>`: Upstream HTTP request timeout in seconds (default: `120`).
- `--log-level <LEVEL>`: Log filter level (`trace`, `debug`, `info`, `warn`, `error`).
- `--dry-run`: Validates configuration, catalog cache, and socket binding without listening.
- `--proxy-enabled`: Activates the outbound forward proxy pool.
- `--proxy <URL>...`: Forward proxy addresses (`http://`, `socks5://`).
- `--proxy-switch-requests <N>`: Global request threshold before rotating to the next proxy node.
- `--session-rotate-requests <N>`: Frequency of `ses_...` session ID regeneration (default: `10`).

### `rudra list`

Inspects registered models and evaluates upstream network status.

```bash
# Print formatted terminal table
rudra list

# Actively probe upstream Zen gateway response latencies
rudra list --live

# Output structured markdown or JSON
rudra list --live --format markdown
rudra list --live --format json
```

### `rudra run`

Executes prompts directly against the upstream gateway.

```bash
# Start an interactive multi-turn REPL
rudra run default

# Run with explicit reasoning effort
rudra run space-bunny-free --reasoning-effort high

# Non-interactive command-line query
rudra run default "Explain how atomic operations work in Rust in 3 sentences."
```

### `rudra doctor`

Runs an automated connectivity and health check across socket binding, upstream gateways, catalog metadata, and session generator encoding.

```bash
rudra doctor
```

### `rudra bench`

Evaluates generation throughput (tokens/second) and Time-To-First-Token (TTFT) for a specified model.

```bash
rudra bench space-bunny-free --prompt "Summarize the Raft consensus protocol."
```

### `rudra config`

Inspects and updates `rudra.toml` non-destructively from the command line.

```bash
# Read configuration values
rudra config get port
rudra config get aliases.default

# Modify configuration values
rudra config set port 11435
rudra config set overrides.space-bunny-free.reasoning_effort high

# Manage forward proxy pool nodes
rudra config add-proxy socks5://127.0.0.1:9050 --requests 50
rudra config remove-proxy socks5://127.0.0.1:9050
```

---

## Configuration

Rudra looks for a `rudra.toml` file in the current working directory, or at the path provided to `--config`. A template is provided in `rudra.toml.example`.

```toml
# Network settings
host = "0.0.0.0"
port = 11434
cors = "*"
timeout_secs = 120
log_level = "info"

# Model aliases: map user-friendly identifiers to target models
[aliases]
"default" = "mimo-v2.6-flash-free"
"jev" = "jev-1.13-free"

# Fine-tune reasoning effort per model
[overrides."space-bunny-free"]
reasoning_effort = "high"

# Outbound forward proxy pool (HTTP / SOCKS5)
[proxy]
enabled = false
switch_after_requests = 100
proxies = [
    # "http://127.0.0.1:8080",
    # "socks5://127.0.0.1:9050",
]

# Session rotation: renews ses_... ID every N requests
[session]
rotate_after_requests = 10
```

---

## Deployment

### Docker Run

```bash
docker build -t rudra-proxy .
docker run -d \
  --name rudra-proxy \
  -p 11434:11434 \
  --restart unless-stopped \
  rudra-proxy
```

### Docker Compose

```yaml
services:
  rudra-proxy:
    build:
      context: .
      dockerfile: Dockerfile
    image: rudra-proxy:latest
    pull_policy: build
    container_name: rudra-proxy
    restart: unless-stopped
    ports:
      - "11434:11434"
    environment:
      - RUDRA_HOST=0.0.0.0
      - RUDRA_PORT=11434
      - RUDRA_SYNC_INTERVAL_MINS=30
    # Optional: mount custom configuration if created
    # volumes:
    #   - ./rudra.toml:/app/rudra.toml:ro
```

---

## Disclaimer

Rudra is an open-source experimental research project developed for protocol interoperability testing and educational research. Please refer to [DISCLAIMER.md](DISCLAIMER.md) for statutory terms, limitations of liability, and third-party terms of service compliance requirements.
