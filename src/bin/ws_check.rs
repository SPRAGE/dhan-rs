//! Binary to connect to DhanHQ Market Feed WebSocket and subscribe to
//! NIFTY 50 (IDX_I:13) and HDFC Bank (NSE_EQ:1333) for inspecting live data.
//!
//! # Usage
//!
//! ```sh
//! export DHAN_CLIENT_ID="your-client-id"
//! export DHAN_ACCESS_TOKEN="your-access-token"
//! cargo run --bin ws_check --features cli
//! ```

use std::env;
use std::time::Duration;

use dhan_rs::types::enums::FeedRequestCode;
use dhan_rs::ws::market_feed::{Instrument, MarketFeedStream};
use futures_util::StreamExt;
use tokio::time;

#[tokio::main]
async fn main() -> dhan_rs::error::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let client_id =
        env::var("DHAN_CLIENT_ID").expect("set DHAN_CLIENT_ID env var before running");
    let access_token =
        env::var("DHAN_ACCESS_TOKEN").expect("set DHAN_ACCESS_TOKEN env var before running");

    println!("Connecting to DhanHQ Market Feed WebSocket…");
    let mut stream = MarketFeedStream::connect(&client_id, &access_token).await?;

    // Subscribe to NIFTY 50 index (Ticker mode — indices don't support Full)
    let index_instruments = vec![Instrument::new("IDX_I", "13")];
    println!("Subscribing to IDX_I:13 NIFTY 50 (Ticker)…");
    stream
        .subscribe(FeedRequestCode::SubscribeTicker, &index_instruments)
        .await?;

    // Subscribe to HDFC Bank on NSE (Full mode — equity with depth)
    let equity_instruments = vec![Instrument::new("NSE_EQ", "1333")];
    println!("Subscribing to NSE_EQ:1333 HDFC Bank (Full)…");
    stream
        .subscribe(FeedRequestCode::SubscribeFull, &equity_instruments)
        .await?;

    println!("Listening for events for 10 seconds…");
    println!("(Note: data only arrives during market hours 9:15–15:30 IST)\n");

    let deadline = time::sleep(Duration::from_secs(10));
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => {
                println!("\n10 seconds elapsed — disconnecting…");
                break;
            }
            event = stream.next() => {
                match event {
                    Some(Ok(e)) => println!("{e:#?}"),
                    Some(Err(e)) => eprintln!("Error: {e}"),
                    None => {
                        println!("Stream ended by server");
                        break;
                    }
                }
            }
        }
    }

    stream.disconnect().await?;
    println!("Done.");

    Ok(())
}
