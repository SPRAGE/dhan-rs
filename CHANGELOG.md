# Changelog

## 0.2.0 — 2026-10-11

### Breaking API changes

- Standard market-feed channel getters return `ManagedMarketFeedReceiver<T>`.
  Its `recv`, `try_recv` and `resubscribe` methods report actual Tokio receiver
  overflow to the connection supervisor. Update explicit receiver type
  annotations; ordinary receive loops retain their existing error handling.
- `cancel_order` and `cancel_super_order` return `CancellationOutcome`.
  `Accepted { order_id }` represents an empty HTTP 202/204 response;
  `Reported(OrderResponse)` represents a broker JSON result. Acceptance alone
  does not establish final cancellation; reconcile through broker order state.

### Feed delivery and performance

- Add `MarketFeedDelivery::Batches` for one authoritative consumer per connection.
  Each batch contains one fully validated binary message in packet order.
  Queue limits cover both batch count and decoded storage held by the queue
  or consumer. Queue pressure drops complete incoming batches, reports exact
  lost packet counts, and never blocks socket control or shutdown.
- Add `parse_packets_into` for allocation reuse, avoid repeated header decoding,
  and limit messages to 8 MiB and 15,016 packets. Malformed messages expose
  no partial ticks.
- Remove inferred receiver-lag gaps based on requested broadcast capacity.
  Raw observers share byte storage and do not influence authoritative health.
- Publish healthy feed snapshots once per message, share subscription maps,
  reuse decoded buffers, and prune previous-close references by desired
  ownership and current IST receipt date. Receipt time remains metadata,
  not an exchange timestamp.
- Preserve durable gaps through reconnect and token rotation; recover after
  oversized batches and consumer-held byte reservations are released.

### Dhan API contracts

- Accept documented empty cancellation responses and validate order identity
  in JSON cancellation results.
- Accept object and array trades-by-order envelopes while checking every
  returned trade belongs to the requested order.
- Add typed, validated news-headline requests and responses for
  `POST /v2/data/newsheadline`.

### Release checks

- Require all-feature CI checks before crates.io publication, include
  documentation tests, and publish with the committed lockfile.
- Reject dirty release trees even when a version argument is supplied.
  Dry runs never bump or commit a version.
- Push the release commit and tag atomically, reuse matching local tags on
  retry, and resume registry publication only for an identical package checksum.
  GitHub Release creation runs separately after crate publication.
- Restrict crate contents to source, tests, examples, public documentation and
  license files, and generate installation examples without a tag prefix.

### Verification

Local HTTP/WebSocket fixtures exercise the API contracts and feed lifecycle.
Authenticated Dhan compatibility and production latency remain separate
qualification steps. See `docs/implementation-status.md` for coverage and
known provider documentation conflicts.
