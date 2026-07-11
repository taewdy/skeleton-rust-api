//! Application configuration.
//!
//! Configuration is layered, with later sources overriding earlier ones:
//! defaults < YAML file < environment variables < CLI flags.
//! Nested keys are addressed in the environment with `__` as the separator,
//! e.g. `SERVER__PORT=9090` overrides `server.port`.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

/// Errors returned when loading the configuration.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The configuration could not be read, parsed, or deserialized.
    #[error("failed to load config: {0}")]
    Load(#[from] config::ConfigError),
}

/// Top-level configuration for the application.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Logging verbosity level: `debug`, `info`, `warn`, or `error`.
    pub log_level: String,
    /// Whether to include backtraces in error output.
    pub stacktrace: bool,
    /// Configuration for the placeholder command.
    pub placeholder: Placeholder,
    /// HTTP server configuration.
    pub server: Server,
    /// Database configuration.
    pub database: Database,
}

/// Configuration for the placeholder command.
#[derive(Debug, Clone, Deserialize)]
pub struct Placeholder {
    /// Sample identifier used by the placeholder command.
    pub id: i64,
}

/// HTTP server configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Server {
    /// Host address the server binds to.
    pub host: String,
    /// Port the server listens on.
    pub port: u16,
    /// Per-request handler timeout (e.g. `30s`).
    #[serde(with = "humantime_serde")]
    pub timeout: Duration,
}

/// Database configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Database {
    /// Database driver name. Only `postgres` is wired up in this skeleton.
    pub driver: String,
    /// Connection URL, e.g. `postgres://user:pass@localhost:5432/db`.
    pub database_url: String,
    /// Maximum number of open connections in the pool. `0` keeps the default.
    pub max_connection: u32,
    /// Number of idle connections the pool keeps warm. `0` keeps the default.
    pub max_idle_connection: u32,
    /// Maximum lifetime of a single connection (e.g. `30m`). Zero disables it.
    #[serde(with = "humantime_serde")]
    pub conn_max_lifetime: Duration,
    /// Timeout for the startup connectivity check (e.g. `5s`). Zero waits forever.
    #[serde(with = "humantime_serde")]
    pub ping_timeout: Duration,
}

/// Values passed on the command line that override every other source.
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    /// Overrides `log_level`.
    pub log_level: Option<String>,
    /// Overrides `stacktrace`.
    pub stacktrace: Option<bool>,
    /// Overrides `placeholder.id`.
    pub placeholder_id: Option<i64>,
}

/// Loads the configuration from the YAML file at `path`, applying defaults,
/// environment variables, and CLI overrides in increasing order of precedence.
pub fn load(path: &Path, overrides: &Overrides) -> Result<Config, Error> {
    let cfg = config::Config::builder()
        .set_default("log_level", "info")?
        .set_default("stacktrace", false)?
        .set_default("placeholder.id", 1)?
        .set_default("server.host", "127.0.0.1")?
        .set_default("server.port", 8080)?
        .set_default("server.timeout", "30s")?
        .set_default("database.driver", "postgres")?
        .set_default("database.database_url", "")?
        .set_default("database.max_connection", 0)?
        .set_default("database.max_idle_connection", 0)?
        .set_default("database.conn_max_lifetime", "0s")?
        .set_default("database.ping_timeout", "0s")?
        .add_source(config::File::from(path.to_path_buf()))
        .add_source(
            config::Environment::default()
                .separator("__")
                .try_parsing(true),
        )
        .set_override_option("log_level", overrides.log_level.clone())?
        .set_override_option("stacktrace", overrides.stacktrace)?
        .set_override_option("placeholder.id", overrides.placeholder_id)?
        .build()?;

    Ok(cfg.try_deserialize()?)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use super::*;

    // Tests that read or write process environment variables must hold this
    // lock, since the environment is shared across the whole test binary.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn testdata(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join(name)
    }

    #[test]
    fn loads_values_from_yaml_and_defaults() {
        let _guard = ENV_LOCK.lock().unwrap();

        let cfg = load(&testdata("config.yaml"), &Overrides::default()).unwrap();

        assert_eq!(cfg.log_level, "debug");
        assert!(!cfg.stacktrace);
        assert_eq!(cfg.server.host, "127.0.0.1");
        assert_eq!(cfg.server.port, 8081);
        assert_eq!(cfg.server.timeout, Duration::from_secs(30));
        assert_eq!(cfg.database.driver, "postgres");
        assert_eq!(cfg.database.max_connection, 20);
        assert_eq!(cfg.database.max_idle_connection, 10);
        assert_eq!(cfg.database.conn_max_lifetime, Duration::from_secs(30 * 60));
        assert_eq!(cfg.database.ping_timeout, Duration::from_secs(5));
        // The `placeholder` section is absent from the file, so the default applies.
        assert_eq!(cfg.placeholder.id, 1);
    }

    #[test]
    fn missing_file_is_an_error() {
        let _guard = ENV_LOCK.lock().unwrap();

        let err = load(&testdata("does-not-exist.yaml"), &Overrides::default());
        assert!(err.is_err());
    }

    #[test]
    fn invalid_yaml_is_an_error() {
        let _guard = ENV_LOCK.lock().unwrap();

        let err = load(&testdata("notyaml.yaml"), &Overrides::default());
        assert!(err.is_err());
    }

    #[test]
    fn cli_overrides_take_precedence() {
        let _guard = ENV_LOCK.lock().unwrap();

        let overrides = Overrides {
            log_level: Some("error".to_string()),
            stacktrace: Some(true),
            placeholder_id: Some(42),
        };
        let cfg = load(&testdata("config.yaml"), &overrides).unwrap();

        assert_eq!(cfg.log_level, "error");
        assert!(cfg.stacktrace);
        assert_eq!(cfg.placeholder.id, 42);
    }

    #[test]
    fn environment_overrides_file() {
        let _guard = ENV_LOCK.lock().unwrap();

        // SAFETY: guarded by ENV_LOCK; no other test mutates the environment.
        unsafe { std::env::set_var("SERVER__PORT", "9999") };
        let cfg = load(&testdata("config.yaml"), &Overrides::default());
        unsafe { std::env::remove_var("SERVER__PORT") };

        assert_eq!(cfg.unwrap().server.port, 9999);
    }
}
