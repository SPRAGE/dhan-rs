//! Core HTTP client for the DhanHQ REST API v2.
//!
//! The [`DhanClient`] struct is the main entry point for interacting with all
//! DhanHQ REST API endpoints. It wraps [`reqwest::Client`] with authentication
//! headers and provides typed `get`, `post`, `put`, and `delete` methods.
//!
//! API endpoint methods are added to `DhanClient` via `impl` blocks in the
//! [`crate::api`] module.

use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::constants::API_BASE_URL;
use crate::error::{ApiErrorBody, DhanError, Result};

/// Core HTTP client for the DhanHQ REST API v2.
///
/// Wraps [`reqwest::Client`] and injects the required authentication headers
/// into every request. Auth header values are cached at construction time to
/// avoid per-request allocation.
///
/// # Example
///
/// ```no_run
/// use dhan_rs::client::DhanClient;
///
/// # #[tokio::main]
/// # async fn main() -> dhan_rs::error::Result<()> {
/// let client = DhanClient::new("1000000001", "your-access-token");
/// // client.get::<MyResponse>("/v2/orders").await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct DhanClient {
    http: reqwest::Client,
    /// The Dhan client ID (user-specific identification).
    client_id: String,
    /// JWT access token.
    access_token: String,
    /// Base URL for REST API requests (defaults to [`API_BASE_URL`]).
    base_url: String,
    /// Pre-built auth header values, cached to avoid per-request allocation.
    auth_header_token: HeaderValue,
    auth_header_client_id: HeaderValue,
}

impl DhanClient {
    /// Create a new `DhanClient` with the given client ID and access token.
    ///
    /// Uses the default API base URL (`https://api.dhan.co`).
    pub fn new(client_id: impl Into<String>, access_token: impl Into<String>) -> Self {
        Self::with_base_url(client_id, access_token, API_BASE_URL)
    }

    /// Create a new `DhanClient` pointing at a custom base URL.
    ///
    /// Useful for testing against a sandbox or mock server.
    pub fn with_base_url(
        client_id: impl Into<String>,
        access_token: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .default_headers(Self::default_headers())
            .build()
            .expect("failed to build reqwest client");

        let access_token = access_token.into();
        let client_id = client_id.into();

        let auth_header_token = HeaderValue::from_str(&access_token)
            .expect("access token contains invalid header characters");
        let auth_header_client_id = HeaderValue::from_str(&client_id)
            .expect("client id contains invalid header characters");

        Self {
            http,
            client_id,
            access_token,
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            auth_header_token,
            auth_header_client_id,
        }
    }

    /// Returns a reference to the underlying `reqwest::Client`.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Returns the Dhan client ID.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Returns the current access token.
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    /// Replace the access token (e.g. after renewal).
    pub fn set_access_token(&mut self, token: impl Into<String>) {
        self.access_token = token.into();
        self.auth_header_token = HeaderValue::from_str(&self.access_token)
            .expect("access token contains invalid header characters");
    }

    /// Returns the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    // -----------------------------------------------------------------------
    // Generic HTTP helpers
    // -----------------------------------------------------------------------

    /// Perform a GET request and deserialize the JSON response.
    pub async fn get<R: DeserializeOwned>(&self, path: &str) -> Result<R> {
        let url = self.url(path);
        tracing::debug!(%url, "GET");

        let resp = self
            .http
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Perform a POST request with a JSON body and deserialize the response.
    pub async fn post<B: Serialize, R: DeserializeOwned>(&self, path: &str, body: &B) -> Result<R> {
        let url = self.url(path);
        tracing::debug!(%url, "POST");

        let resp = self
            .http
            .post(&url)
            .headers(self.auth_headers())
            .json(body)
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Perform a PUT request with a JSON body and deserialize the response.
    pub async fn put<B: Serialize, R: DeserializeOwned>(&self, path: &str, body: &B) -> Result<R> {
        let url = self.url(path);
        tracing::debug!(%url, "PUT");

        let resp = self
            .http
            .put(&url)
            .headers(self.auth_headers())
            .json(body)
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Perform a DELETE request and deserialize the JSON response.
    pub async fn delete<R: DeserializeOwned>(&self, path: &str) -> Result<R> {
        let url = self.url(path);
        tracing::debug!(%url, "DELETE");

        let resp = self
            .http
            .delete(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Perform a DELETE request that returns no body (expects 202 Accepted).
    pub async fn delete_no_content(&self, path: &str) -> Result<()> {
        let url = self.url(path);
        tracing::debug!(%url, "DELETE (no content)");

        let resp = self
            .http
            .delete(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(self.parse_error_body(status, &body))
        }
    }

    /// Perform a GET request that returns no body (expects 202 Accepted).
    pub async fn get_no_content(&self, path: &str) -> Result<()> {
        let url = self.url(path);
        tracing::debug!(%url, "GET (no content)");

        let resp = self
            .http
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(self.parse_error_body(status, &body))
        }
    }

    /// Perform a POST request that returns no body (expects 202 Accepted).
    pub async fn post_no_content<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let url = self.url(path);
        tracing::debug!(%url, "POST (no content)");

        let resp = self
            .http
            .post(&url)
            .headers(self.auth_headers())
            .json(body)
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(self.parse_error_body(status, &body))
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Build the full URL from a path segment.
    fn url(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{}", self.base_url, path)
        } else {
            format!("{}/{}", self.base_url, path)
        }
    }

    /// Default headers applied to every request.
    fn default_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
        headers
    }

    /// Per-request auth headers. Uses cached [`HeaderValue`]s — only the
    /// [`HeaderMap`] container is allocated per call (no string parsing).
    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::with_capacity(2);
        headers.insert("access-token", self.auth_header_token.clone());
        headers.insert("client-id", self.auth_header_client_id.clone());
        headers
    }

    /// Read a response, returning either the deserialized body or a `DhanError`.
    ///
    /// Uses `bytes()` + `serde_json::from_slice()` to avoid the overhead of
    /// UTF-8 validation that `text()` + `from_str()` would incur.
    async fn handle_response<R: DeserializeOwned>(&self, resp: reqwest::Response) -> Result<R> {
        let status = resp.status();
        let bytes = resp.bytes().await.unwrap_or_default();

        if status.is_success() {
            serde_json::from_slice(&bytes).map_err(DhanError::Json)
        } else {
            // Error path: parse as string for the error body
            let body = String::from_utf8_lossy(&bytes);
            Err(self.parse_error_body(status, &body))
        }
    }

    /// Try to parse the API's JSON error structure; fall back to a raw HTTP
    /// status error.
    pub(crate) fn parse_error_body(&self, status: reqwest::StatusCode, body: &str) -> DhanError {
        if let Ok(api_err) = serde_json::from_str::<ApiErrorBody>(body) {
            if api_err.error_code.is_some() || api_err.error_message.is_some() {
                return DhanError::Api(api_err);
            }
        }
        DhanError::HttpStatus {
            status,
            body: body.to_owned(),
        }
    }
}
