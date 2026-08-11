//! EDIS endpoints — T-PIN, Form, Inquiry.

use crate::client::{DhanClient, required_path_segment};
use crate::error::{DhanError, Result};
use crate::types::edis::*;

impl DhanClient {
    /// Generate a T-PIN on the user's registered mobile number.
    ///
    /// Returns `202 Accepted` on success.
    ///
    /// **Endpoint:** `GET /v2/edis/tpin`
    pub async fn generate_tpin(&self) -> Result<()> {
        self.get_no_content("/v2/edis/tpin").await
    }

    /// Generate an eDIS form for CDSL T-PIN entry.
    ///
    /// **Endpoint:** `POST /v2/edis/form`
    pub async fn generate_edis_form(&self, req: &EdisFormRequest) -> Result<EdisFormResponse> {
        self.post("/v2/edis/form", req).await
    }

    /// Generate one eDIS form for multiple ISINs.
    ///
    /// **Endpoint:** `POST /v2/edis/bulkform`
    pub async fn generate_bulk_edis_form(
        &self,
        req: &EdisBulkFormRequest,
    ) -> Result<EdisFormResponse> {
        req.validate()
            .map_err(|message| DhanError::InvalidArgument(message.into()))?;
        self.post("/v2/edis/bulkform", req).await
    }

    /// Inquire the eDIS status for a stock by ISIN.
    ///
    /// Pass `"ALL"` as the ISIN to get status of all holdings.
    ///
    /// **Endpoint:** `GET /v2/edis/inquire/{isin}`
    pub async fn inquire_edis(&self, isin: &str) -> Result<EdisInquiry> {
        let isin = required_path_segment("isin", isin)?;
        self.get(&format!("/v2/edis/inquire/{isin}")).await
    }
}
