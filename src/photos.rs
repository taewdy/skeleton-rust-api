//! Operations for handling photos. Contains the [`Service`] struct and the
//! sample concurrent fetcher, mirroring the Go skeleton's `photos` package.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::client;

const PHOTOS_URL: &str = "https://jsonplaceholder.typicode.com/photos";

/// A photo object as returned by jsonplaceholder.typicode.com.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    /// Identifier of the album the photo belongs to.
    pub album_id: i64,
    /// Identifier of the photo.
    pub id: i64,
    /// Title of the photo.
    pub title: String,
    /// URL of the full-size photo.
    pub url: String,
    /// URL of the thumbnail.
    pub thumbnail_url: String,
}

/// Errors returned by the photos service.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The underlying HTTP request failed.
    #[error("failed to get photos: {0}")]
    Http(#[from] client::Error),
    /// The upstream returned a non-OK status code.
    #[error("received non-OK HTTP status: {0}")]
    Status(StatusCode),
    /// The response body could not be decoded.
    #[error("failed to decode response body: {0}")]
    Decode(#[from] serde_json::Error),
}

/// Abstraction over the HTTP client, defined here — at the consumer side —
/// rather than next to the client implementation (the Go guideline "define
/// interfaces at the consumer, not the provider").
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait HttpGet: Send + Sync {
    /// Makes a GET request to the specified URL.
    async fn get(&self, url: &str) -> Result<client::Response, client::Error>;
}

#[async_trait]
impl HttpGet for client::Client {
    async fn get(&self, url: &str) -> Result<client::Response, client::Error> {
        client::Client::get(self, url).await
    }
}

/// Provides the operations for handling photos.
pub struct Service<C> {
    client: C,
}

impl<C: HttpGet> Service<C> {
    /// Creates a new `Service` for handling photos operations.
    pub fn new(client: C) -> Self {
        Self { client }
    }

    /// Gets a single photo by id from the photos URL.
    pub async fn get_photo(&self, id: i64) -> Result<Photo, Error> {
        let response = self
            .client
            .get(&format!("{PHOTOS_URL}/{id}"))
            .await
            .inspect_err(|err| error!(error = %err, "failed to get photos"))?;

        if response.status != StatusCode::OK {
            error!(status = %response.status, "non-OK HTTP status received");
            return Err(Error::Status(response.status));
        }

        let photo = serde_json::from_slice(&response.body)
            .inspect_err(|err| error!(error = %err, "failed to decode response body"))?;

        Ok(photo)
    }
}

impl<C: HttpGet + 'static> Service<C> {
    /// Gets photos with ids `1..=concurrency` concurrently, processing each
    /// result as soon as it is available, and returns the ids that succeeded.
    pub async fn get_photos_concurrently(self: &Arc<Self>, concurrency: u32) -> Vec<i64> {
        let (tx, mut rx) = mpsc::unbounded_channel();

        for id in 1..=concurrency {
            let service = Arc::clone(self);
            let tx = tx.clone();
            tokio::spawn(async move {
                // The receive side may have been dropped; nothing to do then.
                let _ = tx.send(service.get_photo(i64::from(id)).await);
            });
        }

        // Drop the original sender so the channel closes once every spawned
        // task has finished (the Go version's `wg.Wait(); close(chanResult)`).
        drop(tx);

        let mut processed = Vec::new();
        while let Some(result) = rx.recv().await {
            match result {
                Ok(photo) => {
                    info!(id = photo.id, "processed photo");
                    processed.push(photo.id);
                }
                Err(err) => error!(error = %err, "failed to process photo"),
            }
        }

        processed
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;

    fn photo(id: i64) -> Photo {
        Photo {
            album_id: 1,
            id,
            title: format!("photo {id}"),
            url: format!("https://example.com/{id}"),
            thumbnail_url: format!("https://example.com/thumb/{id}"),
        }
    }

    fn photo_json(id: i64) -> Bytes {
        serde_json::to_vec(&photo(id)).unwrap().into()
    }

    fn id_from_url(url: &str) -> i64 {
        url.rsplit('/').next().unwrap().parse().unwrap()
    }

    #[tokio::test]
    async fn get_photo_returns_decoded_photo() {
        let mut mock = MockHttpGet::new();
        mock.expect_get()
            .withf(|url| url == format!("{PHOTOS_URL}/1"))
            .returning(|_| {
                Ok(client::Response {
                    status: StatusCode::OK,
                    body: photo_json(1),
                })
            });

        let service = Service::new(mock);

        assert_eq!(service.get_photo(1).await.unwrap(), photo(1));
    }

    #[tokio::test]
    async fn get_photo_fails_on_non_ok_status() {
        let mut mock = MockHttpGet::new();
        mock.expect_get().returning(|_| {
            Ok(client::Response {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                body: Bytes::new(),
            })
        });

        let service = Service::new(mock);

        let err = service.get_photo(1).await.unwrap_err();
        assert!(matches!(
            err,
            Error::Status(StatusCode::INTERNAL_SERVER_ERROR)
        ));
    }

    #[tokio::test]
    async fn get_photo_fails_on_invalid_body() {
        let mut mock = MockHttpGet::new();
        mock.expect_get().returning(|_| {
            Ok(client::Response {
                status: StatusCode::OK,
                body: Bytes::from("not json"),
            })
        });

        let service = Service::new(mock);

        let err = service.get_photo(1).await.unwrap_err();
        assert!(matches!(err, Error::Decode(_)));
    }

    #[tokio::test]
    async fn get_photo_propagates_client_errors() {
        let mut mock = MockHttpGet::new();
        mock.expect_get().returning(|_| {
            // Produce a real reqwest error without touching the network by
            // sending a request with an invalid URL.
            Err(client::Error::Request(
                reqwest::Client::new()
                    .get("http://[invalid")
                    .build()
                    .unwrap_err(),
            ))
        });

        let service = Service::new(mock);

        let err = service.get_photo(1).await.unwrap_err();
        assert!(matches!(err, Error::Http(_)));
    }

    #[tokio::test]
    async fn get_photos_concurrently_collects_all_ids() {
        let mut mock = MockHttpGet::new();
        mock.expect_get().times(5).returning(|url| {
            Ok(client::Response {
                status: StatusCode::OK,
                body: photo_json(id_from_url(url)),
            })
        });

        let service = Arc::new(Service::new(mock));
        let mut processed = service.get_photos_concurrently(5).await;
        processed.sort_unstable();

        assert_eq!(processed, vec![1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn get_photos_concurrently_skips_failures() {
        let mut mock = MockHttpGet::new();
        mock.expect_get().times(5).returning(|url| {
            let id = id_from_url(url);
            if id == 3 {
                return Ok(client::Response {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    body: Bytes::new(),
                });
            }
            Ok(client::Response {
                status: StatusCode::OK,
                body: photo_json(id),
            })
        });

        let service = Arc::new(Service::new(mock));
        let mut processed = service.get_photos_concurrently(5).await;
        processed.sort_unstable();

        assert_eq!(processed, vec![1, 2, 4, 5]);
    }
}
