use crate::config::{AppConfig, CliArgs, DoctorArgs};
use crate::registry::sync::{
    DEFAULT_USER_AGENT, OPENCODE_CATALOG_URL, OPENCODE_ZEN_MODELS_URL,
};
use crate::session::SessionId;
use std::net::TcpListener;
use std::time::Instant;

pub async fn execute(args: DoctorArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Rudra Health & Connectivity Doctor ===\n");

    let mut cli_copy = CliArgs {
        command: None,
        host: args.host.clone(),
        port: args.port,
        api_key: None,
        upstream_api_key: None,
        sync_interval_mins: None,
        config: args.config.clone(),
        proxy_enabled: None,
        proxies: None,
        proxy_switch_requests: None,
        session_rotate_requests: None,
        cors: None,
        workers: None,
        timeout: None,
        log_level: None,
        dry_run: false,
    };
    if let Some(c) = args.config {
        cli_copy.config = Some(c);
    }
    let config = AppConfig::load(cli_copy);

    let mut all_ok = true;

    // 1. Check Port Availability & Collision Detection
    let host_to_test = if config.host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        &config.host
    };
    let bind_addr = format!("{}:{}", host_to_test, config.port);
    match TcpListener::bind(&bind_addr) {
        Ok(_) => {
            println!("[✓] Port {} is available for binding on {}", config.port, host_to_test);
        }
        Err(e) => {
            all_ok = false;
            // Detect if Ollama is running on this port
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(1500))
                .build()?;
            let probe_url = format!("http://127.0.0.1:{}/api/version", config.port);
            match client.get(&probe_url).send().await {
                Ok(res) if res.status().is_success() => {
                    let version_text = res.text().await.unwrap_or_default();
                    println!(
                        "[✗] Port collision: Port {} is actively being used by an Ollama instance!\n\
                         \x20   Response from /api/version: {}\n\
                         \x20   Recommendation: Run rudra on a different port (e.g. `rudra serve --port 11435`)\n\
                         \x20   or temporarily stop Ollama (`systemctl stop ollama` or `pkill ollama`).",
                        config.port, version_text.trim()
                    );
                }
                _ => {
                    println!(
                        "[✗] Port collision: Port {} is already in use by another process: {}\n\
                         \x20   Recommendation: Specify a different port using `--port <PORT>`.",
                        config.port, e
                    );
                }
            }
        }
    }

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // 2. Check OpenCode Zen Gateway Reachability
    let auth_header = format!("Bearer {}", config.upstream_api_key);
    let start_zen = Instant::now();
    match http_client
        .get(OPENCODE_ZEN_MODELS_URL)
        .header("User-Agent", DEFAULT_USER_AGENT)
        .header("Authorization", &auth_header)
        .send()
        .await
    {
        Ok(res) => {
            let lat = start_zen.elapsed().as_millis();
            if res.status().is_success() {
                println!(
                    "[✓] OpenCode Zen gateway reachable at {} ({}ms, HTTP {})",
                    OPENCODE_ZEN_MODELS_URL,
                    lat,
                    res.status()
                );
            } else {
                all_ok = false;
                println!(
                    "[✗] OpenCode Zen gateway responded with HTTP {}: {}",
                    res.status(),
                    res.text().await.unwrap_or_default()
                );
            }
        }
        Err(e) => {
            all_ok = false;
            println!("[✗] Failed to reach OpenCode Zen gateway ({}): {e}", OPENCODE_ZEN_MODELS_URL);
        }
    }

    // 3. Check models.opencode.ai Catalog Reachability
    let start_cat = Instant::now();
    match http_client
        .get(OPENCODE_CATALOG_URL)
        .header("User-Agent", DEFAULT_USER_AGENT)
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => {
            let lat = start_cat.elapsed().as_millis();
            println!(
                "[✓] OpenCode catalog metadata reachable at {} ({}ms)",
                OPENCODE_CATALOG_URL, lat
            );
        }
        Ok(res) => {
            println!(
                "[!] OpenCode catalog returned HTTP {}. Proxy will use embedded baseline models.",
                res.status()
            );
        }
        Err(e) => {
            println!(
                "[!] OpenCode catalog unreachable ({e}). Proxy will use embedded baseline models."
            );
        }
    }

    // 4. Validate Session ID Generator
    let test_ses = SessionId::generate();
    if test_ses.as_str().len() == 30 && test_ses.as_str().starts_with("ses_") {
        println!(
            "[✓] Session ID generator valid (generated: {}, length: 30 chars, timestamp encoded)",
            test_ses
        );
    } else {
        all_ok = false;
        println!("[✗] Session ID generator produced invalid session: {}", test_ses);
    }

    // 5. Check Proxy Pool Reachability if configured
    if config.proxy.enabled {
        println!("\n--- Checking Outbound Forward Proxy Pool ---");
        let mut proxy_urls = config.proxy.proxies.clone();
        for node in &config.proxy.list {
            if !proxy_urls.contains(&node.url) {
                proxy_urls.push(node.url.clone());
            }
        }

        if proxy_urls.is_empty() {
            println!("[!] Proxy pool is enabled but no proxy URLs are configured.");
        } else {
            for p_url in proxy_urls {
                match reqwest::Proxy::all(&p_url) {
                    Ok(proxy_obj) => match reqwest::Client::builder()
                        .proxy(proxy_obj)
                        .timeout(std::time::Duration::from_secs(5))
                        .build()
                    {
                        Ok(proxied_client) => {
                            let p_start = Instant::now();
                            match proxied_client.get("https://opencode.ai/favicon.ico").send().await {
                                Ok(res) if res.status().is_success() => {
                                    println!(
                                        "[✓] Outbound proxy '{}' connected ({}ms)",
                                        p_url,
                                        p_start.elapsed().as_millis()
                                    );
                                }
                                Ok(res) => {
                                    println!(
                                        "[!] Outbound proxy '{}' connected but returned status {}",
                                        p_url,
                                        res.status()
                                    );
                                }
                                Err(e) => {
                                    all_ok = false;
                                    println!("[✗] Outbound proxy '{}' connection test failed: {e}", p_url);
                                }
                            }
                        }
                        Err(e) => {
                            all_ok = false;
                            println!("[✗] Failed to initialize client for proxy '{}': {e}", p_url);
                        }
                    },
                    Err(e) => {
                        all_ok = false;
                        println!("[✗] Invalid proxy URL format for '{}': {e}", p_url);
                    }
                }
            }
        }
    }

    println!();
    if all_ok {
        println!("=== Doctor Summary: All essential systems operational! ===");
    } else {
        println!("=== Doctor Summary: Issues detected above. Review the recommendations. ===");
    }

    Ok(())
}
