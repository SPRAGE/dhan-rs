# dhan-rs documentation

## Current guides

- [Implementation status and operating boundaries](implementation-status.md)
- [Sandbox testing and safety boundary](sandbox-testing.md)
- [WebSocket stability guide](websocket-stability.md)

## Historical audit and design notes

- [DhanHQ API v2 compliance audit](dhan-v2-compliance-audit.md) — REST and
  WebSocket coverage, official-source conflicts, confirmed defects, security
  findings, and verification limits as of 2026-08-11.
- [WebSocket stability design](websocket-stability-design.md) — the proposed
  connection ownership, reconnect, subscription recovery, token rotation,
  order reconciliation, observability, shutdown, and testing design.

The audit and design describe the pre-remediation baseline. They remain useful
for rationale and official-source conflicts, but the current guides and source
describe what is implemented now. None of these documents certifies the crate
for production trading.
