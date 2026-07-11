//! The Postgres connection pool for the application.

use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::config;

/// Errors returned when creating the database pool.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The pool could not be created or the database could not be reached.
    #[error("failed to connect to database: {0}")]
    Connect(#[from] sqlx::Error),
    /// The startup connectivity check did not complete in time.
    #[error("timed out pinging database after {0:?}")]
    PingTimeout(Duration),
}

/// Holds the database connection pool.
pub struct DatabasePool {
    /// The underlying sqlx pool; inject this into repositories.
    pub pool: PgPool,
}

/// Creates a new database connection pool and verifies connectivity, honouring
/// the configured ping timeout.
pub async fn new_database_pool(cfg: &config::Database) -> Result<DatabasePool, Error> {
    let mut options = PgPoolOptions::new();

    if cfg.max_connection > 0 {
        options = options.max_connections(cfg.max_connection);
    }
    // sqlx has no direct "max idle connections" knob; `min_connections` keeps
    // that many connections warm, which is the closest equivalent.
    if cfg.max_idle_connection > 0 {
        options = options.min_connections(cfg.max_idle_connection);
    }
    if !cfg.conn_max_lifetime.is_zero() {
        options = options.max_lifetime(cfg.conn_max_lifetime);
    }

    let pool = options.connect_lazy(&cfg.database_url)?;
    ping_with_timeout(&pool, cfg.ping_timeout).await?;

    Ok(DatabasePool { pool })
}

impl DatabasePool {
    /// Shuts down the database pool, waiting for connections to be released.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

async fn ping_with_timeout(pool: &PgPool, timeout: Duration) -> Result<(), Error> {
    if timeout.is_zero() {
        pool.acquire().await?;
        return Ok(());
    }

    tokio::time::timeout(timeout, pool.acquire())
        .await
        .map_err(|_| Error::PingTimeout(timeout))??;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database_config(url: &str, ping_timeout: Duration) -> config::Database {
        config::Database {
            driver: "postgres".to_string(),
            database_url: url.to_string(),
            max_connection: 5,
            max_idle_connection: 2,
            conn_max_lifetime: Duration::from_secs(60),
            ping_timeout,
        }
    }

    #[tokio::test]
    async fn invalid_url_is_an_error() {
        let cfg = database_config("not-a-database-url", Duration::from_millis(100));

        let err = new_database_pool(&cfg).await;

        assert!(matches!(err, Err(Error::Connect(_))));
    }

    #[tokio::test]
    async fn unreachable_database_fails_ping() {
        // Port 1 is reserved and closed, so the ping either gets a refused
        // connection (Connect) or runs into the timeout (PingTimeout).
        let cfg = database_config(
            "postgres://user:pass@127.0.0.1:1/db",
            Duration::from_millis(200),
        );

        let err = new_database_pool(&cfg).await;

        assert!(err.is_err());
    }
}
