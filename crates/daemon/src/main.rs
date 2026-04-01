use clap::Parser;
use std::path::PathBuf;
use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use fetorrent_core::config::FeConfig;
use fetorrent_core::Engine;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Port to bind the API/UI server to
    #[arg(short, long)]
    port: Option<u16>,

    /// Address to bind the server to
    #[arg(short, long)]
    bind: Option<String>,

    /// Directory for downloaded files
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// Path to ui distribution folder
    #[arg(long)]
    ui_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log_path = FeConfig::resolve_log_path();
    let file_appender = tracing_appender::rolling::never(
        log_path.parent().unwrap(), 
        log_path.file_name().unwrap()
    );
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "fetorrent=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer()) // Stdout
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking)) // File
        .init();

    let args = Args::parse();
    let mut config = FeConfig::load_or_default();

    // Override config with CLI args
    if let Some(port) = args.port {
        config.server.port = port;
    }
    if let Some(bind) = args.bind {
        config.server.bind = bind;
    }
    if let Some(dir) = args.dir {
        config.downloads.directory = dir.to_string_lossy().to_string();
    }

    let download_path = get_download_path(&config.downloads.directory);
    tracing::info!("Starting FeTorrent daemon...");
    tracing::info!("Download directory: {}", download_path.display());
    tracing::info!("Log file: {}", log_path.display());

    let bind = config.server.bind.clone();
    let port = config.server.port;
    let engine = Engine::new(config);

    let ui_dir = FeConfig::resolve_ui_dir(args.ui_dir);

    tokio::spawn(async move {
        if let Err(e) = fetorrent_api::start_server(engine, &bind, port, &ui_dir).await {
            tracing::error!("Server error: {}", e);
        }
    });

    match signal::ctrl_c().await {
        Ok(()) => {
            tracing::info!("Received Ctrl-C, shutting down gracefully...");
        }
        Err(err) => {
            tracing::error!("Unable to listen for shutdown signal: {}", err);
        }
    }

    Ok(())
}

fn get_download_path(dir_str: &str) -> PathBuf {
    if dir_str.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&dir_str[2..]);
        }
    }
    PathBuf::from(dir_str)
}
