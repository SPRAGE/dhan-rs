#![allow(missing_docs)]
//! EDIS types — T-PIN, form generation, inquiry.

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Exchange accepted by the bulk eDIS form endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[allow(non_camel_case_types)]
pub enum EdisExchange {
    NSE,
    BSE,
    MCX,
    ALL,
}

/// Segment accepted by the bulk eDIS form endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[allow(non_camel_case_types)]
pub enum EdisSegment {
    EQ,
    COMM,
    FNO,
}

/// Request body for generating one eDIS form for multiple ISINs.
///
/// Used by `POST /v2/edis/bulkform`.
#[derive(Debug, Clone, Serialize)]
pub struct EdisBulkFormRequest {
    pub isin: Vec<String>,
    pub exchange: EdisExchange,
    pub segment: EdisSegment,
}

impl EdisBulkFormRequest {
    /// Validate that the request contains at least one non-empty ISIN.
    pub fn validate(&self) -> std::result::Result<(), &'static str> {
        if self.isin.is_empty() {
            return Err("bulk eDIS isin cannot be empty");
        }
        if self.isin.iter().any(|isin| isin.trim().is_empty()) {
            return Err("bulk eDIS isin entries cannot be empty");
        }
        Ok(())
    }
}

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
    #[serde(
        default,
        deserialize_with = "deserialize_optional_i64_from_integer_or_string"
    )]
    pub total_qty: Option<i64>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_i64_from_integer_or_string"
    )]
    pub aprvd_qty: Option<i64>,
    pub status: Option<String>,
    pub remarks: Option<String>,
}

/// Deserialize documented eDIS quantities which are inconsistently typed as
/// JSON integers and JSON strings across Dhan's official contract sources.
fn deserialize_optional_i64_from_integer_or_string<'de, D>(
    deserializer: D,
) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    struct QuantityVisitor;

    impl<'de> Visitor<'de> for QuantityVisitor {
        type Value = Option<i64>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an integer, an integer string, or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(self)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            i64::try_from(value).map(Some).map_err(E::custom)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value.parse::<i64>().map(Some).map_err(E::custom)
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_option(QuantityVisitor)
}
