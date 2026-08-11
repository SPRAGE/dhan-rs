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

/// Validate a required dynamic path component and encode it as one URL path
/// segment.  Callers must use this rather than interpolating user input into a
/// route: an order ID such as `a/b` is an ID, not two path segments.
pub(crate) fn required_path_segment(name: &str, value: &str) -> Result<String> {
    if value.trim().is_empty() {
        return Err(DhanError::InvalidArgument(format!(
            "{name} must not be empty"
        )));
    }
    // `byte_serialize` leaves RFC 3986 unreserved characters alone. A whole
    // dot segment is special to URL parsers, so reject it rather than relying
    // on percent-encoding that a parser might normalize before transmission.
    if matches!(value, "." | "..") {
        return Err(DhanError::InvalidArgument(format!(
            "{name} must not be a path-navigation segment"
        )));
    }
    Ok(percent_encode_component(value))
}

/// Validate and percent-encode a required query value.
///
/// This uses percent encoding rather than form `+` escaping so generated
/// routes remain unambiguous in logs, proxies, and request-target tests.
pub(crate) fn required_query_value(name: &str, value: &str) -> Result<String> {
    required_path_segment(name, value)
}

/// Percent-encode a URL component without applying HTML-form `+` escaping.
fn percent_encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
    }
    encoded
}

/// Core HTTP client for the DhanHQ REST API v2.
///
/// Wraps [`reqwest::Client`] and injects the required authentication headers
/// into every request. Header values are validated at request time so public
/// credential input cannot panic during client construction or token rotation.
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
#[derive(Clone)]
pub struct DhanClient {
    http: reqwest::Client,
    /// The Dhan client ID (user-specific identification).
    client_id: String,
    /// JWT access token.
    access_token: String,
    /// Base URL for REST API requests (defaults to [`API_BASE_URL`]).
    base_url: String,
}

impl std::fmt::Debug for DhanClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DhanClient")
            .field("client_id", &"[REDACTED]")
            .field("access_token", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
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
        let client_id = client_id.into();
        let access_token = access_token.into();
        let base_url = base_url.into();
        Self::try_with_base_url(client_id.clone(), access_token.clone(), base_url.clone())
            .unwrap_or_else(|_| {
                // Keep the established infallible constructor source-compatible.
                // Invalid credentials are converted to typed errors when a request
                // is attempted, rather than panicking during construction.
                Self::with_unchecked_credentials(client_id, access_token, base_url)
            })
    }

    /// Fallible variant of [`Self::new`] that validates credential header
    /// values at construction time.
    pub fn try_new(client_id: impl Into<String>, access_token: impl Into<String>) -> Result<Self> {
        Self::try_with_base_url(client_id, access_token, API_BASE_URL)
    }

    /// Fallible variant of [`Self::with_base_url`] that validates credential
    /// header values at construction time.
    pub fn try_with_base_url(
        client_id: impl Into<String>,
        access_token: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self> {
        let client_id = client_id.into();
        let access_token = access_token.into();
        Self::validate_credentials(&client_id, &access_token)?;
        Ok(Self::with_unchecked_credentials(
            client_id,
            access_token,
            base_url.into(),
        ))
    }

    fn with_unchecked_credentials(
        client_id: String,
        access_token: String,
        base_url: String,
    ) -> Self {
        let http = reqwest::Client::builder()
            .default_headers(Self::default_headers())
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build reqwest client");

        Self {
            http,
            client_id,
            access_token,
            base_url: base_url.trim_end_matches('/').to_owned(),
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
    }

    /// Validate and replace the access token without deferring invalid-header
    /// errors to a later request.
    pub fn try_set_access_token(&mut self, token: impl Into<String>) -> Result<()> {
        let token = token.into();
        HeaderValue::from_str(&token)?;
        self.access_token = token;
        Ok(())
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
            .headers(self.auth_headers()?)
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
            .headers(self.auth_headers()?)
            .json(body)
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Perform a POST request without a request body and deserialize the
    /// successful JSON response.
    pub async fn post_without_body<R: DeserializeOwned>(&self, path: &str) -> Result<R> {
        let url = self.url(path);
        tracing::debug!(%url, "POST");

        let resp = self
            .http
            .post(&url)
            .headers(self.auth_headers()?)
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
            .headers(self.auth_headers()?)
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
            .headers(self.auth_headers()?)
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
            .headers(self.auth_headers()?)
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = resp
                .text()
                .await
                .map_err(|source| DhanError::ResponseBody { status, source })?;
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
            .headers(self.auth_headers()?)
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = resp
                .text()
                .await
                .map_err(|source| DhanError::ResponseBody { status, source })?;
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
            .headers(self.auth_headers()?)
            .json(body)
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = resp
                .text()
                .await
                .map_err(|source| DhanError::ResponseBody { status, source })?;
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

    /// Per-request auth headers. Credentials are validated here so legacy
    /// infallible constructors cannot panic on public input.
    fn auth_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::with_capacity(2);
        let mut token = HeaderValue::from_str(&self.access_token)?;
        token.set_sensitive(true);
        let mut client_id = HeaderValue::from_str(&self.client_id)?;
        client_id.set_sensitive(true);
        headers.insert("access-token", token);
        headers.insert("client-id", client_id);
        Ok(headers)
    }

    fn validate_credentials(client_id: &str, access_token: &str) -> Result<()> {
        HeaderValue::from_str(client_id)?;
        HeaderValue::from_str(access_token)?;
        Ok(())
    }

    /// Read a response, returning either the deserialized body or a `DhanError`.
    ///
    /// Uses `bytes()` + `serde_json::from_slice()` to avoid the overhead of
    /// UTF-8 validation that `text()` + `from_str()` would incur.
    async fn handle_response<R: DeserializeOwned>(&self, resp: reqwest::Response) -> Result<R> {
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|source| DhanError::ResponseBody { status, source })?;

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
