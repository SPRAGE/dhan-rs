---
name: api-contracts
description: Design or evolve durable service contracts covering semantics, compatibility, errors, pagination, concurrency, retries, authorization, and contract proof.
---

# API Contracts

Design for consumer behavior and failure, not only the happy-path payload.

## Discover

Identify consumers, jobs-to-be-done, trust boundaries, data ownership, expected scale, latency, lifecycle, and compatibility commitments. Inspect existing conventions and generated clients before choosing a new shape.

## Specify

Define resource semantics, identifiers, field meaning, units, nullability, ordering, filtering, pagination stability, and time representation. Make error categories actionable and machine-readable without leaking sensitive internals. State authentication, authorization, tenancy, rate limits, and audit requirements.

For mutations define validation, idempotency, retries, concurrency control, partial failure, timeouts, cancellation, and asynchronous completion. For reads define consistency, caching, freshness, and large-result behavior. Prefer additive compatible evolution; version only when semantics cannot remain compatible.

## Prove

Write representative examples and consumer-visible acceptance cases before implementation. Add schema and behavioral contract tests, including unknown fields, old clients, retries, duplicate requests, permission failures, pagination changes, and dependency outages. Verify observability and deprecation signals. Documentation and implementation must share one authoritative contract or be checked for drift.
