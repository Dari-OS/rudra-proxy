<p align="center">
  <svg width="140" height="140" viewBox="0 0 100 100" fill="none" xmlns="http://www.w3.org/2000/svg">
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

<h1 align="center">Rudra Proxy</h1>

<p align="center">
  <strong>Free, Keyless AI Bridge & Universal Drop-in Replacement for OpenAI & Ollama</strong><br>
  <em>100% Free • No API Keys • No Credit Cards • 13 Frontier Models • 1M Context Windows • Drop-in for Any AI App</em>
</p>

<p align="center">
  <a href="https://github.com/dari-os/rudra-proxy/actions/workflows/healthcheck.yml"><img src="https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fdari-os.github.io%2Frudra-proxy%2Fstatus.json&query=%24.status&label=Zen%20Gateway&color=3fb950&style=flat-square" alt="Gateway Status"></a>
  <a href="https://github.com/dari-os/rudra-proxy/actions/workflows/healthcheck.yml"><img src="https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fdari-os.github.io%2Frudra-proxy%2Fstatus.json&query=%24.models_count&label=Active%20Models&color=58a6ff&style=flat-square" alt="Active Models"></a>
  <img src="https://img.shields.io/badge/Cost-100%25%20Free-success?style=flat-square" alt="100% Free">
  <img src="https://img.shields.io/badge/Port-11434-79c0ff?style=flat-square" alt="Port 11434">
  <img src="https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square" alt="License MIT">
</p>

---

> [!IMPORTANT]
> ### 🎁 Yes, This is 100% Completely Free AI!
> **No subscriptions. No API keys. No credit cards. No waiting lists.**  
> `rudra-proxy` bridges OpenCode Zen's open contributor infrastructure directly to your local computer. It gives you unrestricted, high-speed access to **13 cutting-edge models** (including massive 1,000,000+ token context windows, deep reasoning engines, NVIDIA Nemotron, and TypeSafe AI System One).  
> It acts as a local drop-in clone of OpenAI (`/v1`) and Ollama (`/api`), meaning **virtually every AI app on earth**—from **Open WebUI** and **Cursor** to **Continue.dev**, **Hermes**, **Aider**, and the official **OpenAI Python library**—works right out of the box with zero code changes.

---

## 💡 How Does It Work?

OpenCode Zen hosts an authentication-free contributor tier providing free cloud GPU compute for developer tooling. However, their gateway verifies internal client signatures, header structures, Unix timestamp encodings, and tool parameters to prevent non-compliant clients.

