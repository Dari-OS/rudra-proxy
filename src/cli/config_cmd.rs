use crate::config::{ConfigAction, ConfigArgs, ConfigFile, ProxyNodeConfig};
use std::fs;
use std::path::PathBuf;

pub fn execute(args: ConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = args
        .config
        .unwrap_or_else(|| PathBuf::from("rudra.toml"));

    let mut config: ConfigFile = if config_path.exists() {
        let content = fs::read_to_string(&config_path)?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        ConfigFile::default()
    };

    match args.action {
        ConfigAction::Get { key } => {
            if let Some(k) = key {
                match k.as_str() {
                    "host" => println!("{}", config.host.as_deref().unwrap_or("0.0.0.0")),
                    "port" => println!("{}", config.port.unwrap_or(11434)),
                    "api_key" => println!("{}", config.api_key.as_deref().unwrap_or("(none)")),
                    "upstream_api_key" => {
                        println!("{}", config.upstream_api_key.as_deref().unwrap_or("public"))
                    }
                    "sync_interval_mins" => {
                        println!("{}", config.sync_interval_mins.unwrap_or(30))
                    }
                    "cors" => println!("{}", config.cors.as_deref().unwrap_or("*")),
                    "timeout_secs" => println!("{}", config.timeout_secs.unwrap_or(120)),
                    "log_level" => println!("{}", config.log_level.as_deref().unwrap_or("info")),
                    "proxy.enabled" => println!("{}", config.proxy.enabled),
                    "proxy.switch_after_requests" => {
                        println!("{}", config.proxy.switch_after_requests)
                    }
                    "proxy.proxies" => {
                        println!("{}", serde_json::to_string(&config.proxy.proxies)?)
                    }
                    "session.rotate_after_requests" => {
                        println!("{}", config.session.rotate_after_requests)
                    }
                    other if other.starts_with("aliases.") => {
                        let alias_name = &other["aliases.".len()..];
                        if let Some(target) = config.aliases.get(alias_name) {
                            println!("{target}");
                        } else {
                            eprintln!("Alias '{alias_name}' not found");
                            std::process::exit(1);
                        }
                    }
                    other if other.starts_with("overrides.") => {
                        let rem = &other["overrides.".len()..];
                        if let Some((model_id, field)) = rem.split_once('.') {
                            if let Some(ov) = config.overrides.get(model_id) {
                                match field {
                                    "reasoning_effort" => {
                                        if let Some(ref effort) = ov.reasoning_effort {
                                            println!("{effort}");
                                        } else {
                                            println!("(none)");
                                        }
                                    }
                                    "protocol" => {
                                        if let Some(ref proto) = ov.protocol {
                                            println!("{proto}");
                                        } else {
                                            println!("(none)");
                                        }
                                    }
                                    _ => {
                                        eprintln!("Unknown override field: {field}");
                                        std::process::exit(1);
                                    }
                                }
                            } else {
                                eprintln!("Override for model '{model_id}' not found");
                                std::process::exit(1);
                            }
                        } else {
                            eprintln!("Format must be overrides.<model>.<field> (e.g. overrides.space-bunny-free.reasoning_effort)");
                            std::process::exit(1);
                        }
                    }
                    other => {
                        eprintln!("Unknown configuration key: {other}");
                        std::process::exit(1);
                    }
                }
            } else {
                let toml_str = toml::to_string_pretty(&config)?;
                println!("{toml_str}");
            }
        }
        ConfigAction::Set { key, value } => {
            match key.as_str() {
                "host" => config.host = Some(value.clone()),
                "port" => {
                    let p: u16 = value.parse().map_err(|_| "Port must be a valid u16 integer")?;
                    config.port = Some(p);
                }
                "api_key" => config.api_key = if value.is_empty() { None } else { Some(value.clone()) },
                "upstream_api_key" => config.upstream_api_key = Some(value.clone()),
                "sync_interval_mins" => {
                    let m: u64 = value.parse().map_err(|_| "sync_interval_mins must be a valid integer")?;
                    config.sync_interval_mins = Some(m);
                }
                "cors" => config.cors = Some(value.clone()),
                "timeout_secs" => {
                    let t: u64 = value.parse().map_err(|_| "timeout_secs must be an integer")?;
                    config.timeout_secs = Some(t);
                }
                "log_level" => config.log_level = Some(value.clone()),
                "proxy.enabled" => {
                    let b: bool = value.parse().map_err(|_| "proxy.enabled must be true or false")?;
                    config.proxy.enabled = b;
                }
                "proxy.switch_after_requests" => {
                    let n: u64 = value.parse().map_err(|_| "switch_after_requests must be an integer")?;
                    config.proxy.switch_after_requests = n;
                }
                "session.rotate_after_requests" => {
                    let n: u64 = value.parse().map_err(|_| "rotate_after_requests must be an integer")?;
                    config.session.rotate_after_requests = n;
                }
                other if other.starts_with("aliases.") => {
                    let alias_name = &other["aliases.".len()..];
                    config.aliases.insert(alias_name.to_string(), value.clone());
                }
                other if other.starts_with("overrides.") => {
                    let rem = &other["overrides.".len()..];
                    if let Some((model_id, field)) = rem.split_once('.') {
                        let entry = config.overrides.entry(model_id.to_string()).or_default();
                        match field {
                            "reasoning_effort" => entry.reasoning_effort = Some(value.clone()),
                            "protocol" => entry.protocol = Some(value.clone()),
                            _ => {
                                eprintln!("Unknown override field: {field}");
                                std::process::exit(1);
                            }
                        }
                    } else {
                        eprintln!("Format must be overrides.<model>.<field> (e.g. overrides.space-bunny-free.reasoning_effort)");
                        std::process::exit(1);
                    }
                }
                other => {
                    eprintln!("Unknown configuration key: {other}");
                    std::process::exit(1);
                }
            }

            let new_content = toml::to_string_pretty(&config)?;
            fs::write(&config_path, new_content)?;
            println!("[✓] Successfully set {} = {} in {}", key, value, config_path.display());
        }
        ConfigAction::AddProxy { url, requests } => {
            config.proxy.enabled = true;
            if let Some(req_quota) = requests {
                config.proxy.list.retain(|p| p.url != url);
                config.proxy.list.push(ProxyNodeConfig {
                    url: url.clone(),
                    requests: req_quota,
                });
            } else if !config.proxy.proxies.contains(&url) {
                config.proxy.proxies.push(url.clone());
            }

            let new_content = toml::to_string_pretty(&config)?;
            fs::write(&config_path, new_content)?;
            println!(
                "[✓] Successfully added proxy '{}' (quota: {:?}) to {} (proxy enabled)",
                url, requests, config_path.display()
            );
        }
        ConfigAction::RemoveProxy { url } => {
            config.proxy.proxies.retain(|p| p != &url);
            config.proxy.list.retain(|p| p.url != url);

            let new_content = toml::to_string_pretty(&config)?;
            fs::write(&config_path, new_content)?;
            println!("[✓] Successfully removed proxy '{}' from {}", url, config_path.display());
        }
    }

    Ok(())
}
