//! Safety-gated tests for the official Dhan sandbox contract.
//!
//! The public contract is served at <https://sandbox.dhan.co/v2/v3/api-docs>
//! and currently declares 27 operations under `https://sandbox.dhan.co/v2`.
//! Every remote test is ignored by default: ordinary `cargo test` and CI do
//! not contact Dhan, use credentials, or mutate an account.
//!
//! See `docs/sandbox-testing.md` for the operation matrix, evidence boundary,
//! outcome meanings, and exact commands.

use std::collections::BTreeSet;
use std::env;
use std::fmt;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dhan_rs::client::DhanClient;
use dhan_rs::error::DhanError;
use dhan_rs::types::enums::*;
use dhan_rs::types::funds::MarginCalculatorRequest;
use dhan_rs::types::historical::{HistoricalDataRequest, IntradayDataRequest};
use dhan_rs::types::orders::{ModifyOrderRequest, OrderDetail, PlaceOrderRequest};
use serde_json::Value;
use tokio::sync::{Mutex, MutexGuard};
use tokio::time::{sleep, timeout};

const SANDBOX_BASE_URL: &str = "https://sandbox.dhan.co";
const SANDBOX_API_SERVER: &str = "https://sandbox.dhan.co/v2";
const SANDBOX_OPENAPI_URL: &str = "https://sandbox.dhan.co/v2/v3/api-docs";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const REQUEST_SPACING: Duration = Duration::from_millis(1_100);

const MUTATION_ENV: &str = "DHAN_SANDBOX_ALLOW_MUTATIONS";
const MUTATION_ACK: &str = "I_ACKNOWLEDGE_SANDBOX_ORDER_MUTATIONS";

/// TCS on NSE, used only for non-executable sandbox calculations and the
/// separately acknowledged order lifecycle.
const TCS_SECURITY_ID: &str = "11536";

static SANDBOX_SERIAL: Mutex<()> = Mutex::const_new(());
static CORRELATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The exact operation inventory retrieved from the official sandbox OpenAPI
/// on 2026-08-11. Its recorded SHA-256 is documented in
/// `docs/sandbox-testing.md`.
const EXPECTED_SANDBOX_OPERATIONS: &[(&str, &str)] = &[
    ("DELETE", "/forever/orders/{order-id}"),
    ("DELETE", "/orders/{order-id}"),
    ("GET", "/edis/inquire/{isin}"),
    ("GET", "/edis/tpin"),
    ("GET", "/forever/orders"),
    ("GET", "/fundlimit"),
    ("GET", "/holdings"),
    ("GET", "/ledger"),
    ("GET", "/orders"),
    ("GET", "/orders/external/{correlation-id}"),
    ("GET", "/orders/{order-id}"),
    ("GET", "/positions"),
    ("GET", "/trades"),
    ("GET", "/trades/{from-date}/{to-date}/{page-number}"),
    ("GET", "/trades/{order-id}"),
    ("POST", "/charts/historical"),
    ("POST", "/charts/intraday"),
    ("POST", "/edis/bulkform"),
    ("POST", "/edis/form"),
    ("POST", "/forever/orders"),
    ("POST", "/killswitch"),
    ("POST", "/margincalculator"),
    ("POST", "/orders"),
    ("POST", "/orders/slicing"),
    ("POST", "/positions/convert"),
    ("PUT", "/forever/orders/{order-id}"),
    ("PUT", "/orders/{order-id}"),
];

