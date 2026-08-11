# DhanHQ API v2 compliance audit

> - Status: read-only audit completed on 2026-08-11
> - Repository baseline: `dhan-rs` on branch `main` at `c819d04fbfb23b1d43e3c39a49605bf2e02440d8`
> - Outcome: partial route coverage; confirmed REST contract defects; WebSocket recovery is not production-safe

> [!NOTE]
> This is a historical, pre-remediation audit. The implementation has changed.
> See [Implementation status and operating boundaries](implementation-status.md)
> for the current code and retained validation limits.

## Purpose

This document records how the current `dhan-rs` implementation compares with
the public DhanHQ API v2 documentation. It covers:

- REST method and path coverage;
- request and response model compatibility;
- standard market-feed WebSocket compatibility;
- order-update WebSocket compatibility;
- Full Market Depth support;
- connection lifecycle and recovery behavior;
- credential-handling and observability risks; and
- the evidence and limitations of the audit.

The audit does not certify the crate for live trading. No valid account
credentials were available, so order placement, account data, authenticated
market data, and authenticated order updates were not exercised.

The detailed WebSocket remediation design is in
[WebSocket stability design](websocket-stability-design.md).

## Executive summary

The crate implements much of Dhan's established v2 REST surface and maps the
standard market-feed packet layouts reasonably well. It is not, however,
fully compliant with either of Dhan's current official contract sources.

The most important conclusions are:

1. `set_pnl_exit` uses `PUT`, while both official sources require `POST`.
2. `get_ledger` expects an array, while both official sources describe one
   response object.
3. rolling expired-options data and the instrument-list retrieval APIs are
   missing from the HTML-documented surface.
4. the linked OpenAPI additionally exposes Global Stocks and several data,
   eDIS, and conditional-order operations that have no typed crate API.
5. 20-level and 200-level Full Market Depth are not implemented, despite
   public constants and request codes suggesting support.
6. market-feed manager reconnects do not restore subscriptions made through
   normal usage.
7. order updates have no managed reconnect or post-gap REST reconciliation.
8. access tokens, PINs, TOTPs, and order information can enter debug logs.
9. there are no deterministic WebSocket lifecycle or packet-parser tests.

## Status terminology

| Term | Meaning |
|---|---|
| Confirmed match | The current implementation agrees with the relevant published method, path, or wire layout. |
| Confirmed defect | The implementation contradicts official sources that agree with each other, or its lifecycle logic is demonstrably incorrect. |
| Missing | No typed public crate operation implements the documented capability. The generic HTTP escape hatch is not counted as typed support. |
| Source conflict | Dhan's HTML documentation and linked OpenAPI disagree, so live behavior cannot be inferred safely from documentation alone. |
| Unverified | Static inspection found a likely issue, but valid credentials or representative live fixtures are required to confirm server behavior. |

## Official sources

The audit used the public pages as available on 2026-08-11:

