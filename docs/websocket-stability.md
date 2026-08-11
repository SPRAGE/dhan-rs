# WebSocket stability guide

> Current implementation guide, 2026-08-11. For the original problem analysis
> and design decisions, see the retained
> [historical stability design](websocket-stability-design.md). This guide does
> not replace live authenticated testing.

## Choose the correct API

Use a low-level stream when the application deliberately owns its socket
lifecycle. Use a manager/supervisor when it needs continuous service,
observable health, retry, and explicit gap handling.

| Need | Use | Important constraint |
|---|---|---|
| Standard ticker, quote, or full packets | `MarketFeedStream` | Caller owns reconnect and state recovery |
| Reliable standard-feed ownership across up to five slots | `DhanFeedManager` | Call `start` before subscription and `shutdown` on exit |
| Individual/partner order alerts with caller-owned lifecycle | `OrderUpdateStream` | A valid order alert, not a write, is the readiness evidence |
| Reconnecting order alerts with gap reconciliation | `ManagedOrderUpdate` | Provide an authorized snapshot reconciler when possible |
| 20-level book | `TwentyDepthStream` | Low-level, caller-polled; max 50 instruments |
| 200-level book | `TwoHundredDepthStream` | Low-level, caller-polled; one instrument |

Full Market Depth is not an option on the standard-feed subscription API.
Standard `SubscribeFullMarketDepth`/`UnsubscribeFullMarketDepth` values are
rejected there; use the separate depth stream and its different request shape.

## Standard market-feed manager lifecycle

1. Construct with `DhanFeedManager::new` or `DhanFeedManagerBuilder`.
2. Call `start`. This validates configuration and endpoint syntax only; it is
   intentionally lazy and does not consume a broker socket.
3. Subscribe valid numeric security IDs using Ticker, Quote, or Full mode.
   The manager places instruments across at most five configured slots and
   honours the 5,000-instrument-per-slot and 100-control-frame limits.
4. Subscribe to a parsed channel, lifecycle channel, and health `watch` for
   each slot you consume.
5. On `GapDetected`, take an authoritative REST snapshot appropriate to the
   subscribed instruments. Wait for the replacement transport to be data-live,
   apply your snapshot, and call `acknowledge_gap(slot)`.
6. On process shutdown, call `shutdown().await` rather than relying on `Drop`.

The supervisor is the sole writer/reader for each socket. Desired subscriptions
are mode-aware and generation tracked; after a reconnect it writes the latest
complete desired state, not a stale initial snapshot. Its retry policy is
capped exponential backoff with full jitter. Upgrade, write, transport
inactivity, first-valid-data readiness, close, and join waits are bounded. A
new token supplied by `update_access_token` is used for future connection
attempts. Retry attempts reset only after a stable traffic period.

### Health and data-quality semantics

`ConnectionLifecycle` answers what the owner task is doing; `MarketDataQuality`
answers whether the stream can be treated as continuous. They are deliberately
separate.

| Condition | Lifecycle / quality implication | Application action |
|---|---|---|
| Socket connecting or resubscribing | `Connecting`/`Resubscribing`; not data-live | Do not assume a quote is current |
| First valid binary data after subscription | `Live`, `Current` or still latched gap | Continue or reconcile |
| Parse failure, receiver lag, or disconnect | `GapDetected` with typed `GapCause` | Fetch and apply an authoritative snapshot |
| Valid packet arrives with no parsed receiver | `GapDetected::NoReceiver` (except cached Previous Close) | Create receivers before subscribing, then reconcile |
| Reconnected but no explicit acknowledgment | May be `Live` while quality remains `GapDetected` | Keep the data-quality gate closed |
| `acknowledge_gap` succeeds | Quality returns to current | Only after app reconciliation |

The manager cannot restore ticks that Dhan did not replay. `previous_close_snapshot`
is a convenience cache for one-shot previous-close events, not a market-data
reconciliation mechanism.

## Order-update supervisor lifecycle

Create the credential channel with `order_update_credential_channel`, then
pass the receiver to `ManagedOrderUpdate::start`. Replace credentials through
the updater when tokens rotate. The supervisor observes versions and reconnects
using the current snapshot. It supports both SELF and PARTNER credential
snapshots; partner reconciliation must authorize REST snapshots for each client
independently.

Provide `Some(Arc<dyn OrderUpdateReconciler>)` when the application can fetch
the current order book for a client. It is called after uncertainty under the
configured per-client, overall, and concurrency limits. Without it, the
supervisor truthfully emits unresolved gaps rather than pretending a reconnect
restored order state.

The `ManagedOrderUpdateState` and `ManagedOrderUpdateEvent` channels are the
control plane. Important states/events are:

- `ReadinessPending`: authorization was written but no valid `order_alert` has
  yet proved the data plane live.
- `ReadinessUnconfirmed` / `ReadinessTimeout`: the configurable diagnostic
  deadline elapsed on a quiet connection. The owner keeps polling and does not
  invent an authentication ACK; complete frame inactivity is handled by the
  separate reconnect deadline.
- `GapDetected`: an uncertain disconnect, malformed application frame,
  consumer lag, or reconciliation-buffer overflow made state incomplete.
- `Reconciling`, `Reconciled`, `GapUnresolved`, and `Degraded`: a snapshot
  attempt and its bounded outcome. `Degraded` is not a clean bill of health.
- `ManagedOrderUpdateReceiver::recv`: turns a bounded broadcast lag into an
  explicit consumer-gap event and informs the owner task.

During a snapshot fetch, live updates are bounded and merged with the snapshot
without allowing a terminal order state to roll back. The stream is
at-least-observed, may deduplicate, and is neither exactly once nor replayed.

## Full Market Depth operating guidance

Both depth streams are caller-polled. Keep an active `StreamExt::next` loop so
Ping frames are read and automatic Pong frames can flush. They use a bounded
five-second peer-close wait when `disconnect` is called. A caller that stops
polling can miss the keepalive window and be closed by the peer.

20-level and 200-level packets use independent typed headers, stacked-packet
parsers, and disconnect variants because the official disconnect description
is inconsistent. Parser coverage proves handling of the documented variants;
it is not evidence that a live broker will never send another layout.

There is currently no `DhanDepthManager`. If depth continuity matters, wrap a
depth stream with your own single-owner reconnect loop, retain a separate depth
snapshot/reconciliation policy, and avoid treating a reconnect as lossless.

## Failure-handling checklist

- Set bounded application receiver capacities deliberately; overflow is a
  data-quality event, not merely a throughput metric.
- Monitor lifecycle and health even if the parsed-event receiver is quiet.
- Treat `NoReceiver`, `ReceiverLag`, parse errors, peer closes, and all
  `GapUnresolved` events as operational signals requiring action.
- Rotate access tokens before expiry and monitor the credential version and
  authentication-blocked state.
- Do not retry order placement, modification, or cancellation merely because
  a WebSocket or HTTP transport failed; first reconcile authoritative order
  state.
- Redact URLs and credentials from application logging. WebSocket query
  authentication makes this especially important.
- Run a controlled authenticated reconnect soak before relying on any feed for
  trading decisions.

## Test evidence and limits

Local loopback tests cover Ping-before-close behavior, close-handshake bounds,
subscription restoration, retry/jitter paths, token changes, durable
market-data gaps and acknowledgement, malformed order alerts, lag signalling,
reconciliation timeouts/merging, and bounded shutdown. Binary tests use
synthetic fixtures. No test has authenticated with Dhan, captured live traffic,
or proved broker-side replay/resume behavior.
