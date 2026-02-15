//! REST API endpoint implementations.
//!
//! Each sub-module adds high-level `async` methods to
//! [`DhanClient`](crate::client::DhanClient) via `impl` blocks. All methods
//! handle JSON serialization, HTTP transport, and error mapping automatically.
//!
//! ## Usage
//!
//! Simply import the relevant types and call methods on your `DhanClient`:
//!
//! ```no_run
//! use dhan_rs::DhanClient;
//!
//! # #[tokio::main]
//! # async fn main() -> dhan_rs::Result<()> {
//! let client = DhanClient::new("client-id", "token");
//! let orders = client.get_orders().await?;
//! let holdings = client.get_holdings().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Modules
//!
//! | Module | Endpoints | Description |
//! |---|---|---|
//! | [`orders`] | 10 | Order CRUD, slicing, trade book |
//! | [`super_order`] | 4 | Bracket/cover order management |
//! | [`forever_order`] | 4 | GTT/OCO order management |
//! | [`conditional`] | 5 | Alert-based conditional triggers |
//! | [`portfolio`] | 4 | Holdings, positions, exit all |
//! | [`funds`] | 3 | Margin calculator, fund limits |
//! | [`market_quote`] | 3 | LTP, OHLC, full depth |
//! | [`historical`] | 2 | Daily & intraday candles |
//! | [`option_chain`] | 2 | Option chain, expiry lists |
//! | [`auth`] | 8 | Token generation, consent flows |
//! | [`profile`] | 1 | User profile |
//! | [`ip`] | 3 | Static IP management |
//! | [`edis`] | 3 | T-PIN, eDIS form, inquiry |
//! | [`traders_control`] | 5 | Kill switch, P&L-based exit |
//! | [`statements`] | 2 | Ledger, trade history |

pub mod auth;
pub mod conditional;
pub mod edis;
pub mod forever_order;
pub mod funds;
pub mod historical;
pub mod ip;
pub mod market_quote;
pub mod option_chain;
pub mod orders;
pub mod portfolio;
pub mod profile;
pub mod statements;
pub mod super_order;
pub mod traders_control;
