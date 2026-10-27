use crate::registry::metadata::ModelProtocol;
use crate::registry::sync::DEFAULT_USER_AGENT;
use crate::session::{RequestId, SessionId};
use crate::upstream::proxy_pool::ProxyPool;
use reqwest::Response;
use serde_json::Value;
use tracing::{debug, warn};

pub const OPENCODE_RESPONSES_URL: &str = "https://opencode.ai/inference/v1/responses";
pub const OPENCODE_CHAT_COMPLETIONS_URL: &str = "https://opencode.ai/inference/v1/chat/completions";
pub const OPENCODE_SYSTEMONE_URL: &str = "https://opencode.ai/inference/v1/systemone";
pub const DEFAULT_PROJECT_ID: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";

#[derive(Clone)]
pub struct UpstreamClient {
    proxy_pool: ProxyPool,
    upstream_api_key: String,
    project_id: String,
}

impl UpstreamClient {
    pub fn new(proxy_pool: ProxyPool, upstream_api_key: String) -> Self {
        let project_id =
            std::env::var("OPENCODE_PROJECT_ID").unwrap_or_else(|_| DEFAULT_PROJECT_ID.to_string());
        Self {
            proxy_pool,
            upstream_api_key,
            project_id,
        }
    }

    /// Dispatches the request to OpenCode with mandatory attestation headers and active proxy routing.
    pub async fn dispatch(
        &self,
        protocol: ModelProtocol,
        session_id: &SessionId,
        payload: &Value,
    ) -> Result<Response, reqwest::Error> {
        self.dispatch_with_headers(protocol, session_id, payload, None, None)
            .await
    }

    /// Dispatches the request to OpenCode with explicit request/project ID propagation.
    pub async fn dispatch_with_headers(
        &self,
        protocol: ModelProtocol,
        session_id: &SessionId,
        payload: &Value,
        request_id: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<Response, reqwest::Error> {
        let target_url = match protocol {
            ModelProtocol::Responses => OPENCODE_RESPONSES_URL,
            ModelProtocol::ChatCompletions => OPENCODE_CHAT_COMPLETIONS_URL,
            ModelProtocol::SystemOne => OPENCODE_SYSTEMONE_URL,
        };

        let max_attempts = if self.proxy_pool.active_proxies_count() > 1 {
            self.proxy_pool.active_proxies_count()
        } else {
            1
        };

        let mut attempts = 0;
        loop {
            attempts += 1;
            let (client, proxy_url) = self.proxy_pool.get_client();
            if let Some(ref p) = proxy_url {
                debug!(
                    proxy = %p,
                    target = %target_url,
                    attempt = attempts,
                    "Routing upstream request through proxy"
                );
            }

            let auth_header = format!("Bearer {}", self.upstream_api_key);
            let req_id = match request_id {
                Some(id) if !id.is_empty() => id.to_string(),
                _ => RequestId::generate().to_string(),
            };
            let proj_id = project_id
                .filter(|p| !p.is_empty())
                .unwrap_or(&self.project_id);

            let resp = client
                .post(target_url)
                .header("Content-Type", "application/json")
                .header("User-Agent", DEFAULT_USER_AGENT)
                .header("x-opencode-client", "cli")
                .header("x-opencode-session", session_id.as_str())
                .header("x-opencode-session-id", session_id.as_str())
                .header("x-session-affinity", session_id.as_str())
                .header("X-Session-Id", session_id.as_str())
                .header("x-opencode-request", &req_id)
                .header("x-opencode-project", proj_id)
                .header("Authorization", &auth_header)
                .json(payload)
                .send()
                .await;

            match resp {
                Ok(response)
                    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                        && attempts < max_attempts =>
                {
                    warn!(
                        attempt = attempts,
                        max_attempts = max_attempts,
                        "Upstream returned HTTP 429 (rate limit). Rotating proxy pool node and retrying..."
                    );
                    self.proxy_pool.rotate_now();
                    continue;
                }
                other => return other,
            }
        }
    }

    /// Dispatches a TypeSafe AI / System One evaluation request with active proxy routing.
    pub async fn dispatch_systemone(
        &self,
        session_id: &SessionId,
        payload: &Value,
    ) -> Result<Response, reqwest::Error> {
        self.dispatch_systemone_with_headers(session_id, payload, None, None)
            .await
    }

    /// Dispatches a TypeSafe AI / System One evaluation request with explicit request/project ID propagation.
    pub async fn dispatch_systemone_with_headers(
        &self,
        session_id: &SessionId,
        payload: &Value,
        request_id: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<Response, reqwest::Error> {
        let max_attempts = if self.proxy_pool.active_proxies_count() > 1 {
            self.proxy_pool.active_proxies_count()
        } else {
            1
        };

        let mut attempts = 0;
        loop {
            attempts += 1;
            let (client, proxy_url) = self.proxy_pool.get_client();
            if let Some(ref p) = proxy_url {
                debug!(
                    proxy = %p,
                    attempt = attempts,
                    "Routing upstream System One request through proxy"
                );
            }

            let auth_header = format!("Bearer {}", self.upstream_api_key);
            let req_id = match request_id {
                Some(id) if !id.is_empty() => id.to_string(),
                _ => RequestId::generate().to_string(),
            };
            let proj_id = project_id
                .filter(|p| !p.is_empty())
                .unwrap_or(&self.project_id);

            let resp = client
                .post(OPENCODE_SYSTEMONE_URL)
                .header("Content-Type", "application/json")
                .header("User-Agent", DEFAULT_USER_AGENT)
                .header("x-opencode-client", "cli")
                .header("x-opencode-session", session_id.as_str())
                .header("x-opencode-session-id", session_id.as_str())
                .header("x-session-affinity", session_id.as_str())
                .header("X-Session-Id", session_id.as_str())
                .header("x-opencode-request", &req_id)
                .header("x-opencode-project", proj_id)
                .header("Authorization", &auth_header)
                .json(payload)
                .send()
                .await;

            match resp {
                Ok(response)
                    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                        && attempts < max_attempts =>
                {
                    warn!(
                        attempt = attempts,
                        max_attempts = max_attempts,
                        "Upstream System One returned HTTP 429. Rotating proxy pool node and retrying..."
                    );
                    self.proxy_pool.rotate_now();
                    continue;
                }
                other => return other,
            }
        }
    }

    pub fn proxy_pool(&self) -> &ProxyPool {
        &self.proxy_pool
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }
}
