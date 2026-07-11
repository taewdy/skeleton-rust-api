//! The root command: wires the dependencies together and runs the API server.

use std::sync::Arc;

use tracing::info;

use crate::config::Config;
use crate::{api, client, db, photos, server};

/// Builds the dependency graph (pool, client, services, routes) and serves
/// HTTP until a shutdown signal is received.
pub(crate) async fn start(cfg: &Config) -> anyhow::Result<()> {
    info!(config = ?cfg, "starting");

    let pool = db::new_database_pool(&cfg.database).await?;

    let http_client = client::Client::new(reqwest::Client::new());
    let photos_service = Arc::new(photos::Service::new(http_client));

    let routes = vec![api::photos_router(&cfg.server, photos_service)];
    let server = server::Server::new(&cfg.server, routes);

    server.start().await?;

    pool.close().await;
    info!("shutdown complete");

    Ok(())
}
