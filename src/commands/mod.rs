//! The command line interface for the application: the root command (which
//! runs the API server) and all subcommands.

mod placeholder;
mod root;

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};

use crate::{config, logger};

const APP_NAME: &str = "skeleton-rust-api";

/// CLI for the skeleton-rust-api application.
///
/// This CLI is used to interact with the skeleton-rust-api application.
#[derive(Debug, Parser)]
#[command(name = APP_NAME, version, about)]
pub struct Cli {
    /// Specifies the path to the configuration file.
    #[arg(long, default_value = "./config.yaml")]
    pub config: PathBuf,

    /// Determines the logging verbosity level for the application.
    /// Available options are 'debug', 'info', 'warn', and 'error'.
    #[arg(long, env = "LOG_LEVEL")]
    pub log_level: Option<String>,

    /// Enables or disables the inclusion of stack traces in the log output.
    #[arg(long, env = "STACKTRACE")]
    pub stacktrace: Option<bool>,

    /// Subcommand to run; the API server starts when none is given.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Placeholder for a command that does nothing.
    Placeholder {
        /// placeholder flag option
        #[arg(short, long)]
        id: Option<i64>,
    },
}

/// Loads the configuration, initialises the logger, and runs the requested
/// command.
pub async fn run(cli: Cli) -> anyhow::Result<()> {
    let overrides = config::Overrides {
        log_level: cli.log_level.clone(),
        stacktrace: cli.stacktrace,
        placeholder_id: match cli.command {
            Some(Command::Placeholder { id }) => id,
            None => None,
        },
    };

    let cfg = config::load(&cli.config, &overrides).context("error building config")?;
    logger::init(&cfg.log_level, cfg.stacktrace).context("error initialising logger")?;

    match cli.command {
        None => root::start(&cfg).await,
        Some(Command::Placeholder { .. }) => placeholder::start(&cfg),
    }
}