#[derive(Debug)]
enum ProbeFailure {
    Timeout(&'static str),
    Api {
        operation: &'static str,
        code: Option<String>,
        kind: Option<String>,
    },
    HttpStatus {
        operation: &'static str,
        status: u16,
    },
    Transport(&'static str),
    Schema(&'static str),
    InvalidConfiguration(&'static str),
    Precondition(&'static str),
    Contract(&'static str),
    SafetyIncident(&'static str),
    CleanupUnresolved {
        diagnostic: Option<String>,
    },
}

impl fmt::Display for ProbeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout(operation) => write!(f, "{operation}: request timed out"),
            Self::Api {
                operation,
                code,
                kind,
            } => write!(
                f,
                "{operation}: sandbox API error code={} type={}",
                code.as_deref().unwrap_or("UNKNOWN"),
                kind.as_deref().unwrap_or("UNKNOWN")
            ),
            Self::HttpStatus { operation, status } => {
                write!(f, "{operation}: sandbox returned HTTP {status}")
            }
            Self::Transport(operation) => {
                write!(f, "{operation}: transport failed (details redacted)")
            }
            Self::Schema(operation) => write!(
                f,
                "{operation}: response did not match the documented schema"
            ),
            Self::InvalidConfiguration(message)
            | Self::Precondition(message)
            | Self::Contract(message)
            | Self::SafetyIncident(message) => f.write_str(message),
            Self::CleanupUnresolved { diagnostic } => {
                f.write_str("sandbox order cleanup could not establish a safe terminal state")?;
                if let Some(diagnostic) = diagnostic {
                    write!(f, "; last redacted diagnostic: {diagnostic}")?;
                }
                Ok(())
            }
        }
    }
}

/// Holds the process-wide network-test lock. Tests remain serialized even when
/// a caller forgets `--test-threads=1`, and every request is paced separately.
struct RemoteSession {
    _guard: MutexGuard<'static, ()>,
    made_request: bool,
}

impl RemoteSession {
    async fn acquire() -> Self {
        let guard = SANDBOX_SERIAL.lock().await;
        // Also space the first request from the preceding test's final request.
        sleep(REQUEST_SPACING).await;
        Self {
            _guard: guard,
            made_request: false,
        }
    }

    async fn call<T, F>(&mut self, operation: &'static str, future: F) -> Result<T, ProbeFailure>
    where
        F: Future<Output = Result<T, DhanError>>,
    {
        if self.made_request {
            sleep(REQUEST_SPACING).await;
        }
        self.made_request = true;

        match timeout(REQUEST_TIMEOUT, future).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(redact_dhan_error(operation, &error)),
            Err(_) => Err(ProbeFailure::Timeout(operation)),
        }
    }
}

fn redact_dhan_error(operation: &'static str, error: &DhanError) -> ProbeFailure {
    match error {
        DhanError::Api(body) => ProbeFailure::Api {
            operation,
            code: body.error_code.clone(),
            kind: body.error_type.clone(),
        },
        DhanError::HttpStatus { status, .. } => ProbeFailure::HttpStatus {
            operation,
            status: status.as_u16(),
        },
        DhanError::Json(_) => ProbeFailure::Schema(operation),
        DhanError::InvalidArgument(_) | DhanError::InvalidHeaderValue(_) | DhanError::Url(_) => {
            ProbeFailure::InvalidConfiguration("sandbox test configuration is invalid")
        }
        DhanError::Http(_) | DhanError::ResponseBody { .. } | DhanError::WebSocket(_) => {
            ProbeFailure::Transport(operation)
        }
    }
}

fn cleanup_unresolved(cause: Option<ProbeFailure>) -> ProbeFailure {
    ProbeFailure::CleanupUnresolved {
        diagnostic: cause.map(|error| error.to_string()),
    }
}

fn required_sandbox_value(name: &'static str) -> String {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => panic!(
            "ignored sandbox test explicitly selected, but {name} is not set; load .env.sandbox with direnv"
        ),
    }
}

fn sandbox_client() -> DhanClient {
    let client_id = required_sandbox_value("DHAN_SANDBOX_CLIENT_ID");
    let token = required_sandbox_value("DHAN_SANDBOX_ACCESS_TOKEN");
    let client = DhanClient::try_with_base_url(client_id, token, SANDBOX_BASE_URL)
        .expect("sandbox credential headers must be valid");
    assert_eq!(client.base_url(), SANDBOX_BASE_URL);
    client
}

fn require_mutation_acknowledgement() {
    let acknowledged = env::var(MUTATION_ENV).unwrap_or_default();
    assert_eq!(
        acknowledged, MUTATION_ACK,
        "sandbox mutation refused: set {MUTATION_ENV} to the exact acknowledgement documented in docs/sandbox-testing.md"
    );
}

async fn expect_probe<T, F>(session: &mut RemoteSession, operation: &'static str, future: F) -> T
where
    F: Future<Output = Result<T, DhanError>>,
{
    session
        .call(operation, future)
        .await
        .unwrap_or_else(|error| panic!("{error}"))
}

fn expected_operation_set() -> BTreeSet<(String, String)> {
    EXPECTED_SANDBOX_OPERATIONS
        .iter()
        .map(|(method, path)| ((*method).to_owned(), (*path).to_owned()))
        .collect()
}

fn openapi_operation_set(document: &Value) -> BTreeSet<(String, String)> {
    const HTTP_METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "options"];

    document
        .get("paths")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|paths| paths.iter())
        .flat_map(|(path, item)| {
            item.as_object().into_iter().flat_map(move |operations| {
                operations
                    .keys()
                    .filter(|method| HTTP_METHODS.contains(&method.as_str()))
                    .map(move |method| (method.to_ascii_uppercase(), path.clone()))
            })
        })
        .collect()
}

async fn existing_order_fixture(
    session: &mut RemoteSession,
    client: &DhanClient,
) -> Result<OrderDetail, ProbeFailure> {
    let orders = session.call("GET /orders", client.get_orders()).await?;
    orders
        .into_iter()
        .find(|order| {
            order
                .order_id
                .as_deref()
                .is_some_and(|order_id| !order_id.trim().is_empty())
        })
        .ok_or(ProbeFailure::Precondition(
            "GET order fixture unavailable: sandbox order book has no order ID",
        ))
}

fn unique_correlation_id() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sequence = CORRELATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("dsbx-{seconds:x}-{sequence:x}")
}