`rudra-proxy` runs silently on your machine on port `11434` (Ollama's default port):
1. Your tools connect to `http://localhost:11434` thinking it's standard OpenAI or Ollama.
2. `rudra-proxy` transparently injects legitimate headers, generates authenticated `ses_...` session timestamps, handles protocol translation (Responses API, Chat Completions, or TypeSafe AI System One), and injects mandatory dummy tools behind the scenes.
3. You get free, lightning-fast cloud LLM inference without burning through your wallet or running heavy local weights.

```
┌────────────────────────────────────────────────────────────────────────┐
│ Your Apps (Open WebUI, Cursor, Continue, Hermes, Aider, Python, etc.)  │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ OpenAI: /v1/*  |  Ollama: /api/*
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│               rudra-proxy (Listening on localhost:11434)               │
│    Header Spoofing • Session Rotation • Protocol Dispatch • Tools      │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Direct or via SOCKS5/HTTP Proxy Pool
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                   OpenCode Zen Gateway (Free AI Tier)                  │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 🌟 Super Easy Setup Guide (3 Minutes)

Anyone can get up and running with free AI in 3 simple steps:

### Step 1: Start Rudra

Choose whichever method you prefer:

#### Option A: Run with Cargo (Native & Fastest)
```bash
# 1. Clone and enter the folder
git clone https://github.com/dari-os/rudra-proxy.git
cd rudra-proxy

# 2. Build and launch (default port: 11434)
cargo run --release -- serve
```

#### Option B: Run with Docker
```bash
# 1. Clone and enter folder
git clone https://github.com/dari-os/rudra-proxy.git
cd rudra-proxy

# 2. Build and start container in background
docker build -t rudra-proxy .
docker run -d --name rudra-proxy -p 11434:11434 rudra-proxy
```

*(Optional)* Run the built-in diagnostic doctor to verify everything is reachable:
```bash
cargo run --release -- doctor
```

---

### Step 2: Connect Your Favorite App

Because `rudra-proxy` speaks standard OpenAI and Ollama protocols, setting it up in your favorite tools takes seconds:

#### 💬 Open WebUI
1. Go to **Settings** $\rightarrow$ **Connections**.
2. Under **Ollama API**, ensure URL is set to `http://localhost:11434` (or `http://host.docker.internal:11434` if Open WebUI is in Docker).
3. Click **Save** — all 13 free models will immediately populate your model picker!

#### 💻 Cursor / Continue.dev / Windsurf
1. Open settings / configuration.
2. Select **OpenAI Compatible**.
3. **Base URL**: `http://localhost:11434/v1`
4. **API Key**: `public` (or any dummy text)
5. **Model**: `default` or `mimo-v2.6-flash-free`

#### 🤖 Aider
```bash
aider --openai-api-base http://localhost:11434/v1 --openai-api-key public --model default
```

#### 🐍 Python (Standard OpenAI SDK)
```python
from openai import OpenAI

# Connect directly to your local rudra proxy
client = OpenAI(
    base_url="http://localhost:11434/v1",
    api_key="public"  # No real key needed!
)

response = client.chat.completions.create(
    model="default",  # Routes to mimo-v2.6-flash-free
    messages=[{"role": "user", "content": "Write a Python script to reverse a string."}],
    stream=True
)

for chunk in response:
    content = chunk.choices[0].delta.content or ""
    print(content, end="", flush=True)
```

#### ⚡ Terminal REPL (Zero Tools Needed!)
You can also chat directly from your terminal with conversation history:
```bash
./target/release/rudra run default
# rudra> Hello! What models do you offer?
# rudra> /clear (clears history)
# rudra> exit
```

---

## 🤖 13 Verified Free Models (Included Out of the Box)

All 13 models below are completely free to query:

| Model Identifier | Provider | Architecture | Context Window | Best Suited For |
|---|---|---|---|---|
| `muse-spark-1.3-contributor-free` | Muse | Responses | **1,048,576 tokens** | Massive codebase refactoring & long document analysis |
| `muse-spark-1.2-contributor-free` | Muse | Responses | **1,048,576 tokens** | 1M context analysis with deep reasoning traces |
| `mimo-v2.6-flash-free` | Mimo | Chat | **262,144 tokens** | Ultra-fast everyday coding, agentic reasoning & chat |
| `nemotron-3-ultra-free` | NVIDIA | Chat | **131,072 tokens** | Complex reasoning, mathematics & code generation |
| `nemotron-3.5-lightning-free` | NVIDIA | Chat | **131,072 tokens** | High-throughput coding & fast instruction following |
| `space-bunny-free` | BunnyAI | Chat | **131,072 tokens** | Configurable reasoning effort (low to xhigh) |
| `step-5-preview-free` | Step | Chat | **131,072 tokens** | Multistep problem solving & reasoning |
| `exo-free` | Exo | Chat | **131,072 tokens** | Deep high-effort reasoning traces |
| `longcat-2.5-preview-free` | LongCat | Chat | **262,144 tokens** | Extended context workflows |
| `ling-3.0-flash-fin-free` | Ling | Chat | **131,072 tokens** | Fast reasoning with financial & logical domain tuning |
| `ling-3.1-flash-free` | Ling | Chat | **131,072 tokens** | General low-latency conversational tasks |
| `jev-1.13-free` | TypeSafe AI | System One | **131,072 tokens** | Structured classification, scoring & decision engine |
| `jev-1.13` | TypeSafe AI | System One | **131,072 tokens** | TypeSafe AI production decision model |

---

## 🛠️ Complete CLI Command Reference

The `rudra` binary provides powerful operational and debugging tools:

### 1. `rudra serve`
Runs the proxy server daemon.

```bash
rudra serve [OPTIONS]
```

* `--port <PORT>`: Listening port (default: `11434`).
* `--host <HOST>`: Bind address (default: `0.0.0.0`).
* `--cors <ORIGIN>`: CORS policy (default: `*`).
* `--config <PATH>`: Path to `rudra.toml` config file.
* `--dry-run`: Validates config, catalog loading, and port availability without starting the listener.
* `--proxy-enabled`: Turns on the outbound forward proxy pool.
* `--proxy <URL>...`: Forward proxy addresses (`http://`, `socks5://`).
* `--proxy-switch-requests <N>`: Switch to the next proxy after every $N$ requests.
* `--session-rotate-requests <N>`: Generate a fresh authenticated session ID after every $N$ requests.

### 2. `rudra list`
Inspects active models and tests live network reachability:

```bash
# Aligned terminal table
rudra list

# Actively probe upstream Zen gateway latency
rudra list --live

# Format output as Markdown
rudra list --live --format markdown

# Format output as JSON (used by CI/CD healthchecks)
rudra list --live --format json
```

### 3. `rudra run`
Direct execution from your terminal without opening a browser or running a separate client:

```bash
# One-shot command line prompt
rudra run default "Explain how atomic operations work in Rust in 3 sentences."

# Interactive terminal session (REPL)
rudra run space-bunny-free --reasoning-effort high
```

### 4. `rudra doctor`
Automatic diagnostic system check:

```bash
rudra doctor
```

```text
=== Rudra Health & Connectivity Doctor ===

[✓] Port 11434 is available for binding on 127.0.0.1
[✓] OpenCode Zen gateway reachable at https://opencode.ai/zen/v1/models (142ms, HTTP 200 OK)
[✓] OpenCode catalog metadata reachable at https://models.opencode.ai/api.json (88ms)
[✓] Session ID generator valid (generated: ses_11e02..., length: 30 chars, timestamp encoded)

=== Doctor Summary: All essential systems operational! ===
```

*(Tip: If you have an existing Ollama service running on port 11434, `rudra doctor` will immediately detect it and suggest passing `--port 11435` or stopping Ollama!)*

### 5. `rudra bench`
Measures generation speed (tokens/sec) and Time-To-First-Token (TTFT):

```bash
rudra bench space-bunny-free --prompt "Write a quick explanation of Paxos."
```

### 6. `rudra config`
Directly inspect and modify your `rudra.toml` file without opening a text editor:

```bash
# View configuration
rudra config get
rudra config get port
rudra config get aliases.default

# Change settings
rudra config set port 11435
rudra config set overrides.space-bunny-free.reasoning_effort high

# Add a SOCKS5 proxy with a 50-request quota
rudra config add-proxy socks5://127.0.0.1:9050 --requests 50
```

---

## ⚙️ Configuration Reference (`rudra.toml`)

Copy `rudra.toml.example` to `rudra.toml` to customize your setup:

```toml
# Network settings
host = "0.0.0.0"
port = 11434                  # Default Ollama drop-in port
cors = "*"
timeout_secs = 120
log_level = "info"

# Model aliases: shorthand names for your clients
[aliases]
"default" = "mimo-v2.6-flash-free"
"jev" = "jev-1.13-free"

# Fine-tune reasoning effort per model (optional)
[overrides."space-bunny-free"]
reasoning_effort = "high"

# Outbound forward proxy pool (optional)
[proxy]
enabled = false
switch_after_requests = 100
proxies = [
    # "http://127.0.0.1:8080",
    # "socks5://127.0.0.1:9050",
]

# Session rotation (fresh ses_... ID every N requests)
[session]
rotate_after_requests = 10
```

---

## 🐳 Docker & Container Deployment

### Simple Container Run
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
    build: .
    container_name: rudra-proxy
    restart: unless-stopped
    ports:
      - "11434:11434"
    volumes:
      - ./rudra.toml:/app/rudra.toml:ro
```

---

## 📜 Disclaimer & Terms of Service

`rudra-proxy` is an open-source experimental research project for protocol interoperability. Please review [DISCLAIMER.md](DISCLAIMER.md) for statutory AS-IS warranty disclaimers, complete limitation of liability, and terms regarding third-party service compliance.
