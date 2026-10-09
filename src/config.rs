use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_HOST: &str = "0.0.0.0";
pub const DEFAULT_PORT: u16 = 11434;
pub const DEFAULT_SYNC_INTERVAL_MINS: u64 = 30;
pub const DEFAULT_UPSTREAM_API_KEY: &str = "public";
pub const DEFAULT_CORS: &str = "*";
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;
pub const DEFAULT_LOG_LEVEL: &str = "info";

/// CLI arguments for rudra
#[derive(Parser, Debug, Clone, Default)]
#[command(
    name = "rudra",
    author,
    version,
    about = "Universal AI Proxy Bridge & Drop-in Replacement for OpenCode Zen",
    long_about = None
)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Bind address host
    #[arg(long, env = "RUDRA_HOST")]
    pub host: Option<String>,

    /// Listening port
    #[arg(short, long, env = "RUDRA_PORT")]
    pub port: Option<u16>,

    /// Optional downstream API key for client authentication
    #[arg(long, env = "RUDRA_API_KEY")]
    pub api_key: Option<String>,

    /// Upstream OpenCode Zen API key (default: "public")
    #[arg(long, env = "RUDRA_UPSTREAM_API_KEY")]
    pub upstream_api_key: Option<String>,

    /// Background model sync interval in minutes
    #[arg(long, env = "RUDRA_SYNC_INTERVAL_MINS")]
    pub sync_interval_mins: Option<u64>,

    /// Path to configuration file (rudra.toml)
    #[arg(short, long, env = "RUDRA_CONFIG")]
    pub config: Option<PathBuf>,

    /// Enable outbound forward proxy pool
    #[arg(long, env = "RUDRA_PROXY_ENABLED")]
    pub proxy_enabled: Option<bool>,

    /// Outbound proxy URL(s) to route upstream requests through (can specify multiple)
    #[arg(long = "proxy", env = "RUDRA_PROXIES")]
    pub proxies: Option<Vec<String>>,

    /// Number of requests before rotating to the next outbound proxy
    #[arg(long, env = "RUDRA_PROXY_SWITCH_REQUESTS")]
    pub proxy_switch_requests: Option<u64>,

    /// Number of requests before generating a new OpenCode session ID
    #[arg(long, env = "RUDRA_SESSION_ROTATE_REQUESTS")]
    pub session_rotate_requests: Option<u64>,

    /// CORS allowed origin (e.g. "*" or "http://localhost:3000")
    #[arg(long, env = "RUDRA_CORS")]
    pub cors: Option<String>,

    /// Number of worker threads
    #[arg(long, env = "RUDRA_WORKERS")]
    pub workers: Option<usize>,

    /// Upstream request timeout in seconds
    #[arg(long, env = "RUDRA_TIMEOUT")]
    pub timeout: Option<u64>,

    /// Log level filter (trace, debug, info, warn, error)
    #[arg(long, env = "RUDRA_LOG_LEVEL")]
    pub log_level: Option<String>,

    /// Validate configuration and upstream connectivity without starting listener
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start the proxy server (default)
    Serve(ServeArgs),

    /// List models with live upstream health and latency probing
    List(ListArgs),

    /// Inspect and modify local configuration file (rudra.toml)
    Config(ConfigArgs),

    /// Run an interactive terminal REPL or one-shot prompt runner
    Run(RunArgs),

    /// Diagnose upstream network health and detect port collisions
    Doctor(DoctorArgs),

    /// Benchmark Time-To-First-Token and tokens/sec for a model
    Bench(BenchArgs),
}

#[derive(clap::Args, Debug, Clone, Default)]
pub struct ServeArgs {
    /// Bind address host
    #[arg(long)]
    pub host: Option<String>,

    /// Listening port
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Optional downstream API key
    #[arg(long)]
    pub api_key: Option<String>,

    /// Upstream OpenCode Zen API key
    #[arg(long)]
    pub upstream_api_key: Option<String>,

    /// Background sync interval in minutes
    #[arg(long)]
    pub sync_interval_mins: Option<u64>,

    /// Path to configuration file
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Enable outbound forward proxy pool
    #[arg(long)]
    pub proxy_enabled: Option<bool>,

    /// Outbound proxy URL(s) to route upstream requests through
    #[arg(long = "proxy")]
    pub proxies: Option<Vec<String>>,

    /// Number of requests before rotating to the next outbound proxy
    #[arg(long)]
    pub proxy_switch_requests: Option<u64>,

    /// Number of requests before generating a new OpenCode session ID
    #[arg(long)]
    pub session_rotate_requests: Option<u64>,

    /// CORS allowed origin
    #[arg(long)]
    pub cors: Option<String>,

    /// Number of worker threads
    #[arg(long)]
    pub workers: Option<usize>,

    /// Upstream request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,

    /// Log level filter
    #[arg(long)]
    pub log_level: Option<String>,

