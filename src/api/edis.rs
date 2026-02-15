//! EDIS endpoints — T-PIN, Form, Inquiry.

use crate::client::DhanClient;
use crate::error::Result;
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

    /// Inquire the eDIS status for a stock by ISIN.
    ///
    /// Pass `"ALL"` as the ISIN to get status of all holdings.
    ///
    /// **Endpoint:** `GET /v2/edis/inquire/{isin}`
    pub async fn inquire_edis(&self, isin: &str) -> Result<EdisInquiry> {
        self.get(&format!("/v2/edis/inquire/{isin}")).await
    }
}
