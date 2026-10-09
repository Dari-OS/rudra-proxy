use crate::registry::metadata::ModelProtocol;
use crate::registry::sync::DEFAULT_USER_AGENT;
use crate::session::SessionId;
use crate::upstream::proxy_pool::ProxyPool;
use reqwest::Response;
use serde_json::Value;
use tracing::debug;

pub const OPENCODE_RESPONSES_URL: &str = "https://opencode.ai/zen/v1/responses";
pub const OPENCODE_CHAT_COMPLETIONS_URL: &str = "https://opencode.ai/zen/v1/chat/completions";
pub const OPENCODE_SYSTEMONE_URL: &str = "https://opencode.ai/zen/v1/systemone";

#[derive(Clone)]
pub struct UpstreamClient {
    proxy_pool: ProxyPool,
    upstream_api_key: String,
}

impl UpstreamClient {
    pub fn new(proxy_pool: ProxyPool, upstream_api_key: String) -> Self {
        Self {
            proxy_pool,
            upstream_api_key,
        }
    }

    /// Dispatches the request to OpenCode Zen with mandatory spoofed headers and active proxy routing.
    pub async fn dispatch(
        &self,
        protocol: ModelProtocol,
        session_id: &SessionId,
        payload: &Value,
    ) -> Result<Response, reqwest::Error> {
        let target_url = match protocol {
            ModelProtocol::Responses => OPENCODE_RESPONSES_URL,
            ModelProtocol::ChatCompletions => OPENCODE_CHAT_COMPLETIONS_URL,
            ModelProtocol::SystemOne => OPENCODE_SYSTEMONE_URL,
        };

        let (client, proxy_url) = self.proxy_pool.get_client();
        if let Some(ref p) = proxy_url {
            debug!(proxy = %p, target = %target_url, "Routing upstream request through proxy");
        }

        let auth_header = format!("Bearer {}", self.upstream_api_key);

        client
            .post(target_url)
            .header("Content-Type", "application/json")
            .header("User-Agent", DEFAULT_USER_AGENT)
            .header("x-opencode-session", session_id.as_str())
            .header("Authorization", &auth_header)
            .json(payload)
            .send()
            .await
    }

    /// Dispatches a TypeSafe AI / System One evaluation request with active proxy routing.
    pub async fn dispatch_systemone(
        &self,
        session_id: &SessionId,
        payload: &Value,
    ) -> Result<Response, reqwest::Error> {
        let (client, proxy_url) = self.proxy_pool.get_client();
        if let Some(ref p) = proxy_url {
            debug!(proxy = %p, "Routing upstream System One request through proxy");
        }

        let auth_header = format!("Bearer {}", self.upstream_api_key);

        client
            .post(OPENCODE_SYSTEMONE_URL)
            .header("Content-Type", "application/json")
            .header("User-Agent", DEFAULT_USER_AGENT)
            .header("x-opencode-session", session_id.as_str())
            .header("Authorization", &auth_header)
            .json(payload)
            .send()
            .await
    }

    pub fn proxy_pool(&self) -> &ProxyPool {
        &self.proxy_pool
    }
}
