//! Constants for the DhanHQ API v2.
//!
//! Contains base URLs, WebSocket endpoints, and rate limit values.
//! These are used internally by [`DhanClient`](crate::client::DhanClient)
//! and the WebSocket stream types, but are also exported for advanced usage.

// ---------------------------------------------------------------------------
// Base URLs
// ---------------------------------------------------------------------------

/// Base URL for the DhanHQ REST API v2.
pub const API_BASE_URL: &str = "https://api.dhan.co";

/// Base URL for authentication endpoints.
pub const AUTH_BASE_URL: &str = "https://auth.dhan.co";

// ---------------------------------------------------------------------------
// WebSocket URLs
// ---------------------------------------------------------------------------

/// WebSocket endpoint for live market feed (binary).
pub const WS_MARKET_FEED_URL: &str = "wss://api-feed.dhan.co";

/// WebSocket endpoint for live order updates (JSON).
pub const WS_ORDER_UPDATE_URL: &str = "wss://api-order-update.dhan.co";

/// WebSocket endpoint for 20-level full market depth (binary).
pub const WS_DEPTH_20_URL: &str = "wss://depth-api-feed.dhan.co/twentydepth";

/// WebSocket endpoint for 200-level full market depth (binary).
pub const WS_DEPTH_200_URL: &str = "wss://full-depth-api.dhan.co/twohundreddepth";

// ---------------------------------------------------------------------------
// Rate Limits
// ---------------------------------------------------------------------------

/// Rate limit configuration for the different API categories.
pub mod rate_limits {
    /// Orders API rate limits.
    pub mod orders {
        /// Maximum order requests per second.
        pub const PER_SECOND: u32 = 10;
        /// Maximum order requests per minute.
        pub const PER_MINUTE: u32 = 250;
        /// Maximum order requests per hour.
        pub const PER_HOUR: u32 = 1000;
        /// Maximum order requests per day.
        pub const PER_DAY: u32 = 7000;
        /// Maximum number of modifications allowed per single order.
        pub const MAX_MODIFICATIONS_PER_ORDER: u32 = 25;
    }

    /// Data API (REST snapshots) rate limits.
    pub mod data {
        /// Maximum data requests per second.
        pub const PER_SECOND: u32 = 5;
        /// Maximum data requests per day.
        pub const PER_DAY: u32 = 100_000;
    }

    /// Historical data API rate limits.
    pub mod historical {
        /// Maximum historical data requests per second.
        pub const PER_SECOND: u32 = 1;
    }

    /// Instrument API rate limits.
    pub mod instruments {
        /// Maximum instrument API requests per second.
        pub const PER_SECOND: u32 = 20;
    }

    /// Market quote (REST) constraints.
    pub mod market_quote {
        /// Maximum instruments per single LTP/OHLC/Quote request.
        pub const MAX_INSTRUMENTS_PER_REQUEST: u32 = 1000;
        /// Maximum market quote requests per second.
        pub const PER_SECOND: u32 = 1;
    }

    /// Option chain API constraints.
    pub mod option_chain {
        /// Minimum interval between unique requests (in seconds).
        pub const MIN_INTERVAL_SECS: u32 = 3;
    }

    /// WebSocket constraints.
    pub mod websocket {
        /// Maximum concurrent WebSocket connections per user.
        pub const MAX_CONNECTIONS: u32 = 5;
        /// Maximum instruments per single connection.
        pub const MAX_INSTRUMENTS_PER_CONNECTION: u32 = 5000;
        /// Maximum instruments per single subscribe message.
        pub const MAX_INSTRUMENTS_PER_SUBSCRIBE: u32 = 100;
        /// Server ping interval in seconds.
        pub const PING_INTERVAL_SECS: u32 = 10;
        /// Connection timeout if no pong response (in seconds).
        pub const PONG_TIMEOUT_SECS: u32 = 40;
    }
}
