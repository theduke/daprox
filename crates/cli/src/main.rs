use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Parser;
use daprox::config::ServerConfig;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "daprox=trace,info");
    }
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let config = if let Some(path) = &args.config {
        load_config_file(&path)
            .with_context(|| format!("Failed to load config file at '{}'", path.display()))?
    } else {
        ServerConfig::default()
    };

    tracing::debug!(?config, "loaded config");

    let shutdown = tokio::signal::ctrl_c().fuse();

    let fut = daprox::server::start(config).fuse();

    tokio::select! {
        res = fut => {
            res
        }
        _ = shutdown => {
            tracing::info!("shutting down...");
            Ok(())
        }
    }
}

#[derive(clap::Parser, Debug)]
#[clap(
    name = "daprox",
    version = clap::crate_version!(),
    about = clap::crate_description!(),
)]
struct Args {
    /// Path to the configuration file
    #[clap(long)]
    config: Option<PathBuf>,
}

fn load_config_file(path: &Path) -> Result<ServerConfig, anyhow::Error> {
    let content = std::fs::read_to_string(path)?;
    let conf = serde_yaml::from_str(&content)?;
    Ok(conf)
}