fn normalized_status(status: &str) -> String {
    status.trim().to_ascii_uppercase()
}

fn is_active_status(status: &str) -> bool {
    matches!(normalized_status(status).as_str(), "TRANSIT" | "PENDING")
}

fn is_safe_terminal_status(status: &str) -> bool {
    matches!(
        normalized_status(status).as_str(),
        "REJECTED" | "CANCELLED" | "EXPIRED"
    )
}

fn is_trade_status(status: &str) -> bool {
    matches!(normalized_status(status).as_str(), "PART_TRADED" | "TRADED")
}

async fn cancel_and_verify(
    session: &mut RemoteSession,
    client: &DhanClient,
    order_id: &str,
) -> Result<(), ProbeFailure> {
    let last_diagnostic = match session
        .call("DELETE /orders/{order-id}", client.cancel_order(order_id))
        .await
    {
        Ok(response) if is_safe_terminal_status(&response.order_status) => return Ok(()),
        Ok(response) if is_trade_status(&response.order_status) => {
            return Err(ProbeFailure::SafetyIncident(
                "safety incident: sandbox order traded during cleanup",
            ));
        }
        Ok(_) => None,
        Err(error) => Some(error),
    };

    for _ in 0..5 {
        let orders = match session.call("GET /orders", client.get_orders()).await {
            Ok(orders) => orders,
            Err(error) => return Err(cleanup_unresolved(Some(error))),
        };
        if let Some(order) = orders
            .iter()
            .find(|order| order.order_id.as_deref() == Some(order_id))
        {
            if let Some(status) = order.order_status.as_deref() {
                if is_safe_terminal_status(status) {
                    return Ok(());
                }
                if is_trade_status(status) {
                    return Err(ProbeFailure::SafetyIncident(
                        "safety incident: sandbox order traded during cleanup",
                    ));
                }
            }
        }
    }

    Err(cleanup_unresolved(last_diagnostic))
}

async fn discover_and_cleanup_after_uncertain_placement(
    session: &mut RemoteSession,
    client: &DhanClient,
    correlation_id: &str,
) -> Result<(), ProbeFailure> {
    // A timed-out POST may still have reached the sandbox. Search repeatedly by
    // the unique correlation marker. Bounded non-discovery is not proof that
    // the POST did not succeed, so it remains an unresolved cleanup incident.
    for _ in 0..3 {
        let orders = match session.call("GET /orders", client.get_orders()).await {
            Ok(orders) => orders,
            Err(error) => return Err(cleanup_unresolved(Some(error))),
        };
        if let Some(order_id) = orders.iter().find_map(|order| {
            (order.correlation_id.as_deref() == Some(correlation_id))
                .then(|| order.order_id.clone())
                .flatten()
        }) {
            return cancel_and_verify(session, client, &order_id).await;
        }
    }
    Err(cleanup_unresolved(None))
}

