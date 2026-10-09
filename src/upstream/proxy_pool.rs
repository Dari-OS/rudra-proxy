use crate::config::ProxyConfig;
use reqwest::Client;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

#[derive(Clone)]
pub struct ProxyNode {
    pub url: String,
    pub client: Client,
    pub max_requests: u64,
}

#[derive(Clone)]
pub struct ProxyPool {
    enabled: bool,
    default_client: Client,
    nodes: Vec<ProxyNode>,
    current_index: Arc<AtomicUsize>,
    request_counter: Arc<AtomicU64>,
}

impl ProxyPool {
    pub fn new(config: &ProxyConfig, default_client: Client) -> Self {
        if !config.enabled {
            return Self {
                enabled: false,
                default_client,
                nodes: Vec::new(),
                current_index: Arc::new(AtomicUsize::new(0)),
                request_counter: Arc::new(AtomicU64::new(0)),
            };
        }

        let mut nodes = Vec::new();

        // 1. Check detailed list
        for item in &config.list {
            match Self::build_client_for_proxy(&item.url) {
                Ok(client) => {
                    nodes.push(ProxyNode {
                        url: item.url.clone(),
                        client,
                        max_requests: item.requests.max(1),
                    });
                }
                Err(e) => {
                    warn!("Failed to configure proxy '{}': {e}", item.url);
                }
            }
        }

        // 2. Check simple string list
        for url in &config.proxies {
            // Avoid duplicate if already added in list
            if !nodes.iter().any(|n| n.url == *url) {
                match Self::build_client_for_proxy(url) {
                    Ok(client) => {
                        nodes.push(ProxyNode {
                            url: url.clone(),
                            client,
                            max_requests: config.switch_after_requests.max(1),
                        });
                    }
                    Err(e) => {
                        warn!("Failed to configure proxy '{url}': {e}");
                    }
                }
            }
        }

        let enabled = config.enabled && !nodes.is_empty();
        if enabled {
            info!(
                count = nodes.len(),
                "Outbound proxy pool initialized with {} proxies",
                nodes.len()
            );
        } else if config.enabled {
            warn!("Proxy support is enabled but no valid proxy URLs were provided; using direct connection");
        }

        Self {
            enabled,
            default_client,
            nodes,
            current_index: Arc::new(AtomicUsize::new(0)),
            request_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    fn build_client_for_proxy(url: &str) -> Result<Client, reqwest::Error> {
        let proxy = reqwest::Proxy::all(url)?;
        Client::builder()
            .proxy(proxy)
            .timeout(Duration::from_secs(300))
            .build()
    }

    /// Returns the active HTTP client to use for the next request, rotating proxies if necessary.
    pub fn get_client(&self) -> (Client, Option<String>) {
        if !self.enabled || self.nodes.is_empty() {
            return (self.default_client.clone(), None);
        }

        let total_nodes = self.nodes.len();
        let current_idx = self.current_index.load(Ordering::Relaxed) % total_nodes;
        let node = &self.nodes[current_idx];

        let req_num = self.request_counter.fetch_add(1, Ordering::Relaxed) + 1;

        if req_num >= node.max_requests {
            self.request_counter.store(0, Ordering::Relaxed);
            let next_idx = (current_idx + 1) % total_nodes;
            self.current_index.store(next_idx, Ordering::Relaxed);
            let next_node = &self.nodes[next_idx];
            info!(
                switched_from = %node.url,
                switched_to = %next_node.url,
                after_requests = req_num,
                "Rotated outbound proxy"
            );
        }

        (node.client.clone(), Some(node.url.clone()))
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn active_proxies_count(&self) -> usize {
        self.nodes.len()
    }
}