    /// Validate configuration without starting server
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListFormat {
    #[default]
    Table,
    Json,
    Markdown,
}

#[derive(clap::Args, Debug, Clone, Default)]
pub struct ListArgs {
    /// Actively probe upstream status and latency for models
    #[arg(long)]
    pub live: bool,

    /// Output format (table, json, markdown)
    #[arg(short, long, value_enum, default_value_t = ListFormat::Table)]
    pub format: ListFormat,

    /// Path to configuration file
    #[arg(short, long)]
    pub config: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct ConfigArgs {
    /// Target configuration file path (default: rudra.toml)
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum ConfigAction {
    /// Get a configuration value or display the full configuration
    Get {
        /// Configuration key (e.g., "port", "host", "proxy.enabled", "aliases.gpt-4o")
        key: Option<String>,
    },
    /// Set a configuration value
    Set {
        /// Configuration key to set
        key: String,
        /// New value
        value: String,
    },
    /// Add an outbound proxy to the pool
    AddProxy {
        /// Proxy URL (e.g. "http://127.0.0.1:8080" or "socks5://127.0.0.1:9050")
        url: String,
        /// Optional requests quota for this proxy before switching
        #[arg(short, long)]
        requests: Option<u64>,
    },
    /// Remove an outbound proxy from the pool
    RemoveProxy {
        /// Proxy URL to remove
        url: String,
    },
}

#[derive(clap::Args, Debug, Clone)]
pub struct RunArgs {
    /// Target model name or alias (e.g. "mimo-v2.6-flash-free", "muse-spark-1.3-contributor-free", "gpt-4o")
    pub model: String,

    /// Prompt to send. If omitted, launches an interactive terminal REPL
    pub prompt: Option<String>,

    /// Optional system instruction
    #[arg(short, long)]
    pub system: Option<String>,

    /// Temperature (0.0 to 2.0)
    #[arg(short, long)]
    pub temperature: Option<f64>,

    /// Reasoning effort (e.g. "minimal", "low", "medium", "high", "xhigh")
    #[arg(short, long)]
    pub reasoning_effort: Option<String>,

    /// Path to configuration file
    #[arg(short, long)]
    pub config: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone, Default)]
pub struct DoctorArgs {
    /// Host to test (default: 127.0.0.1)
    #[arg(long)]
    pub host: Option<String>,

    /// Port to check for collisions (default: 11434)
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Path to configuration file
    #[arg(short, long)]
    pub config: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct BenchArgs {
    /// Target model to benchmark
    pub model: String,

    /// Test prompt to run
    #[arg(
        short,
        long,
        default_value = "Explain quantum computing in 3 concise bullet points."
    )]
    pub prompt: String,

    /// Path to configuration file
    #[arg(short, long)]
    pub config: Option<PathBuf>,
}

/// Optional model override configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelOverrideConfig {
    /// Protocol override: "responses", "chat_completions", or "systemone"
    pub protocol: Option<String>,
    /// Default reasoning effort: e.g. "high", "medium", "low"
    pub reasoning_effort: Option<String>,
}

/// Configuration for a specific outbound proxy node with its own request quota
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyNodeConfig {
    pub url: String,
    #[serde(default = "default_proxy_node_requests")]
    pub requests: u64,
}

fn default_proxy_node_requests() -> u64 {
    1
}

/// Outbound forward proxy pool configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_switch_after_requests")]
    pub switch_after_requests: u64,
    #[serde(default)]
    pub proxies: Vec<String>,
    #[serde(default)]
    pub list: Vec<ProxyNodeConfig>,
}

fn default_switch_after_requests() -> u64 {
    1
}

/// Request-based OpenCode session ID rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_session_rotate_requests")]
    pub rotate_after_requests: u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            rotate_after_requests: default_session_rotate_requests(),
        }
    }
}

fn default_session_rotate_requests() -> u64 {
    10
}

/// Representation of the TOML config file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigFile {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub api_key: Option<String>,
    pub upstream_api_key: Option<String>,
    pub sync_interval_mins: Option<u64>,
    pub cors: Option<String>,
    pub timeout_secs: Option<u64>,
    pub log_level: Option<String>,
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    #[serde(default)]
    pub overrides: HashMap<String, ModelOverrideConfig>,
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub session: SessionConfig,
}

/// Resolved runtime configuration
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub api_key: Option<String>,
    pub upstream_api_key: String,
    pub sync_interval_mins: u64,
    pub cors: String,
    pub timeout_secs: u64,
    pub log_level: String,
    pub workers: Option<usize>,
    pub dry_run: bool,
    pub aliases: HashMap<String, String>,
    pub overrides: HashMap<String, ModelOverrideConfig>,
    pub proxy: ProxyConfig,
    pub session: SessionConfig,
}

