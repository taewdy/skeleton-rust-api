//! Entry point for the skeleton-rust-api application.

use clap::Parser;
use skeleton_rust_api::commands;

#[tokio::main]
async fn main() {
    let cli = commands::Cli::parse();

    if let Err(err) = commands::run(cli).await {
        // The logger may not be initialised if config loading failed, so fall
        // back to stderr before exiting.
        eprintln!("failed to execute command: {err:#}");
        std::process::exit(1);
    }

    tracing::info!("command executed successfully");
}