async fn execute_order_lifecycle(
    session: &mut RemoteSession,
    client: &DhanClient,
) -> Result<(), ProbeFailure> {
    let correlation_id = unique_correlation_id();
    let request = PlaceOrderRequest {
        dhan_client_id: client.client_id().to_owned(),
        correlation_id: Some(correlation_id.clone()),
        transaction_type: TransactionType::BUY,
        exchange_segment: ExchangeSegment::NSE_EQ,
        product_type: ProductType::INTRADAY,
        order_type: OrderType::LIMIT,
        validity: Validity::DAY,
        security_id: TCS_SECURITY_ID.into(),
        quantity: 1,
        disclosed_quantity: None,
        price: Some(100.0),
        trigger_price: None,
        after_market_order: Some(false),
        amo_time: None,
        bo_profit_value: None,
        bo_stop_loss_value: None,
    };

    let placement = match session
        .call("POST /orders", client.place_order(&request))
        .await
    {
        Ok(response) => response,
        Err(placement_error) => {
            discover_and_cleanup_after_uncertain_placement(session, client, &correlation_id)
                .await?;
            return Err(placement_error);
        }
    };

    if placement.order_id.trim().is_empty() {
        discover_and_cleanup_after_uncertain_placement(session, client, &correlation_id).await?;
        return Err(ProbeFailure::Contract(
            "POST /orders returned no usable order ID",
        ));
    }

    let order_id = placement.order_id;
    let placement_status = normalized_status(&placement.order_status);

    if is_safe_terminal_status(&placement_status) {
        return Err(ProbeFailure::Contract(
            "POST /orders ended in a terminal non-active state; mutation lifecycle was not exercised",
        ));
    }
    if placement_status == "TRADED" {
        return Err(ProbeFailure::SafetyIncident(
            "safety incident: sandbox placement unexpectedly traded",
        ));
    }
    if placement_status == "PART_TRADED" {
        cancel_and_verify(session, client, &order_id).await?;
        return Err(ProbeFailure::SafetyIncident(
            "safety incident: sandbox placement unexpectedly partially traded",
        ));
    }
    if !is_active_status(&placement_status) {
        let cleanup = cancel_and_verify(session, client, &order_id).await;
        return cleanup.and(Err(ProbeFailure::Contract(
            "POST /orders returned an undocumented active-state acknowledgement",
        )));
    }

    let modify_request = ModifyOrderRequest {
        dhan_client_id: client.client_id().to_owned(),
        order_id: order_id.clone(),
        order_type: OrderType::LIMIT,
        leg_name: None,
        quantity: Some(1),
        price: Some(110.0),
        disclosed_quantity: None,
        trigger_price: None,
        validity: Validity::DAY,
    };

    let modification_error = match session
        .call(
            "PUT /orders/{order-id}",
            client.modify_order(&order_id, &modify_request),
        )
        .await
    {
        Ok(response) if response.order_status.eq_ignore_ascii_case("TRADED") => {
            return Err(ProbeFailure::SafetyIncident(
                "safety incident: sandbox order traded during modification",
            ));
        }
        Ok(response) if response.order_status.eq_ignore_ascii_case("PART_TRADED") => {
            Some(ProbeFailure::SafetyIncident(
                "safety incident: sandbox order partially traded during modification",
            ))
        }
        Ok(response) if is_safe_terminal_status(&response.order_status) => Some(
            ProbeFailure::Contract("PUT /orders/{order-id} returned a terminal non-modified state"),
        ),
        Ok(_) => None,
        Err(error) => Some(error),
    };

    cancel_and_verify(session, client, &order_id).await?;
    if let Some(error) = modification_error {
        return Err(error);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Deterministic local tests (run in ordinary CI)
// ---------------------------------------------------------------------------

#[test]
fn local_sandbox_operation_inventory_is_exact_and_unique() {
    assert_eq!(EXPECTED_SANDBOX_OPERATIONS.len(), 27);
    assert_eq!(expected_operation_set().len(), 27);
    assert!(
        EXPECTED_SANDBOX_OPERATIONS
            .iter()
            .all(|(_, path)| path.starts_with('/'))
    );
}

#[test]
fn local_sandbox_client_pins_and_normalizes_host() {
    let client = DhanClient::try_with_base_url("test", "test", "https://sandbox.dhan.co/")
        .expect("static test credentials are valid header values");
    assert_eq!(client.base_url(), SANDBOX_BASE_URL);
}

#[test]
fn local_sandbox_error_summary_redacts_raw_body() {
    let error = DhanError::HttpStatus {
        status: reqwest::StatusCode::BAD_REQUEST,
        body: "secret-account-response".into(),
    };
    let summary = redact_dhan_error("GET /orders", &error).to_string();
    assert!(!summary.contains("secret-account-response"));
    assert_eq!(summary, "GET /orders: sandbox returned HTTP 400");
}

#[test]
fn local_sandbox_correlation_ids_are_bounded_and_unique() {
    let first = unique_correlation_id();
    let second = unique_correlation_id();
    assert_ne!(first, second);
    assert!(first.len() <= 25);
    assert!(second.len() <= 25);
}

#[test]
fn local_sandbox_status_classification_keeps_trades_out_of_safe_terminals() {
    assert!(is_active_status("pending"));
    assert!(is_safe_terminal_status("REJECTED"));
    assert!(is_trade_status("PART_TRADED"));
    assert!(!is_safe_terminal_status("TRADED"));
}

// ---------------------------------------------------------------------------
// Public contract inventory (remote, credential-free, ignored by default)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "remote public sandbox OpenAPI check; select explicitly"]
async fn sandbox_contract_openapi_operation_inventory() {
    let _guard = SANDBOX_SERIAL.lock().await;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("build public contract client");
    let response = timeout(REQUEST_TIMEOUT, http.get(SANDBOX_OPENAPI_URL).send())
        .await
        .expect("sandbox OpenAPI request timed out")
        .expect("sandbox OpenAPI transport failed");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let document: Value = timeout(REQUEST_TIMEOUT, response.json())
        .await
        .expect("sandbox OpenAPI body timed out")
        .expect("sandbox OpenAPI body was not JSON");

    assert_eq!(
        document.get("openapi").and_then(Value::as_str),
        Some("3.0.1")
    );
    assert_eq!(
        document.pointer("/servers/0/url").and_then(Value::as_str),
        Some(SANDBOX_API_SERVER)
    );
    assert_eq!(openapi_operation_set(&document), expected_operation_set());
}

// ---------------------------------------------------------------------------
// Credentialed read-only and computational operations (ignored by default)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "credentialed sandbox read; select explicitly"]
async fn sandbox_readonly_orders() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let _ = expect_probe(&mut session, "GET /orders", client.get_orders()).await;
}

#[tokio::test]
#[ignore = "credentialed sandbox read; select explicitly"]
async fn sandbox_readonly_order_by_id() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let fixture = existing_order_fixture(&mut session, &client)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let order_id = fixture.order_id.expect("fixture has a non-empty order ID");
    let result = expect_probe(
        &mut session,
        "GET /orders/{order-id}",
        client.get_order(&order_id),
    )
    .await;
    assert!(
        result.order_id.as_deref() == Some(order_id.as_str()),
        "GET order-by-ID returned a different order identity"
    );
}

