//! Thin wrapper around the HTTP client for making requests.
//!
//! Consumers depend on their own trait (see [`crate::photos::HttpGet`]) rather
//! than on this concrete type, mirroring the Go proverb "accept interfaces,
//! return structs".

use bytes::Bytes;
use reqwest::StatusCode;
use reqwest::header::ACCEPT;

/// Errors returned by the HTTP client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The request failed to be built, sent, or read.
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
}

/// A fully-buffered HTTP response: status code plus body bytes.
#[derive(Debug, Clone)]
pub struct Response {
    /// HTTP status code of the response.
    pub status: StatusCode,
    /// Raw response body.
    pub body: Bytes,
}

/// A wrapper around the HTTP client.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
}

impl Client {
    /// Creates a new `Client` around an existing `reqwest::Client`.
    pub fn new(http: reqwest::Client) -> Self {
        Self { http }
    }

    /// Makes a GET request to the specified URL and buffers the response.
    pub async fn get(&self, url: &str) -> Result<Response, Error> {
        let response = self
            .http
            .get(url)
            .header(ACCEPT, "application/json")
            .send()
            .await?;

        let status = response.status();
        let body = response.bytes().await?;

        Ok(Response { status, body })
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn get_returns_status_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/photos/1"))
            .and(header("accept", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_string("hello"))
            .mount(&server)
            .await;

        let client = Client::new(reqwest::Client::new());
        let response = client
            .get(&format!("{}/photos/1", server.uri()))
            .await
            .unwrap();

        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(response.body, Bytes::from("hello"));
    }

    #[tokio::test]
    async fn get_propagates_connection_errors() {
        let client = Client::new(reqwest::Client::new());

        // Port 1 is reserved and closed, so the connection is refused.
        let err = client.get("http://127.0.0.1:1").await;

        assert!(matches!(err, Err(Error::Request(_))));
    }
}
