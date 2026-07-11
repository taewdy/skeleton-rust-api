//! Structured logging built on `tracing` (the zap equivalent).
//!
//! Services log through the global `tracing` macros (`info!`, `error!`, ...)
//! instead of an injected logger value — that is the idiomatic Rust
//! translation of injecting a `*zap.Logger`.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry, reload};

/// Errors returned when initialising the logger.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A global subscriber was already installed.
    #[error("failed to initialise logger: {0}")]
    Init(#[from] tracing_subscriber::util::TryInitError),
}

/// Handle to the logging system. Allows changing the log level at runtime.
pub struct Logger {
    handle: reload::Handle<EnvFilter, Registry>,
}

/// Initialises the global logger with the given level (`debug`, `info`,
/// `warn`, `error`). When `stacktrace` is enabled, backtraces are captured
/// for panics and errors that support them.
pub fn init(level: &str, stacktrace: bool) -> Result<Logger, Error> {
    if stacktrace {
        // SAFETY: called once at startup, before any other threads exist.
        unsafe { std::env::set_var("RUST_BACKTRACE", "1") };
    }

    let (filter, handle) = reload::Layer::new(parse_filter(level));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .try_init()?;

    Ok(Logger { handle })
}

impl Logger {
    /// Sets the log level of the logger at runtime. An invalid level is
    /// reported and the existing level is kept.
    pub fn set_level(&self, level: &str) {
        match EnvFilter::try_new(level) {
            Ok(filter) => {
                if let Err(err) = self.handle.reload(filter) {
                    tracing::error!(error = %err, "failed to set log level");
                    return;
                }
                tracing::info!(level, "new log level");
            }
            Err(err) => {
                tracing::error!(error = %err, "invalid log level provided, keeping existing level");
            }
        }
    }
}

fn parse_filter(level: &str) -> EnvFilter {
    EnvFilter::try_new(level).unwrap_or_else(|_| {
        eprintln!("invalid log level '{level}', using default 'info'");
        EnvFilter::new("info")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_filter_falls_back_to_info_on_invalid_level() {
        // Valid levels parse as given; invalid ones fall back to `info`.
        assert_eq!(parse_filter("debug").to_string(), "debug");
        assert_eq!(parse_filter("no-such-level=?!").to_string(), "info");
    }

    #[test]
    fn init_and_set_level() {
        // `init` installs a global subscriber, so a single test exercises the
        // whole lifecycle: init, change level, reject an invalid level, and
        // fail on double initialisation.
        let logger = init("info", false).unwrap();
        logger.set_level("debug");
        logger.set_level("no-such-level=?!");

        assert!(init("info", false).is_err());
    }
}
