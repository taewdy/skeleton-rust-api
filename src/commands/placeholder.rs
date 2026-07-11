//! The placeholder subcommand: a template for adding new commands.

use tracing::info;

use crate::config::Config;

/// Does nothing beyond logging the resolved configuration.
// Every subcommand shares the fallible signature (Go's `RunE`), even though
// this placeholder cannot fail yet.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn start(cfg: &Config) -> anyhow::Result<()> {
    info!(config = ?cfg, "starting");

    Ok(())
}
