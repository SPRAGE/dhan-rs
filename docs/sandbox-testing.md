# Dhan sandbox testing

This guide defines the sandbox test boundary for this crate. It is based on
the official [Dhan sandbox documentation](https://sandbox.dhan.co/v2/) and
its machine-readable [OpenAPI description](https://sandbox.dhan.co/v2/v3/api-docs),
retrieved on 2026-08-11.

The retrieved description is OpenAPI 3.0.1, declares the server
`https://sandbox.dhan.co/v2`, and contains exactly 27 operations. Its
SHA-256 is
`7610f4be9db848aedf180cd7cfff4108f2665ffeecfdf916462d7763744255ed`.
The digest records the retrieved artifact for provenance. The automated remote
test is deliberately an operation-inventory check: it verifies the OpenAPI
version, server URL, and 27 method/path pairs, but does not currently hash the
whole document or detect schema-only changes that preserve that inventory.

This is sandbox-contract evidence only. It does not establish production
parity, and an endpoint that is present in the production documentation but
absent below is not sandbox conformance coverage.

## Running the tests

Sandbox credentials are loaded locally through `direnv` from the ignored
`.env.sandbox` file. The repository's `.envrc` refuses credential files that
are not mode `0600` and discards any mutation acknowledgement loaded from the
file, keeping that acknowledgement one-command-only. Remote tests are
intentionally ignored by default, so normal CI remains deterministic and
never contacts Dhan.

Create the file from `.env.sandbox.example` and fill only
`DHAN_SANDBOX_CLIENT_ID` and `DHAN_SANDBOX_ACCESS_TOKEN`. The sandbox-specific
names prevent unrelated production-oriented shell credentials from being
consumed. For private files created from the earlier generic-name template,
`.envrc` maps the two values once and removes the ambiguous names from the
direnv environment.

```sh
# Deterministic local checks only; remote tests remain ignored.
cargo test --test sandbox

# Fetch and compare the public sandbox OpenAPI operation inventory.
cargo test --test sandbox sandbox_contract_ -- --ignored --test-threads=1

# Run credentialed, read-only/computational sandbox probes.
cargo test --test sandbox sandbox_readonly_ -- --ignored --test-threads=1

# A single explicitly acknowledged order lifecycle. This may place, modify,
# and cancel one sandbox order; use only when that activity is intended.
DHAN_SANDBOX_ALLOW_MUTATIONS=I_ACKNOWLEDGE_SANDBOX_ORDER_MUTATIONS \
  cargo test --test sandbox sandbox_mutating_order_lifecycle -- --ignored --test-threads=1
```

The remote suite serializes its calls, spaces them to avoid avoidable rate
pressure, bounds each request with a timeout, and does not print credentials,
account data, order IDs, or raw response bodies. A failed ignored test is
evidence to investigate; it is never silently converted to a pass because an
endpoint is flaky or incomplete.

## Outcome categories

| Outcome | Meaning |
|---|---|
| Pass | The endpoint responded and the client accepted the documented contract for this probe. |
| Contract drift | The sandbox response shape or semantics contradict the retrieved OpenAPI description. This must be recorded without changing production types solely on sandbox evidence. |
| Sandbox limitation | The endpoint is documented but presently unavailable or constrained in sandbox (for example a `DH-905`, 404, or 5xx response). This is an observation, not a passing test. |
| Environment/precondition | Credentials, required account state, or a non-sensitive fixture needed by a read endpoint are absent. The test should say which precondition is missing. |
| Safety incident | A mutating test gets an unexpected executable, partially traded, or traded result, or cleanup cannot establish a terminal state. Stop and investigate before any further mutation. |

## OpenAPI operation matrix

“Automated” means the operation is eligible for a test under the stated gate;
it does not mean it is continuously exercised. Read-only probes still require
explicit `--ignored` selection and sandbox credentials. “Manual only” means
the suite must never invoke it automatically.

| Method | Sandbox path | Safety class | Automated status |
|---|---|---|---|
| `GET` | `/fundlimit` | Read-only account data | Read-only probe |
| `GET` | `/holdings` | Read-only account data | Read-only probe |
| `GET` | `/ledger` | Read-only account data | Read-only probe |
| `GET` | `/orders` | Read-only account data | Read-only probe |
| `GET` | `/orders/{order-id}` | Read-only, needs existing order fixture | Read-only probe when precondition is supplied |
| `GET` | `/orders/external/{correlation-id}` | Read-only, needs existing correlation fixture | Read-only probe when precondition is supplied |
| `GET` | `/positions` | Read-only account data | Read-only probe |
| `GET` | `/trades` | Read-only account data | Read-only probe |
| `GET` | `/trades/{order-id}` | Read-only, needs existing order fixture | Read-only probe when precondition is supplied |
| `GET` | `/trades/{from-date}/{to-date}/{page-number}` | Read-only account data | Read-only probe |
| `GET` | `/forever/orders` | Read-only account data | Read-only probe |
| `GET` | `/edis/inquire/{isin}` | Read-only eDIS inquiry | Read-only probe |
| `POST` | `/charts/historical` | Computational market-data request | Read-only probe |
| `POST` | `/charts/intraday` | Computational market-data request | Read-only probe |
| `POST` | `/margincalculator` | Computational account/margin request | Read-only probe |
| `POST` | `/orders` | Order mutation | Dedicated, exact acknowledgement and cleanup required |
| `PUT` | `/orders/{order-id}` | Order mutation | Only within the dedicated lifecycle after a safe placement |
| `DELETE` | `/orders/{order-id}` | Order mutation/cleanup | Only within the dedicated lifecycle or required cleanup |
| `POST` | `/orders/slicing` | Multi-order mutation | Manual only |
| `POST` | `/positions/convert` | Position mutation | Manual only |
| `POST` | `/forever/orders` | Forever-order mutation | Manual only |
| `PUT` | `/forever/orders/{order-id}` | Forever-order mutation | Manual only |
| `DELETE` | `/forever/orders/{order-id}` | Forever-order mutation | Manual only |
| `GET` | `/edis/tpin` | Sends/initiates T-PIN activity | Manual only |
| `POST` | `/edis/form` | Generates an eDIS form | Manual only |
| `POST` | `/edis/bulkform` | Generates eDIS forms | Manual only |
| `POST` | `/killswitch` | Account-control mutation | Manual only |

## Mutation rules

The `sandbox_mutating_order_lifecycle` test is intentionally narrow. It uses a
unique correlation ID, performs no blind retries, serializes network access,
and checks the placement acknowledgement before modifying or cancelling. It
must attempt cleanup before reporting an assertion failure, then verifies a
terminal order state. A rejected placement is not a successful lifecycle; an
unexpected partially traded or traded result is a safety incident.

The following are never part of an automated sandbox run, even when
credentials are available:

- Kill switch calls.
- T-PIN generation and eDIS form or bulk-form generation.
- Position conversion.
- Order slicing.
- Forever-order create, modify, or delete operations without a separate,
  dedicated user approval and test plan.

Do not put the mutation acknowledgement in `.env.sandbox` by default, commit
credentials, or point these commands at a production host.

## Known sandbox boundary observations

The sandbox OpenAPI description is the inventory source of truth for this
guide, but live sandbox behavior can drift. Treat such drift as a separately
reported outcome:

### 2026-08-11 read-only validation

The public OpenAPI inventory check passed: the retrieved description matched
all 27 of the expected method/path pairs. A credentialed read-only validation
then exercised the 15 documented safe, read-only, or computational operations.
Ten passed:

- `GET /orders`
- `GET /orders/external/{correlation-id}`
- `GET /trades`
- `GET /trades/{order-id}`
- `GET /holdings`
- `GET /positions`
- `GET /fundlimit`
- `GET /ledger`
- `GET /forever/orders`
- `POST /charts/intraday`

Five did not pass and were not skipped:

| Operation | Observed outcome |
|---|---|
| `POST /charts/historical` | `DH-905 Input_Exception` |
| `GET /edis/inquire/{isin}` | `DH-0035 EDIS_ERROR` |
| `POST /margincalculator` | Timed out at the client 20-second boundary |
| `GET /orders/{order-id}` | Typed response schema mismatch |
| `GET /trades/{from-date}/{to-date}/{page-number}` | `DH-905 Input_Exception` |

An auxiliary invalid-token rejection check also passed. It is useful
authentication evidence, but is not another sandbox OpenAPI operation in the
15-operation read-only validation. No mutation ran during this hardening
validation.

These are point-in-time results, not stable capability guarantees. A later
isolated `GET /orders` smoke probe in the same hardening session returned
`DH-906 Order_Error`; it was recorded without retrying it into a pass. This
later observation does not rewrite the earlier full-run count, but it does show
that sandbox availability can change within one session.

### Earlier acknowledged mutation characterization

Before the hardened harness replaced the original lifecycle test, one
explicitly authorized sandbox placement request decoded successfully but
ended in `REJECTED`. The subsequent order-by-ID response had the array/object
schema drift described below, so modify and cancel were not exercised. A
correlation-based cleanup inventory found no active matching order; no cleanup
cancellation was required. This is placement-response and terminal-rejection
evidence only, not a passing place/modify/cancel lifecycle.

- The documented `GET /orders/{order-id}` schema describes one order object,
  while an observed sandbox response was an array (including an empty array
  for a nonexistent ID). Do not change the production response model based on
  this sandbox-only mismatch.
- Sandbox testing previously observed endpoint-specific availability failures
  such as `DH-905`, 404, and 504 responses. They remain failures or explicit
  limitations until revalidated; they are not globally skipped.
- Sandbox OpenAPI does not list profile, market-quote, Super Order,
  WebSocket, or kill-switch `GET` endpoints. If probed, they are auxiliary
  compatibility checks, not part of the 27-operation sandbox contract.

## Scope and evidence

Keep the public OpenAPI inventory check separate from authenticated endpoint
checks: the first detects documentation changes, while the second observes a
specific sandbox account and time. Neither substitutes for production
validation or authorizes live trading.