impl AppConfig {
    /// Load configuration by merging CLI flags, ENV, config file, and defaults.
    pub fn load(cli: CliArgs) -> Self {
        let serve_args = match &cli.command {
            Some(Commands::Serve(args)) => Some(args),
            _ => None,
        };

        let config_path = cli
            .config
            .as_ref()
            .or_else(|| serve_args.and_then(|a| a.config.as_ref()))
            .cloned()
            .or_else(|| {
                let default_toml = Path::new("rudra.toml");
                if default_toml.exists() {
                    Some(default_toml.to_path_buf())
                } else {
                    None
                }
            });

        let file_config = if let Some(ref path) = config_path {
            match fs::read_to_string(path) {
                Ok(content) => match toml::from_str::<ConfigFile>(&content) {
                    Ok(cfg) => {
                        tracing::info!("Loaded configuration from {}", path.display());
                        cfg
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse {}: {e}", path.display());
                        ConfigFile::default()
                    }
                },
                Err(e) => {
                    tracing::warn!("Could not read configuration file {}: {e}", path.display());
                    ConfigFile::default()
                }
            }
        } else {
            ConfigFile::default()
        };

        let host = cli
            .host
            .or_else(|| serve_args.and_then(|a| a.host.clone()))
            .or(file_config.host)
            .unwrap_or_else(|| DEFAULT_HOST.to_string());

        let port = cli
            .port
            .or_else(|| serve_args.and_then(|a| a.port))
            .or(file_config.port)
            .unwrap_or(DEFAULT_PORT);

        let api_key = cli
            .api_key
            .or_else(|| serve_args.and_then(|a| a.api_key.clone()))
            .or(file_config.api_key);

        let upstream_api_key = cli
            .upstream_api_key
            .or_else(|| serve_args.and_then(|a| a.upstream_api_key.clone()))
            .or(file_config.upstream_api_key)
            .unwrap_or_else(|| DEFAULT_UPSTREAM_API_KEY.to_string());

        let sync_interval_mins = cli
            .sync_interval_mins
            .or_else(|| serve_args.and_then(|a| a.sync_interval_mins))
            .or(file_config.sync_interval_mins)
            .unwrap_or(DEFAULT_SYNC_INTERVAL_MINS);

        let cors = cli
            .cors
            .or_else(|| serve_args.and_then(|a| a.cors.clone()))
            .or(file_config.cors)
            .unwrap_or_else(|| DEFAULT_CORS.to_string());

        let timeout_secs = cli
            .timeout
            .or_else(|| serve_args.and_then(|a| a.timeout))
            .or(file_config.timeout_secs)
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        let log_level = cli
            .log_level
            .or_else(|| serve_args.and_then(|a| a.log_level.clone()))
            .or(file_config.log_level)
            .unwrap_or_else(|| DEFAULT_LOG_LEVEL.to_string());

        let workers = cli.workers.or_else(|| serve_args.and_then(|a| a.workers));

        let dry_run = cli.dry_run || serve_args.map(|a| a.dry_run).unwrap_or(false);

        // Proxy pool resolution
        let proxy_enabled = cli
            .proxy_enabled
            .or_else(|| serve_args.and_then(|a| a.proxy_enabled))
            .unwrap_or(file_config.proxy.enabled);

        let switch_after_requests = cli
            .proxy_switch_requests
            .or_else(|| serve_args.and_then(|a| a.proxy_switch_requests))
            .unwrap_or(file_config.proxy.switch_after_requests);

        let proxies = cli
            .proxies
            .or_else(|| serve_args.and_then(|a| a.proxies.clone()))
            .unwrap_or(file_config.proxy.proxies);

        let proxy = ProxyConfig {
            enabled: proxy_enabled,
            switch_after_requests,
            proxies,
            list: file_config.proxy.list,
        };

        // Session ID rotation resolution
        let rotate_after_requests = cli
            .session_rotate_requests
            .or_else(|| serve_args.and_then(|a| a.session_rotate_requests))
            .unwrap_or(file_config.session.rotate_after_requests);

        let session = SessionConfig {
            rotate_after_requests,
        };

        let mut aliases = default_aliases();
        aliases.extend(file_config.aliases);

        Self {
            host,
            port,
            api_key,
            upstream_api_key,
            sync_interval_mins,
            cors,
            timeout_secs,
            log_level,
            workers,
            dry_run,
            aliases,
            overrides: file_config.overrides,
            proxy,
            session,
        }
    }
}

pub fn default_aliases() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert(
        "gpt-4o".to_string(),
        "muse-spark-1.3-contributor-free".to_string(),
    );
    map.insert(
        "gpt-4".to_string(),
        "muse-spark-1.3-contributor-free".to_string(),
    );
    map.insert(
        "gpt-3.5-turbo".to_string(),
        "mimo-v2.6-flash-free".to_string(),
    );
    map.insert(
        "claude-3-5-sonnet".to_string(),
        "nemotron-3-ultra-free".to_string(),
    );
    map.insert(
        "llama3".to_string(),
        "nemotron-3.5-lightning-free".to_string(),
    );
    map.insert(
        "default".to_string(),
        "mimo-v2.6-flash-free".to_string(),
    );
    map.insert(
        "jev".to_string(),
        "jev-1.13-free".to_string(),
    );
    map
}
