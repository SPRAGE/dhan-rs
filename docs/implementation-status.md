# Implementation status and operating boundaries

> Status: implementation snapshot, 2026-08-11. This is an unofficial client,
> not a certification for live trading. The documented behaviors below have
> deterministic local coverage, but have not been verified with authenticated
> Dhan services.

The [compliance audit](dhan-v2-compliance-audit.md) and
[WebSocket stability design](websocket-stability-design.md) are retained as
historical pre-remediation evidence. Read this page for the current code; read
those documents for the original findings, rationale, and remaining source
ambiguities.

## What is implemented

`DhanClient` has typed REST modules for authentication, static IP, profile,
orders, Super Orders, Forever Orders, conditional triggers and multi-order,
portfolio, eDIS, Trader's Control, funds, statements, market quotes,
historical data, option chain, Data APIs, Global Stocks, and instrument-master
downloads. Requests with dynamic route components validate and percent-encode
those values before making a request.

The latest additions and corrections are:

| Area | Typed API or behavior |
|---|---|
| Data APIs | `get_rolling_option_data`, `get_technical_metrics`, `get_market_movers`, and `get_company_info` |
| Instrument masters | `download_compact_instruments_csv`, `download_detailed_instruments_csv`, and `download_segment_instruments_csv` |
| Global Stocks | Orders, estimates, margin, trades, market status, holdings, and fund-limit methods in `api::global_stocks` |
| Conditional/eDIS | `place_multi_order` and `generate_bulk_edis_form` |
| Trader's Control | P&L setup uses `POST /v2/pnlExit`; kill switch is sent without an accidental JSON body |
| Statements | `get_ledger_response` accepts the documented object or a legacy array envelope; `get_ledger` presents a flattened `Vec<LedgerEntry>` |
| Model drift | IP metadata, holdings, order/trade/super-order fields, eDIS quantities, postback fields, `NSE_COMM`, multi-margin naming, and multi-order constraints are modelled or made tolerant |

The CSV downloads are intentionally raw `Bytes`: Dhan owns the CSV schema, so
the crate does not silently impose an unstable row model. The compact and
detailed all-segment downloads are public CDN calls and do not send account
headers. The segment download is a Dhan API request.

### REST contract boundaries

The implementation follows the public v2 HTML documentation where it is
unambiguous and also fills the linked OpenAPI-only operations. It does **not**
claim complete parity because Dhan's official sources conflict in several
places:

- the Forever-order list path is described as both `/v2/forever/all` and
  `/v2/forever/orders`; `get_all_forever_orders` and
  `get_all_forever_orders_openapi` expose the alternatives explicitly;
- consent-flow examples and prose disagree about three request verbs;
- Super Order cancellation is described with incompatible success envelopes;
  `cancel_super_order` models the OpenAPI JSON response and
  `cancel_super_order_no_content` models the HTML empty response;
- multi-margin request/response shapes differ between sources; and
- Full Market Depth disconnect framing is internally inconsistent.

The selected behavior is covered by local request/response contract tests
where possible. Treat a response schema that is intentionally tolerant as
compatibility handling, not proof that every live variant has been observed.

## WebSocket protocols

The standard market feed, order-update feed, 20-level depth feed, and
200-level depth feed are separate protocols. Do not send standard-feed
Full-Depth request codes to the ordinary market-feed API.

| Protocol | Public type | Wire form | Scope |
|---|---|---|---|
| Standard market feed | `MarketFeedStream` | JSON controls, binary events | Ticker, Quote, Full, OI and previous-close data |
| Managed standard feed | `DhanFeedManager` | Same standard protocol | Up to five supervised standard-feed connections |
| Live order update | `OrderUpdateStream` | JSON | Caller-polled individual or partner updates |
| Managed order update | `ManagedOrderUpdate` | JSON | One reconnecting, reconciled order-update owner |
| 20-level depth | `TwentyDepthStream` | Its own JSON subscription envelope and 12-byte binary header | At most 50 instruments per connection |
| 200-level depth | `TwoHundredDepthStream` | Its own single-instrument envelope and 12-byte binary header | One subscribed instrument per connection |

`TwentyDepthStream` and `TwoHundredDepthStream` are deliberately low-level,
caller-polled streams. Keep polling them continuously so Ping/Pong processing
can make progress. They parse their own stacked packet formats, enforce their
own subscription limits, and implement a bounded disconnect/close handshake;
they do not yet have a depth-specific reconnecting supervisor.

### Standard market-feed manager

Create a `DhanFeedManager`, call `start`, then subscribe. Starting the manager
validates configuration but opens no socket; a slot is opened lazily only when
it has desired instruments.

```rust,no_run
use dhan_rs::types::enums::FeedRequestCode;
use dhan_rs::ws::manager::{ConnectionId, DhanFeedConfig, DhanFeedManager};
use dhan_rs::ws::market_feed::Instrument;

# async fn example() -> dhan_rs::Result<()> {
let mut manager = DhanFeedManager::new(
    "client-id",
    "access-token",
    DhanFeedConfig::default(),
);
manager.start().await?;
let mut events = manager
    .get_parsed_channel(ConnectionId(0))
    .expect("configured slot");
manager
    .subscribe(
        &[Instrument::new("NSE_EQ", "1333")],
        FeedRequestCode::SubscribeTicker,
    )
    .await?;

// Read `events.recv().await` and observe `manager.get_health_channel(ConnectionId(0))`.
let _ = &mut events;
// After a reported gap, an authoritative REST snapshot, and replacement data:
// manager.acknowledge_gap(ConnectionId(0)).await?;
manager.shutdown().await?;
# Ok(())
# }
```

