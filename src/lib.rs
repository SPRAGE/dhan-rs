//! # dhan-rs
//!
//! A Rust client library for the [DhanHQ Broker API v2](https://dhanhq.co/docs/v2/).
//!
//! ## Quick Start
//!
//! ```no_run
//! use dhan_rs::client::DhanClient;
//!
//! #[tokio::main]
//! async fn main() -> dhan_rs::error::Result<()> {
//!     let client = DhanClient::new("your-client-id", "your-access-token");
//!     // Use client.get(), client.post(), etc. to interact with the API.
//!     Ok(())
//! }
//! ```

pub mod api;
pub mod client;
pub mod constants;
pub mod error;
pub mod types;

/// Re-export the main client type at crate root for convenience.
pub use client::DhanClient;
/// Re-export the error type and Result alias.
pub use error::{DhanError, Result};