#[tokio::test]
#[ignore = "credentialed sandbox read requiring an existing correlation ID"]
async fn sandbox_readonly_order_by_correlation_id() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let orders = expect_probe(&mut session, "GET /orders", client.get_orders()).await;
    let correlation_id = orders
        .into_iter()
        .find_map(|order| {
            order
                .correlation_id
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| {
            panic!("GET correlation fixture unavailable: no correlation ID in order book")
        });
    let result = expect_probe(
        &mut session,
        "GET /orders/external/{correlation-id}",
        client.get_order_by_correlation_id(&correlation_id),
    )
    .await;
    assert!(
        result.correlation_id.as_deref() == Some(correlation_id.as_str()),
        "GET order-by-correlation returned a different correlation identity"
    );
}

#[tokio::test]
#[ignore = "credentialed sandbox read; select explicitly"]
async fn sandbox_readonly_trades() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let _ = expect_probe(&mut session, "GET /trades", client.get_trades()).await;
}

#[tokio::test]
#[ignore = "credentialed sandbox read requiring an existing order ID"]
async fn sandbox_readonly_trades_for_order() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let fixture = existing_order_fixture(&mut session, &client)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let order_id = fixture.order_id.expect("fixture has a non-empty order ID");
    let _ = expect_probe(
        &mut session,
        "GET /trades/{order-id}",
        client.get_trades_for_order(&order_id),
    )
    .await;
}

#[tokio::test]
#[ignore = "credentialed sandbox read; select explicitly"]
async fn sandbox_readonly_trade_history() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let _ = expect_probe(
        &mut session,
        "GET /trades/{from-date}/{to-date}/{page-number}",
        client.get_trade_history("2025-01-01", "2025-01-31", 0),
    )
    .await;
}

#[tokio::test]
#[ignore = "credentialed sandbox read; select explicitly"]
async fn sandbox_readonly_holdings() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let _ = expect_probe(&mut session, "GET /holdings", client.get_holdings()).await;
}

