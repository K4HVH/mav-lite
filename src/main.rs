mod config;
mod connection;
mod mavlink;
mod metrics;
mod router;

use config::Config;
use connection::tcp::TcpServer;
use connection::uart::UartConnection;
use connection::uart_discovery::UartDiscovery;
use metrics::Metrics;
use router::Router;
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration first (before logging, so we can use config log level)
    let config = match std::env::args().nth(1) {
        Some(path) => Config::from_file(&path)?,
        None => Config::example(),
    };

    // Initialize tracing with config log level (RUST_LOG env var overrides if set)
    let log_filter = std::env::var("RUST_LOG")
        .ok()
        .or_else(|| Some(config.log_level.clone()))
        .unwrap_or_else(|| "info".to_string());

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| log_filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("mav-lite starting...");

    if std::env::args().nth(1).is_some() {
        info!("Loading config from {}", std::env::args().nth(1).unwrap());
    } else {
        info!("No config file specified, using default configuration");
        info!("Usage: mav-lite [config.toml]");
    }

    info!("Configuration loaded:");
    info!("  Log level: {}", config.log_level);
    info!("  TCP: {}:{}", config.tcp.bind_addr, config.tcp.listen_port);
    info!("  UART devices: {}", config.uart.len());
    info!("  UART discovery: {}", if config.uart_discovery.enabled { "enabled" } else { "disabled" });
    info!("  Stats interval: {}s", config.stats_interval_secs);
    info!("  Routing:");
    info!("    UART->UART: {}", config.routing.allow_uart_to_uart);
    info!("    UART->TCP: {}", config.routing.allow_uart_to_tcp);
    info!("    TCP->UART: {}", config.routing.allow_tcp_to_uart);
    info!("    TCP->TCP: {}", config.routing.allow_tcp_to_tcp);

    // Create metrics and start stats logger
    let metrics = Metrics::new();
    if config.stats_interval_secs > 0 {
        info!(
            "Starting performance monitoring (stats every {}s)",
            config.stats_interval_secs
        );
        metrics.clone().start_stats_logger(config.stats_interval_secs);
    } else {
        info!("Performance monitoring disabled (stats_interval_secs = 0)");
    }

    // Create router channel
    let (router_tx, router_rx) = mpsc::unbounded_channel();

    // Start router task
    let router = Router::new(config.routing.clone(), metrics);
    tokio::spawn(async move {
        router.run(router_rx).await;
    });

    // Start static UART connections
    let mut next_uart_id = 0;
    for uart_cfg in &config.uart {
        let uart_conn = UartConnection::new(
            next_uart_id,
            uart_cfg.path.clone(),
            uart_cfg.baud_rate,
            uart_cfg.name.clone(),
        );
        uart_conn.start(router_tx.clone()).await;
        next_uart_id += 1;
    }

    // Start dynamic UART discovery if enabled
    if config.uart_discovery.enabled {
        let discovery = UartDiscovery::new(config.uart_discovery.clone(), next_uart_id);
        let discovery_tx = router_tx.clone();
        tokio::spawn(async move {
            discovery.run(discovery_tx).await;
        });
    }

    // Start TCP server
    let bind_addr = format!("{}:{}", config.tcp.bind_addr, config.tcp.listen_port);
    let mut tcp_server = TcpServer::bind(&bind_addr).await?;

    info!("mav-lite ready");

    // Accept TCP connections in a loop
    loop {
        match tcp_server.accept(router_tx.clone()).await {
            Ok(_) => {}
            Err(e) => {
                error!("Failed to accept TCP connection: {}", e);
            }
        }
    }
}
