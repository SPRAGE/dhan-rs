use std::fmt;

/// Error response returned by the DhanHQ API.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorBody {
    /// Category of the error (e.g. "Invalid Authentication").
    #[serde(default)]
    pub error_type: Option<String>,
    /// Dhan error code (e.g. "DH-901").
    #[serde(default)]
    pub error_code: Option<String>,
    /// Human-readable description of the error.
    #[serde(default)]
    pub error_message: Option<String>,
}

impl fmt::Display for ApiErrorBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {}: {}",
            self.error_code.as_deref().unwrap_or("UNKNOWN"),
            self.error_type.as_deref().unwrap_or("Unknown Error"),
            self.error_message.as_deref().unwrap_or("No message"),
        )
    }
}

/// All possible errors produced by the `dhan-rs` client.
#[derive(Debug, thiserror::Error)]
pub enum DhanError {
    /// An error response returned by the DhanHQ REST API.
    #[error("API error: {0}")]
    Api(ApiErrorBody),

    /// The server returned an unexpected HTTP status code.
    #[error("HTTP {status}: {body}")]
    HttpStatus {
        status: reqwest::StatusCode,
        body: String,
    },

    /// A network or transport-level error from `reqwest`.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Failed to deserialize a JSON response body.
    #[error("JSON deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// A WebSocket-level error.
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    /// An error building or parsing a URL.
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),

    /// The caller provided an invalid argument.
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, DhanError>;
