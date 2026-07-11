//! The HTTP server for the application: route registration, request logging
//! middleware, and graceful shutdown.

use std::time::Instant;

use axum::Json;
use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;
use tracing::{debug, info};

use crate::config;

/// Errors returned by the server.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The listener could not be bound to the configured address.
    #[error("failed to bind server address: {0}")]
    Bind(std::io::Error),
    /// The server stopped with an error.
    #[error("failed to start server: {0}")]
    Serve(std::io::Error),
}

/// The HTTP server. Routes are passed in as sub-routers with their
/// dependencies already baked in (the equivalent of Go's `[]RouteParam`).
pub struct Server {
    addr: String,
    router: Router,
}

impl Server {
    /// Creates a new server instance, registering the given routes plus the
    /// health check, the not-found fallback, and the logging middleware.
    pub fn new(cfg: &config::Server, sub_routers: Vec<Router>) -> Self {
        let router = sub_routers
            .into_iter()
            .fold(Router::new().route("/", get(health)), Router::merge)
            .fallback(not_found)
            .layer(middleware::from_fn(log_request));

        Self {
            addr: format!("{}:{}", cfg.host, cfg.port),
            router,
        }
    }

    /// Returns a clone of the router, for exercising the server in tests
    /// without binding a socket (the equivalent of Go's `ServeHTTP`).
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    /// Starts the HTTP server and serves until a shutdown signal (SIGINT or
    /// SIGTERM) is received, then drains in-flight requests.
    pub async fn start(self) -> Result<(), Error> {
        let listener = tokio::net::TcpListener::bind(&self.addr)
            .await
            .map_err(Error::Bind)?;

        info!(addr = %self.addr, "server listening");

        axum::serve(listener, self.router)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(Error::Serve)
    }
}

// Axum handlers must be `async` even when they never await.
#[allow(clippy::unused_async)]
async fn health() -> &'static str {
    "ok"
}

#[allow(clippy::unused_async)]
async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"}))).into_response()
}

async fn log_request(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let path = request
        .uri()
        .path_and_query()
        .map_or_else(String::new, ToString::to_string);

    let response = next.run(request).await;

    debug!(
        %method,
        path,
        status = response.status().as_u16(),
        latency = ?start.elapsed(),
        "http request"
    );

    response
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install SIGINT handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    info!("shutdown signal received");
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    fn test_server(routes: Vec<Router>) -> Server {
        let cfg = config::Server {
            host: "127.0.0.1".to_string(),
            port: 0,
            timeout: std::time::Duration::from_secs(1),
        };
        Server::new(&cfg, routes)
    }

    #[tokio::test]
    async fn health_check_returns_ok() {
        let response = test_server(vec![])
            .router()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, "ok");
    }

    #[tokio::test]
    async fn unknown_route_returns_not_found_json() {
        let response = test_server(vec![])
            .router()
            .oneshot(Request::builder().uri("/nope").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body, json!({"message": "Not Found"}));
    }

    #[tokio::test]
    async fn registered_routes_are_served() {
        let routes = vec![Router::new().route("/hello", get(|| async { "world" }))];

        let response = test_server(routes)
            .router()
            .oneshot(
                Request::builder()
                    .uri("/hello")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, "world");
    }
}