The owner task maintains a mode-aware desired subscription set and applies the
latest generation after each reconnect. It uses bounded connect/write/no-frame
and first-valid-data readiness deadlines, bounded close handling, capped
exponential full-jitter retries, token-version
watching through `update_access_token`, and a stable-traffic period before the
retry counter resets. It exposes parsed and optional raw broadcast channels,
lifecycle diagnostics, a `watch` health snapshot, and a previous-close cache.

Transport recovery does not make missed prices reappear. A parse failure,
receiver lag, or uncertain disconnect latches health at `GapDetected` with a
typed cause. Receiving a live packet without any parsed receiver also latches
an explicit `NoReceiver` gap (Previous Close is cached separately). The latch
survives reconnection. Reconcile the desired state from
an authoritative snapshot after the replacement transport is data-live, then
call `acknowledge_gap`; it will not clear the condition before that point.
`shutdown` stops new retries, sends standard-feed RequestCode 12, attempts the
close handshake, joins the task, and uses abort only as a bounded fallback.

### Managed order updates

`OrderUpdateStream` remains the straightforward stream for applications that
own reconnection themselves. For a long-running consumer, use
`ManagedOrderUpdate::start` with a refreshable credential channel and, where
available, an `OrderUpdateReconciler` that fetches the current order state for
each authorized client.

The managed supervisor does not infer success from an authorization write. It
reaches `Live` only after a valid `order_alert`. It has bounded upgrade,
authorization write, no-frame inactivity, close, reconciliation, and overall
reconciliation deadlines; capped full-jitter retry; credential-version
rotation; a stable-connection retry reset; and graceful shutdown.

A legitimately quiet order stream cannot prove authorization because Dhan
documents no positive ACK. After `readiness_timeout`, the owner emits
`ReadinessTimeout` and moves to `ReadinessUnconfirmed` while continuing to poll
the socket. This diagnostic neither claims success nor forces a healthy quiet
connection to churn. A total absence of WebSocket frames for
`inactivity_timeout` does trigger reconnect and gap handling.

An uncertain close, malformed order-alert frame, bounded receiver overflow, or
reconciliation-buffer overflow produces a typed gap. Snapshot reconciliation
is application supplied because a partner WebSocket secret does not authorize
REST snapshots for every client. During reconciliation, live updates are
bounded and merged deterministically with snapshot results; ambiguity emits a
warning and retains the safer state. `GapUnresolved` and `Degraded` mean the
application must treat the affected client/order state as uncertain. Delivery
is not exactly-once and there is no broker replay or resume cursor guarantee.

Every consumer should use `ManagedOrderUpdateReceiver::recv`, not its internal
broadcast receiver: a consumer lag is converted into an explicit
`GapDetected { ConsumerLag { .. } }` event and signals the supervisor.

## Security and error handling

- `DhanClient` redacts client ID and access token in `Debug` output.
- Fallible constructors validate credential header values; established
  infallible constructors defer invalid values to a typed request error rather
  than panicking.
- Account-bearing REST clients disable redirects, preventing credentials from
  being forwarded to a redirect target. Public instrument downloads use a
  separate, bounded-redirect client without account headers.
- Authentication URLs and reqwest errors are sanitized to avoid exposing
  query credentials. Response-body read errors retain the received status.
- Dynamic path and query inputs are validated and component-encoded. This
  keeps IDs such as `a/b` inside one path component instead of changing the
  route.
- REST error responses preserve structured Dhan API errors where available.

Normal logging should still avoid request bodies, tokens, PINs, TOTP values,
and order data. The crate cannot protect credentials copied into application
logs or supplied to third-party telemetry.

## Verification performed locally

The repository contains deterministic local checks for request method/path,
headers and bodies; path encoding; error redaction; response-model tolerance;
standard-feed framing and close handling; market-manager lifecycle and gap
behavior; order-update lifecycle, lag, reconciliation and shutdown; and
20/200-depth subscriptions, framing, Ping/Close handling.

The relevant integration suites are `rest_contracts`, `rest_path_hardening`,
`data_instrument_contracts`, `global_stocks_contracts`,
`remaining_contracts`, `market_manager_stability`, and
`order_update_stability`. The tests use loopback servers and synthetic binary
fixtures; they make no authenticated broker call and do not place, amend, or
cancel live orders.

## Remaining limits and required live validation

Before production use, run a non-destructive, credential-gated validation in a
controlled account that verifies token renewal/rotation, exact REST schemas,
authorized WebSocket handshakes, real standard/depth packet captures,
disconnect codes, and a controlled reconnect soak. Confirm the official
source conflicts listed above directly with Dhan.

In particular, this implementation provides no proof of live authenticated
behavior, no replay of missed market data or order updates, no exactly-once
delivery, no automatic retry for non-idempotent trading REST operations, and
no reconnecting supervisor for either Full Market Depth protocol. Application
code remains responsible for rate limits, risk checks, order intent, and its
own authoritative reconciliation policy.