#[tokio::test]
#[ignore = "credentialed sandbox read; select explicitly"]
async fn sandbox_readonly_positions() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let _ = expect_probe(&mut session, "GET /positions", client.get_positions()).await;
}

#[tokio::test]
#[ignore = "credentialed sandbox read; select explicitly"]
async fn sandbox_readonly_fund_limit() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let _ = expect_probe(&mut session, "GET /fundlimit", client.get_fund_limit()).await;
}

#[tokio::test]
#[ignore = "credentialed sandbox read; select explicitly"]
async fn sandbox_readonly_ledger() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let _ = expect_probe(
        &mut session,
        "GET /ledger",
        client.get_ledger_response("2025-01-01", "2025-01-31"),
    )
    .await;
}

#[tokio::test]
#[ignore = "credentialed sandbox read; select explicitly"]
async fn sandbox_readonly_forever_orders() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let _ = expect_probe(
        &mut session,
        "GET /forever/orders",
        client.get_all_forever_orders_openapi(),
    )
    .await;
}

#[tokio::test]
#[ignore = "credentialed sandbox read; select explicitly"]
async fn sandbox_readonly_edis_inquiry() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let _ = expect_probe(
        &mut session,
        "GET /edis/inquire/{isin}",
        client.inquire_edis("ALL"),
    )
    .await;
}

#[tokio::test]
#[ignore = "credentialed sandbox computation; select explicitly"]
async fn sandbox_readonly_margin_calculator() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let request = MarginCalculatorRequest {
        dhan_client_id: client.client_id().to_owned(),
        exchange_segment: ExchangeSegment::NSE_EQ,
        transaction_type: TransactionType::BUY,
        quantity: 1,
        product_type: ProductType::INTRADAY,
        security_id: TCS_SECURITY_ID.into(),
        price: 3_500.0,
        trigger_price: None,
    };
    let _ = expect_probe(
        &mut session,
        "POST /margincalculator",
        client.calculate_margin(&request),
    )
    .await;
}

#[tokio::test]
#[ignore = "credentialed sandbox computation; select explicitly"]
async fn sandbox_readonly_daily_historical() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let request = HistoricalDataRequest {
        security_id: TCS_SECURITY_ID.into(),
        exchange_segment: ExchangeSegment::NSE_EQ,
        instrument: Instrument::EQUITY,
        expiry_code: None,
        oi: None,
        from_date: "2025-01-01".into(),
        to_date: "2025-01-31".into(),
    };
    let _ = expect_probe(
        &mut session,
        "POST /charts/historical",
        client.get_daily_historical(&request),
    )
    .await;
}

#[tokio::test]
#[ignore = "credentialed sandbox computation; select explicitly"]
async fn sandbox_readonly_intraday_historical() {
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    let request = IntradayDataRequest {
        security_id: TCS_SECURITY_ID.into(),
        exchange_segment: ExchangeSegment::NSE_EQ,
        instrument: Instrument::EQUITY,
        interval: "5".into(),
        oi: None,
        from_date: "2025-01-15 09:15:00".into(),
        to_date: "2025-01-15 15:30:00".into(),
    };
    let _ = expect_probe(
        &mut session,
        "POST /charts/intraday",
        client.get_intraday_historical(&request),
    )
    .await;
}

#[tokio::test]
#[ignore = "remote auth rejection probe; select explicitly"]
async fn sandbox_readonly_invalid_token_is_rejected() {
    let client = DhanClient::try_with_base_url("invalid", "invalid-token", SANDBOX_BASE_URL)
        .expect("static invalid credentials are valid header values");
    let mut session = RemoteSession::acquire().await;
    let error = session
        .call("GET /orders", client.get_orders())
        .await
        .expect_err("invalid sandbox credentials must not be accepted");
    match error {
        ProbeFailure::Api { code, .. } => assert_eq!(code.as_deref(), Some("DH-901")),
        ProbeFailure::HttpStatus { status, .. } => {
            assert!(matches!(status, 401 | 403), "unexpected auth HTTP status")
        }
        other => panic!("unexpected auth rejection category: {other}"),
    }
}

// ---------------------------------------------------------------------------
// Explicitly acknowledged order mutation (ignored by default)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "places/modifies/cancels one sandbox order; exact acknowledgement required"]
async fn sandbox_mutating_order_lifecycle() {
    require_mutation_acknowledgement();
    let client = sandbox_client();
    let mut session = RemoteSession::acquire().await;
    execute_order_lifecycle(&mut session, &client)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
}
