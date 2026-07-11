//! Handlers for the API endpoints.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;
use tracing::error;

use crate::config;
use crate::photos::{self, HttpGet, Photo};

/// Abstraction over the photos service, defined at the consumer side.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait PhotosGetter: Send + Sync {
    /// Gets a single photo by id.
    async fn get_photo(&self, id: i64) -> Result<Photo, photos::Error>;
}

#[async_trait]
impl<C: HttpGet> PhotosGetter for photos::Service<C> {
    async fn get_photo(&self, id: i64) -> Result<Photo, photos::Error> {
        photos::Service::get_photo(self, id).await
    }
}

#[derive(Clone)]
struct PhotosState {
    timeout: Duration,
    service: Arc<dyn PhotosGetter>,
}

/// Builds the router for the photos endpoints, with its dependencies baked in
/// (the equivalent of the Go handler factory `api.Photos(cfg, ps, l)`).
pub fn photos_router(cfg: &config::Server, service: Arc<dyn PhotosGetter>) -> Router {
    Router::new()
        .route("/photos/{id}", get(get_photo))
        .with_state(PhotosState {
            timeout: cfg.timeout,
            service,
        })
}

async fn get_photo(State(state): State<PhotosState>, Path(id): Path<String>) -> Response {
    let Ok(id) = id.parse::<i64>() else {
        error!("failed to parse id");
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid id"})),
        )
            .into_response();
    };

    match tokio::time::timeout(state.timeout, state.service.get_photo(id)).await {
        Ok(Ok(photo)) => (StatusCode::OK, Json(photo)).into_response(),
        Ok(Err(err)) => {
            error!(error = %err, "failed to get photos");
            internal_error()
        }
        Err(_elapsed) => {
            error!("timed out getting photos");
            internal_error()
        }
    }
}

fn internal_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "failed to get photos"})),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    fn server_config(timeout: Duration) -> config::Server {
        config::Server {
            host: "127.0.0.1".to_string(),
            port: 8080,
            timeout,
        }
    }

    fn photo(id: i64) -> Photo {
        Photo {
            album_id: 1,
            id,
            title: "a title".to_string(),
            url: "https://example.com/1".to_string(),
            thumbnail_url: "https://example.com/thumb/1".to_string(),
        }
    }

    async fn send(router: Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let response = router
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn returns_photo_as_json() {
        let mut mock = MockPhotosGetter::new();
        mock.expect_get_photo()
            .withf(|id| *id == 1)
            .returning(|id| Ok(photo(id)));

        let router = photos_router(&server_config(Duration::from_secs(1)), Arc::new(mock));
        let (status, body) = send(router, "/photos/1").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, serde_json::to_value(photo(1)).unwrap());
    }

    #[tokio::test]
    async fn rejects_non_numeric_id() {
        let router = photos_router(
            &server_config(Duration::from_secs(1)),
            Arc::new(MockPhotosGetter::new()),
        );
        let (status, body) = send(router, "/photos/abc").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, json!({"error": "invalid id"}));
    }

    #[tokio::test]
    async fn maps_service_errors_to_internal_error() {
        let mut mock = MockPhotosGetter::new();
        mock.expect_get_photo()
            .returning(|_| Err(photos::Error::Status(StatusCode::INTERNAL_SERVER_ERROR)));

        let router = photos_router(&server_config(Duration::from_secs(1)), Arc::new(mock));
        let (status, body) = send(router, "/photos/1").await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, json!({"error": "failed to get photos"}));
    }

    #[tokio::test]
    async fn times_out_slow_service_calls() {
        struct SlowGetter;

        #[async_trait]
        impl PhotosGetter for SlowGetter {
            async fn get_photo(&self, id: i64) -> Result<Photo, photos::Error> {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(photo(id))
            }
        }

        let router = photos_router(
            &server_config(Duration::from_millis(20)),
            Arc::new(SlowGetter),
        );
        let (status, body) = send(router, "/photos/1").await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, json!({"error": "failed to get photos"}));
    }
}