- [DhanHQ API v2 documentation](https://dhanhq.co/docs/v2/)
- [Linked OpenAPI document](https://api.dhan.co/v2/v3/api-docs)
- [Authentication](https://dhanhq.co/docs/v2/authentication/)
- [Orders](https://dhanhq.co/docs/v2/orders/)
- [Super Orders](https://dhanhq.co/docs/v2/super-order/)
- [Forever Orders](https://dhanhq.co/docs/v2/forever/)
- [Conditional Trigger](https://dhanhq.co/docs/v2/conditional-trigger/)
- [Portfolio](https://dhanhq.co/docs/v2/portfolio/)
- [eDIS](https://dhanhq.co/docs/v2/edis/)
- [Trader's Control](https://dhanhq.co/docs/v2/traders-control/)
- [Funds and Margin](https://dhanhq.co/docs/v2/funds/)
- [Statements](https://dhanhq.co/docs/v2/statements/)
- [Instruments](https://dhanhq.co/docs/v2/instruments/)
- [Market Quote](https://dhanhq.co/docs/v2/market-quote/)
- [Historical Data](https://dhanhq.co/docs/v2/historical-data/)
- [Expired Options Data](https://dhanhq.co/docs/v2/expired-options-data/)
- [Option Chain](https://dhanhq.co/docs/v2/option-chain/)
- [Postback](https://dhanhq.co/docs/v2/postback/)
- [Live Market Feed](https://dhanhq.co/docs/v2/live-market-feed/)
- [Live Order Update](https://dhanhq.co/docs/v2/order-update/)
- [Full Market Depth](https://dhanhq.co/docs/v2/full-market-depth/)
- [Annexure](https://dhanhq.co/docs/v2/annexure/)

The HTML documentation and linked OpenAPI are both official Dhan sources, but
they do not describe the same API surface. Coverage is therefore reported
against both baselines.

## Method

The audit used the following process:

1. inventory every callable operation in the HTML documentation;
2. inventory every operation in the linked OpenAPI document;
3. map each method and path to the crate's typed `DhanClient` methods;
4. compare important request and response schemas;
5. compare WebSocket URLs, authentication envelopes, request codes, response
   codes, headers, endianness, field offsets, and connection limits;
6. inspect task ownership, reconnect, subscription tracking, token renewal,
   shutdown, backpressure, and error propagation;
7. run compilation, lint, formatting, and test checks; and
8. perform one invalid-credential market-feed probe that could not affect an
   account.

Route coverage does not imply schema compatibility. An operation can have the
right method and path while still using incompatible fields or response types.

## REST coverage

### HTML documentation baseline

The HTML site describes 62 HTTP integrations when its eight authentication
flow calls/URLs and three instrument retrieval modes are counted. Postback is
an inbound webhook and is reported separately.

| Category | Documented | Typed crate surface | Assessment |
|---|---:|---:|---|
| Authentication flows | 8 | 8 | Three verb ambiguities |
| Static IP | 3 | 3 | Paths match |
| Profile | 1 | 1 | Path matches |
| Conditional Trigger | 5 | 5 | Paths match |
| eDIS | 3 | 3 | Paths match; type conflict remains |
| Expired Options | 1 | 0 | Missing |
| Forever Orders | 4 | 4 | List path conflicts between official sources |
| Funds and Margin | 3 | 3 | Multi-margin schema conflicts |
| Historical Data | 2 | 2 | Paths match |
| Instruments | 3 | 0 | Missing |
| Market Quote | 3 | 3 | Paths match |
| Option Chain | 2 | 2 | Paths match |
| Orders | 9 | 9 | Paths match |
| Portfolio | 4 | 4 | Paths match |
| Statements | 2 | 2 | Ledger response model mismatch |
| Super Orders | 4 | 4 | Paths match; cancel response conflict remains |
| Trader's Control | 5 | 5 | P&L setup uses wrong verb |
| **Total** | **62** | **58** | **53 exact routes, 5 partial or ambiguous, 4 missing** |

The four missing HTML-documented integrations are:

- `POST /v2/charts/rollingoption`;
- compact instrument CSV retrieval;
- detailed instrument CSV retrieval; and
- segment-wise instrument retrieval through `/instrument/{exchangeSegment}`.

### Linked OpenAPI baseline

The linked OpenAPI document also contains 62 operations, but they are a
different set.

| OpenAPI tag | Operations | Exact method/path | Partial | Missing |
|---|---:|---:|---:|---:|
| IP Setup | 3 | 3 | 0 | 0 |
| Orders | 10 | 10 | 0 | 0 |
| Super Order | 4 | 4 | 0 | 0 |
| Conditional and Multi Order | 6 | 5 | 0 | 1 |
| Forever Order | 4 | 3 | 1 | 0 |
| Positions and Portfolio | 4 | 4 | 0 | 0 |
| eDIS | 4 | 3 | 0 | 1 |
| Trader's Control | 5 | 4 | 1 | 0 |
| Funds and Margin | 3 | 3 | 0 | 0 |
| Statements | 1 | 1 | 0 | 0 |
| Data APIs | 6 | 2 | 0 | 4 |
| Global Stocks | 12 | 0 | 0 | 12 |
| **Total** | **62** | **42** | **2** | **18** |

The 18 OpenAPI operations with no typed implementation are:

- `POST /alerts/multi/orders`;
- `POST /edis/bulkform`;
- `POST /charts/rollingoption`;
- `POST /data/technical`;
- `POST /data/marketmovers`;
- `POST /data/companyinfo`;
- `GET /globalstocks/orders`;
- `POST /globalstocks/orders`;
- `GET /globalstocks/orders/{order-id}`;
- `PUT /globalstocks/orders/{order-id}`;
- `DELETE /globalstocks/orders/{order-id}`;
- `POST /globalstocks/transEstimate`;
- `POST /globalstocks/margincalculator`;
- `GET /globalstocks/trades`;
- `GET /globalstocks/trades/{security-id}`;
- `GET /globalstocks/marketstatus`;
- `GET /globalstocks/holdings`; and
- `GET /globalstocks/fundlimit`.

The generic `get`, `post`, `put`, and `delete` methods can call arbitrary
routes, but that is not equivalent to a documented, typed, tested crate API.

## Confirmed REST defects

### REST-001: P&L setup uses the wrong HTTP verb

- Severity: high
- Status: confirmed defect

Both the HTML Trader's Control page and OpenAPI specify:

```text
POST /v2/pnlExit
```

The implementation sends:

```text
PUT /v2/pnlExit
```

Evidence: [`src/api/traders_control.rs`](../src/api/traders_control.rs#L26)

Impact: a conforming server will reject the configuration request or route it
incorrectly. The method documentation also repeats the incorrect verb.

Required correction:

- use `POST`;
- update the Rust documentation;
- add a local HTTP contract test asserting method, path, headers, and body; and
- add a credential-gated sandbox test if Dhan exposes this operation there.

### REST-002: ledger response shape is incompatible

- Severity: high
- Status: confirmed contract mismatch; runtime not authenticated

The implementation returns `Result<Vec<LedgerEntry>>`:

Evidence: [`src/api/statements.rs`](../src/api/statements.rs#L8)

Both official contracts describe a single ledger response object. If the
server follows those contracts, deserialization into a vector fails before the
caller can inspect any ledger data.

Required correction:

- model the documented response object;
- retain tolerant handling only if captured Dhan responses demonstrate more
  than one shape; and
- add object and malformed-response fixtures.

### REST-003: multi-margin serialization conflicts with OpenAPI

- Severity: high
- Status: official-source conflict; likely incompatible with OpenAPI

The crate serializes:

- `includeOrders` from `include_orders`; and
- `scripts` from `scripts`.

The OpenAPI document specifies:

- `includeOrder`; and
- `scripList`.

Evidence: [`src/types/funds.rs`](../src/types/funds.rs#L69)

The `#[serde(alias = "scripList")]` attribute affects deserialization only. It
does not rename the serialized `scripts` field. The response model also expects
snake-case strings, while OpenAPI specifies camelCase numeric values.

The HTML page contains examples of both naming schemes, so backend truth must
be established with Dhan or a credentialed contract capture before choosing a
single strict schema.

Required correction:

- capture a real request/response or obtain Dhan clarification;
- serialize the accepted wire names explicitly;
- use tolerant numeric/string deserializers where live evidence requires them;
  and
- keep ambiguity fixtures so a later documentation update is visible.

### REST-004: response model drift

- Severity: medium
- Status: OpenAPI drift; runtime impact varies by serde behavior

The linked OpenAPI includes fields or variants absent from current types:

| Crate model | OpenAPI addition or difference | Evidence |
|---|---|---|
| `IpInfo` | `detectedIP`, `ipMatchStatus`, `ordersAllowed` | [`src/types/auth.rs`](../src/types/auth.rs#L89) |
| `Holding` | `mtf_t1_qty`, `mtf_qty`, `lastTradedPrice` | [`src/types/portfolio.rs`](../src/types/portfolio.rs#L13) |
| `OrderDetail` | `exchangeOrderId` | [`src/types/orders.rs`](../src/types/orders.rs#L114) |
| `SuperOrderDetail` | `algoId` | [`src/types/super_order.rs`](../src/types/super_order.rs#L84) |
| `TradeDetail` | `customSymbol` | [`src/types/orders.rs`](../src/types/orders.rs#L165) |
| `ExchangeSegment` | `NSE_COMM` variant | [`src/types/enums.rs`](../src/types/enums.rs#L14) |
| `EdisInquiry` | quantity fields documented as strings in OpenAPI/table | [`src/types/edis.rs`](../src/types/edis.rs#L46) |
| `PnlExitConfig` | numeric `profit`/`loss`; alternate kill-switch key | [`src/types/traders_control.rs`](../src/types/traders_control.rs#L47) |

Unknown JSON fields are normally ignored, so some additions only prevent the
caller from accessing new data. Missing enum variants and incompatible scalar
types can cause complete deserialization failures.

### REST-005: response-body assumptions are unsafe

- Severity: medium
- Status: source conflict or unverified

- The HTML Super Order cancel documentation describes a `202` response with no
  body, while the implementation expects `OrderResponse`. The linked OpenAPI
  describes a JSON response instead. A documented empty response would fail
  JSON decoding.
- The kill-switch documentation says there is no request body, while the crate
  sends `{}`.

Evidence:

- [`src/api/super_order.rs`](../src/api/super_order.rs#L32)
- [`src/api/traders_control.rs`](../src/api/traders_control.rs#L8)

The HTTP layer should be able to model successful empty responses explicitly
instead of forcing every success through JSON deserialization.

### REST-006: body-read failures are hidden

- Severity: medium
- Status: confirmed implementation defect

`handle_response` converts a `resp.bytes()` error into an empty body using
`unwrap_or_default()`.

Evidence: [`src/client.rs`](../src/client.rs#L278)

Impact: a network/body failure can appear as an unrelated JSON end-of-file
error. This loses the original transport cause and makes retry decisions less
reliable.

The error path also drops useful response metadata when a body happens to match
the structured Dhan error shape. Status, headers, and `Retry-After` should be
retained.

### REST-007: request limits are constants, not enforcement

- Severity: medium
- Status: confirmed behavior

The crate defines documented rate and request limits but does not enforce them.
There is no request deadline, limiter, `Retry-After` handling, or automatic
backoff for `429` responses.

This can be a valid library design if made explicit, but callers must not infer
protection from the presence of constants.

### REST-008: malformed public credential values can panic

- Severity: medium
- Status: confirmed implementation defect

Client construction and access-token replacement convert public strings into
HTTP header values with `expect`. A value containing invalid header characters
therefore panics instead of returning a typed error.

Evidence:

- [`src/client.rs`](../src/client.rs#L60)
- [`src/client.rs`](../src/client.rs#L103)

Required correction:

- make client construction and credential replacement fallible;
- preserve the `InvalidHeaderValue` source in `DhanError`;
- mark secret header values as sensitive; and
- test newline and other invalid-header inputs without panicking.

## Official-source conflicts

These items must not be labelled as definite runtime bugs without Dhan
clarification or credentialed evidence.

### Forever Order list path

- the HTML section example and crate use `GET /v2/forever/all`;
- the HTML summary table and OpenAPI specify `GET /forever/orders`.

Evidence: [`src/api/forever_order.rs`](../src/api/forever_order.rs#L38)

### Authentication verbs

Three HTML cURL examples omit `--request`, which makes literal cURL use `GET`:

- individual consent consumption;
- partner consent generation; and
- partner consent consumption.

The crate sends `POST`. The prose does not state the verbs clearly. Invalid
credential probes returned `401` for both `GET` and `POST`, so authentication
was evaluated before route semantics and the probe could not distinguish them.

Evidence: [`src/api/auth.rs`](../src/api/auth.rs#L193)

### Multi-margin and eDIS scalar types

The HTML, tables, examples, and OpenAPI contain different names and scalar
types. These require tolerant parsing backed by real fixtures rather than a
blind choice between official sources.

## Security and privacy findings

### SEC-001: authentication URL logs expose PIN and TOTP

Severity: high

Direct token generation places client ID, PIN, and TOTP in the URL and logs the
complete URL at debug level.

Evidence: [`src/api/auth.rs`](../src/api/auth.rs#L43)

Consent token IDs are logged through URLs in the same module. Authentication
logs should contain only an operation name and a redacted correlation value.

### SEC-002: secret-bearing types derive `Debug`

Severity: high

`DhanClient` and `TokenResponse` derive `Debug` while containing access tokens.

Evidence:

- [`src/client.rs`](../src/client.rs#L35)
- [`src/types/auth.rs`](../src/types/auth.rs#L17)

A routine `tracing::debug!(?client)` or error context can therefore disclose
credentials. Secret fields should use redacted wrappers or manual `Debug`
implementations.

### SEC-003: cross-origin redirects can retain custom credentials

Severity: high

The REST client uses reqwest's default redirect policy and sends Dhan
credentials in custom `access-token` and `client-id` headers.

Evidence: [`src/client.rs`](../src/client.rs#L60)

Standard `Authorization` and cookies receive special cross-origin handling;
custom secret headers should not be assumed to receive equivalent protection.
Broker and authentication clients should reject redirects unless a narrowly
validated same-origin policy is required.

### SEC-004: malformed order updates can leak full payloads

Severity: medium

Order-update JSON parse failures log the complete raw text. That data can
contain client ID, order ID, instrument, quantity, prices, remarks, and
timestamps.

Evidence: [`src/ws/order_update.rs`](../src/ws/order_update.rs#L345)

Logs should record the serde error, message length, event category, and a
bounded hash or correlation identifier—not the complete payload.

## WebSocket protocol assessment

### Confirmed standard market-feed matches

| Area | Assessment |
|---|---|
| URL and query | `wss://api-feed.dhan.co` with `version=2`, token, client ID, and `authType=2` matches. |
| Subscription JSON | `RequestCode`, `InstrumentCount`, and `InstrumentList` match. |
| Standard response header | Eight-byte little-endian parsing matches the documented feed. |
| Packet layouts | Ticker, Previous Close, Quote, OI, and Full offsets and sizes match. |
| Standard request/response codes | Values match the annexure. |
| Standalone disconnect | Sends the documented JSON `RequestCode: 12`. |

Evidence: [`src/ws/market_feed.rs`](../src/ws/market_feed.rs#L295)

The existing Ping branches are not by themselves a defect. Tungstenite queues
Pong responses while reads continue. The real lifecycle gap is that control
progress depends on continued polling and there is no inactivity deadline.

### Confirmed order-update matches

- the order-update URL is correct;
- SELF authentication matches `LoginReq`, message code `42`, client ID, token,
  and `UserType=SELF`;
- PARTNER authentication matches the intended envelope and valid JSON form;
- currently documented order-update keys are represented; and
- the Postback model covers the current documented keys, including
  `filled_qty`.

Evidence:

- [`src/ws/order_update.rs`](../src/ws/order_update.rs#L44)
- [`src/types/postback.rs`](../src/types/postback.rs#L27)

### Full Market Depth is not implemented

- Severity: high
- Status: confirmed gap with misleading public surface

The crate publishes URLs for 20-level and 200-level depth and exposes request
codes `23` and `24`. The usable streams and manager nevertheless connect only
to the standard market-feed endpoint and parse only its eight-byte header.

Evidence:

- [`src/constants.rs`](../src/constants.rs#L21)
- [`src/types/enums.rs`](../src/types/enums.rs#L279)
- [`src/ws/market_feed.rs`](../src/ws/market_feed.rs#L530)

Dhan's Full Market Depth protocols use:

- separate 20-level and 200-level endpoints;
- a different authentication URL shape;
- 12-byte headers;
- response codes `41` and `51`;
- stacked packets and different payload layouts; and
- different instrument limits.

Passing codes `23` or `24` to the current generic standard-feed subscription
API does not provide Full Market Depth support and may issue an invalid request
on the wrong protocol.

## WebSocket lifecycle findings

The full failure analysis and target design are in
[WebSocket stability design](websocket-stability-design.md). The highest-risk
findings are:

1. reconnect captures a subscription snapshot before normal subscriptions are
   created, so reconnect usually restores nothing;
2. only one failed reconnect attempt is made;
3. successful reconnects recursively retain another future layer;
4. partial `start()` failure leaves previously started tasks alive;
5. default startup consumes all five documented sockets before subscriptions
   exist;
6. manager shutdown skips Dhan's JSON disconnect request and immediately
   aborts reader tasks;
7. order updates have no reconnect, token refresh, or REST reconciliation;
8. close codes, close reasons, parse failures, and reconnect failures do not
   reach consumers as durable lifecycle state;
9. early one-shot packets can be lost when no broadcast receiver exists; and
10. `reconnect_count` is exposed but never incremented.

Evidence: [`src/ws/manager.rs`](../src/ws/manager.rs#L409)

## Public documentation drift

The audit found that the root README overstated coverage, had inaccurate method
counts, and contained three quick-start examples that did not compile against
the public types. The accompanying documentation revision corrected those
examples and now links this audit.

Corrections included:

- constructing every required `PlaceOrderRequest` field explicitly;
- treating `MarketQuoteRequest` as `HashMap<String, Vec<u64>>`;
- using the current PascalCase `OptionChainRequest` fields and response
  envelope; and
- reporting 56 current async API methods and nine order methods without
  presenting that count as complete Dhan coverage.

Evidence:

- [`README.md` place-order example](../README.md#L82)
- [`README.md` market-quote example](../README.md#L139)
- [`README.md` option-chain example](../README.md#L212)

These examples should become compiled doctests or dedicated compile tests so
documentation cannot drift silently.

## Verification evidence

The following safe checks were run during the audit:

| Check | Result | Interpretation |
|---|---|---|
| `cargo check --all-features` | Passed | The current crate compiles with all features. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed | No clippy warning remained under this command. |
| `cargo test --doc --all-features` | 16 passed | Rust doctests compile and pass. README snippets are not included. |
| `cargo test --lib --all-features` | 0 tests | There are no library unit tests. |
| `cargo test --all-features` with network access | 23 sandbox tests reported passed | Twenty credential-gated tests returned early and did not exercise Dhan. |
| `cargo fmt --all -- --check` | Failed | Pre-existing formatting drift exists in `tests/sandbox.rs`. |
| Temporary README compile harness | Passed | All six current quick-start examples type-checked without running network calls. |

There are no unit or mock-server tests for:

- binary packet parsing and declared-length validation;
- WebSocket authentication lifecycle;
- Ping/Pong and inactivity behavior;
- partial manager startup rollback;
- repeated reconnect failures;
- current-state resubscription;
- token rotation;
- broadcast lag or zero-receiver behavior;
- graceful shutdown and task cleanup; or
- order-update gap reconciliation.

### Invalid-credential WebSocket probe

A standard market-feed connection was attempted using deliberately invalid
credentials. No account or valid secret was involved.

Observed sequence:

1. the client logged that the WebSocket was connected;
2. it sent subscription requests;
3. the connection failed with `Connection reset without closing handshake`;
4. the stream ended; and
5. cleanup returned `WebSocket(AlreadyClosed)`.

This confirms that the current "connected" message represents only the
transport upgrade, not successful asynchronous Dhan authorization. It also
shows that cleanup is not idempotent after a server-side reset.

## Verification limits

This audit proves source-level agreement, disagreement, and local lifecycle
behavior. It does not prove:

- successful authentication against a real Dhan account;
- live method semantics where official sources conflict;
- real response shapes for multi-margin, eDIS, P&L status, or Forever Orders;
- live binary values for every exchange segment and packet type;
- server ordering, duplicate, replay, or delivery guarantees; or
- production behavior during token renewal and connection-limit events.

Those require a credential-gated test environment, sanitized captured fixtures,
or written clarification from Dhan.

## Remediation order

### P0: prevent silent trading-data loss

1. replace the market-feed reconnect implementation with an iterative owner
   task and current desired-subscription state;
2. add repeated retry, timeouts, failure classification, and truthful health;
3. add managed order-update reconnect plus REST gap reconciliation; and
4. add deterministic local WebSocket lifecycle tests.

### P1: repair definite contracts and credential handling

1. change P&L setup to `POST`;
2. correct the ledger response model;
3. remove secrets and raw order payloads from logs and `Debug` output;
4. disable or constrain redirects for authenticated clients;
5. propagate body-read errors and retain response status/headers; and
6. make startup and shutdown transactional.

### P2: close coverage gaps

1. implement Full Market Depth as separate protocols;
2. add rolling expired-options and instrument-list APIs;
3. decide whether linked-OpenAPI Global Stocks and data operations are in crate
   scope;
4. reconcile response model drift with captured fixtures; and
5. turn README examples into compile-checked documentation.

## Completion criteria for a future compliance claim

The crate should claim Dhan v2 compliance only when all of the following are
true:

- one named official contract baseline and its version/date are recorded;
- every in-scope operation has an exact method/path/body contract test;
- documented response examples deserialize through fixtures;
- every deliberate omission is listed publicly;
- standard feed and Full Market Depth are described as distinct protocols;
- reconnect tests prove current-state resubscription and no orphan tasks;
- order-update recovery proves REST reconciliation after a simulated gap;
- credential rotation and redaction are tested;
- README examples compile; and
- a small, non-destructive authenticated smoke suite passes against the chosen
  Dhan environment.
