pub mod cli;
pub mod config;
pub mod middleware;
pub mod registry;
pub mod routes;
pub mod session;
pub mod upstream;

use crate::config::AppConfig;
use crate::registry::ModelRegistry;
use crate::routes::{create_router_with_cors, AppState};
use crate::session::SessionManager;
use crate::upstream::{ProxyPool, UpstreamClient};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// Run the rudra server with the specified configuration.
pub async fn run_server(config: AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.timeout_secs))
        .build()?;

    let proxy_pool = ProxyPool::new(&config.proxy, client.clone());
    let session_manager = SessionManager::new(config.session.rotate_after_requests);

    let registry = ModelRegistry::new(client.clone(), &config);
    let upstream = UpstreamClient::new(proxy_pool, config.upstream_api_key.clone());

    // Run initial catalog sync
    info!("Performing initial model discovery from OpenCode Zen...");
    match registry.sync().await {
        Ok(models) => {
            info!("Initial sync succeeded: {} models discovered", models.len());
        }
        Err(err) => {
            tracing::warn!(
                "Initial sync failed (offline or network issue): {err}. Using embedded fallback baseline."
            );
        }
    }

    if config.dry_run {
        let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
        println!("[✓] Configuration valid.");
        println!("[✓] Dry run: verified config, model catalog loaded ({} models).", registry.list_models().await.len());
        match std::net::TcpListener::bind(addr) {
            Ok(_) => println!("[✓] Port {} is available for binding on {}", config.port, config.host),
            Err(e) => eprintln!("[!] Port collision warning: Cannot bind to {addr}: {e}"),
        }
        println!("[✓] Dry run completed successfully. Exiting without starting HTTP listener.");
        return Ok(());
    }

    // Spawn background periodic sync task
    let sync_duration = Duration::from_secs(config.sync_interval_mins.max(1) * 60);
    Arc::new(registry.clone()).start_background_sync(sync_duration);

    let state = AppState {
        client,
        registry,
        upstream,
        session_manager,
    };

    let router = create_router_with_cors(state, config.api_key.clone(), &config.cors);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!("rudra listening on http://{addr}");
    if config.api_key.is_some() {
        info!("Downstream client authentication is ENABLED");
    } else {
        info!("Downstream client authentication is DISABLED (open access)");
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

use clap::Parser;
use crate::config::{CliArgs, Commands};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// CLI entrypoint for both rudra and rudra-proxy binaries
pub fn run_main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = CliArgs::parse();

    let log_level_filter = cli
        .log_level
        .as_deref()
        .or_else(|| match &cli.command {
            Some(Commands::Serve(args)) => args.log_level.as_deref(),
            _ => None,
        })
        .unwrap_or("info");

    let filter_str = format!("{},rudra_proxy=debug", log_level_filter);

    let is_simple_cmd = matches!(
        cli.command,
        Some(Commands::List(_))
            | Some(Commands::Config(_))
            | Some(Commands::Doctor(_))
            | Some(Commands::Run(_))
            | Some(Commands::Bench(_))
    );

    if !is_simple_cmd {
        tracing_subscriber::registry()
            .with(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| filter_str.into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    let workers = cli.workers.or_else(|| match &cli.command {
        Some(Commands::Serve(args)) => args.workers,
        _ => None,
    });

    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    if let Some(w) = workers {
        builder.worker_threads(w);
    }
    let runtime = builder.build()?;

    runtime.block_on(async {
        match cli.command {
            Some(Commands::Serve(_)) | None => {
                let config = AppConfig::load(cli);
                run_server(config).await
            }
            Some(Commands::List(args)) => cli::list::execute(args).await,
            Some(Commands::Config(args)) => cli::config_cmd::execute(args),
            Some(Commands::Run(args)) => cli::run::execute(args).await,
            Some(Commands::Doctor(args)) => cli::doctor::execute(args).await,
            Some(Commands::Bench(args)) => cli::bench::execute(args).await,
        }
    })
}
