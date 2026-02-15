//! EDIS types — T-PIN, form generation, inquiry.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// eDIS Form Request
// ---------------------------------------------------------------------------

/// Request body for generating an eDIS form.
///
/// Used by `POST /v2/edis/form`.
#[derive(Debug, Clone, Serialize)]
pub struct EdisFormRequest {
    /// ISIN of the stock.
    pub isin: String,
    /// Number of shares to mark for eDIS transaction.
    pub qty: u64,
    /// Exchange (`NSE` or `BSE`).
    pub exchange: String,
    /// Segment (`EQ`).
    pub segment: String,
    /// Mark eDIS for all stocks in portfolio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bulk: Option<bool>,
}

// ---------------------------------------------------------------------------
// eDIS Form Response
// ---------------------------------------------------------------------------

/// Response from generating an eDIS form.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdisFormResponse {
    pub dhan_client_id: String,
    /// Escaped HTML form for CDSL T-PIN entry.
    pub edis_form_html: String,
}

// ---------------------------------------------------------------------------
// eDIS Inquiry
// ---------------------------------------------------------------------------

/// eDIS inquiry result for a stock.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdisInquiry {
    pub client_id: Option<String>,
    pub isin: Option<String>,
    #[serde(default)]
    pub total_qty: Option<i64>,
    #[serde(default)]
    pub aprvd_qty: Option<i64>,
    pub status: Option<String>,
    pub remarks: Option<String>,
}
