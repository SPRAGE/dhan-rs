//! Authentication endpoint implementations.
//!
//! These methods hit the **auth.dhan.co** domain (not the regular API base URL)
//! except for `renew_token` which uses the standard `/v2/RenewToken` endpoint.

use reqwest::header::HeaderValue;
use serde_json::Value;

use crate::client::DhanClient;
use crate::constants::AUTH_BASE_URL;
use crate::error::{ApiErrorBody, DhanError, Result};
use crate::types::auth::{AppConsentResponse, PartnerConsentResponse, TokenResponse};

impl DhanClient {
    // -----------------------------------------------------------------------
    // Direct token generation (TOTP)
    // -----------------------------------------------------------------------

    /// Generate an access token using client credentials and TOTP.
    ///
    /// Requires TOTP to be enabled on the Dhan account.
    ///
    /// **Endpoint:** `POST https://auth.dhan.co/app/generateAccessToken`
    ///
    /// # Arguments
    ///
    /// * `client_id` — The Dhan Client ID.
    /// * `pin` — 6-digit Dhan PIN.
    /// * `totp` — 6-digit TOTP code from an authenticator app.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use dhan_rs::client::DhanClient;
    /// # #[tokio::main]
    /// # async fn main() -> dhan_rs::error::Result<()> {
    /// let client = DhanClient::new("1000000001", "");
    /// let token = DhanClient::generate_access_token("1000000001", "123456", "654321").await?;
    /// println!("Access token: {}", token.access_token);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn generate_access_token(
        client_id: &str,
        pin: &str,
        totp: &str,
    ) -> Result<TokenResponse> {
        let url = format!(
            "{}/app/generateAccessToken?dhanClientId={}&pin={}&totp={}",
            AUTH_BASE_URL, client_id, pin, totp
        );

        tracing::debug!(%url, "POST generate_access_token");

        let http = reqwest::Client::new();
        let resp = http.post(&url).send().await?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if status.is_success() {
            serde_json::from_str(&body).map_err(DhanError::Json)
        } else {
            if let Ok(api_err) = serde_json::from_str::<ApiErrorBody>(&body) {
                if api_err.error_code.is_some() || api_err.error_message.is_some() {
                    return Err(DhanError::Api(api_err));
                }
            }
            Err(DhanError::HttpStatus { status, body })
        }
    }

    // -----------------------------------------------------------------------
    // Token renewal
    // -----------------------------------------------------------------------

    /// Renew the current access token for another 24 hours.
    ///
    /// Only works for tokens generated from Dhan Web that are still active.
    /// This expires the current token and returns a new one.
    ///
    /// **Endpoint:** `GET /v2/RenewToken`
    pub async fn renew_token(&mut self) -> Result<TokenResponse> {
        let token: TokenResponse = self.get("/v2/RenewToken").await?;
        // Update the client's token so subsequent calls use the new one.
        self.set_access_token(&token.access_token);
        Ok(token)
    }

    // -----------------------------------------------------------------------
    // Individual — API key & secret OAuth flow
    // -----------------------------------------------------------------------

    /// **Step 1:** Generate a consent session for API key-based login.
    ///
    /// Validates the `app_id` and `app_secret` and creates a new session.
    ///
    /// **Endpoint:** `POST https://auth.dhan.co/app/generate-consent?client_id={dhanClientId}`
    ///
    /// Returns an [`AppConsentResponse`] containing a `consent_app_id` to be
    /// used in the browser redirect step.
    pub async fn generate_consent(
        client_id: &str,
        app_id: &str,
        app_secret: &str,
    ) -> Result<AppConsentResponse> {
        let url = format!(
            "{}/app/generate-consent?client_id={}",
            AUTH_BASE_URL, client_id
        );

        tracing::debug!(%url, "POST generate_consent");

        let http = reqwest::Client::new();
        let resp = http
            .post(&url)
            .header(
                "app_id",
                HeaderValue::from_str(app_id).map_err(|_| {
                    DhanError::InvalidArgument("app_id contains invalid characters".into())
                })?,
            )
            .header(
                "app_secret",
                HeaderValue::from_str(app_secret).map_err(|_| {
                    DhanError::InvalidArgument("app_secret contains invalid characters".into())
                })?,
            )
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if status.is_success() {
            serde_json::from_str(&body).map_err(DhanError::Json)
        } else {
            Self::parse_auth_error(status, &body)
        }
    }

    /// **Step 2:** Build the browser login URL for user consent.
    ///
    /// Open this URL in a browser. After the user authenticates, they will be
    /// redirected to the redirect URL configured for the API key, with a
    /// `tokenId` query parameter appended.
    ///
    /// ```
    /// let url = DhanClient::consent_login_url("940b0ca1-3ff4-4476-b46e-03a3ce7dc55d");
    /// // → "https://auth.dhan.co/login/consentApp-login?consentAppId=940b0ca1-..."
    /// ```
    pub fn consent_login_url(consent_app_id: &str) -> String {
        format!(
            "{}/login/consentApp-login?consentAppId={}",
            AUTH_BASE_URL, consent_app_id
        )
    }

    /// **Step 3:** Consume the consent to obtain an access token.
    ///
    /// Uses the `token_id` obtained from the browser redirect after the user
    /// logged in.
    ///
    /// **Endpoint:** `POST https://auth.dhan.co/app/consumeApp-consent?tokenId={tokenId}`
    pub async fn consume_consent(
        token_id: &str,
        app_id: &str,
        app_secret: &str,
    ) -> Result<TokenResponse> {
        let url = format!(
            "{}/app/consumeApp-consent?tokenId={}",
            AUTH_BASE_URL, token_id
        );

        tracing::debug!(%url, "POST consume_consent");

        let http = reqwest::Client::new();
        let resp = http
            .post(&url)
            .header(
                "app_id",
                HeaderValue::from_str(app_id).map_err(|_| {
                    DhanError::InvalidArgument("app_id contains invalid characters".into())
                })?,
            )
            .header(
                "app_secret",
                HeaderValue::from_str(app_secret).map_err(|_| {
                    DhanError::InvalidArgument("app_secret contains invalid characters".into())
                })?,
            )
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if status.is_success() {
            serde_json::from_str(&body).map_err(DhanError::Json)
        } else {
            Self::parse_auth_error(status, &body)
        }
    }

    // -----------------------------------------------------------------------
    // Partner — OAuth flow
    // -----------------------------------------------------------------------

    /// **Step 1 (Partner):** Generate a partner consent session.
    ///
    /// **Endpoint:** `POST https://auth.dhan.co/partner/generate-consent`
    pub async fn partner_generate_consent(
        partner_id: &str,
        partner_secret: &str,
    ) -> Result<PartnerConsentResponse> {
        let url = format!("{}/partner/generate-consent", AUTH_BASE_URL);

        tracing::debug!(%url, "POST partner_generate_consent");

        let http = reqwest::Client::new();
        let resp = http
            .post(&url)
            .header(
                "partner_id",
                HeaderValue::from_str(partner_id).map_err(|_| {
                    DhanError::InvalidArgument("partner_id contains invalid characters".into())
                })?,
            )
            .header(
                "partner_secret",
                HeaderValue::from_str(partner_secret).map_err(|_| {
                    DhanError::InvalidArgument("partner_secret contains invalid characters".into())
                })?,
            )
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if status.is_success() {
            serde_json::from_str(&body).map_err(DhanError::Json)
        } else {
            Self::parse_auth_error(status, &body)
        }
    }

    /// **Step 2 (Partner):** Build the browser login URL for partner consent.
    ///
    /// Open this URL in a browser. After the user authenticates, they will be
    /// redirected with a `tokenId` query parameter.
    pub fn partner_consent_login_url(consent_id: &str) -> String {
        format!("{}/consent-login?consentId={}", AUTH_BASE_URL, consent_id)
    }

    /// **Step 3 (Partner):** Consume the partner consent to obtain an access token.
    ///
    /// **Endpoint:** `POST https://auth.dhan.co/partner/consume-consent?tokenId={tokenId}`
    pub async fn partner_consume_consent(
        token_id: &str,
        partner_id: &str,
        partner_secret: &str,
    ) -> Result<TokenResponse> {
        let url = format!(
            "{}/partner/consume-consent?tokenId={}",
            AUTH_BASE_URL, token_id
        );

        tracing::debug!(%url, "POST partner_consume_consent");

        let http = reqwest::Client::new();
        let resp = http
            .post(&url)
            .header(
                "partner_id",
                HeaderValue::from_str(partner_id).map_err(|_| {
                    DhanError::InvalidArgument("partner_id contains invalid characters".into())
                })?,
            )
            .header(
                "partner_secret",
                HeaderValue::from_str(partner_secret).map_err(|_| {
                    DhanError::InvalidArgument("partner_secret contains invalid characters".into())
                })?,
            )
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if status.is_success() {
            serde_json::from_str(&body).map_err(DhanError::Json)
        } else {
            Self::parse_auth_error(status, &body)
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers for auth endpoints
    // -----------------------------------------------------------------------

    /// Parse an error response from an auth endpoint.
    fn parse_auth_error<T>(status: reqwest::StatusCode, body: &str) -> Result<T> {
        if let Ok(api_err) = serde_json::from_str::<ApiErrorBody>(body) {
            if api_err.error_code.is_some() || api_err.error_message.is_some() {
                return Err(DhanError::Api(api_err));
            }
        }
        // Some auth endpoints may return a simple JSON with a "status" key.
        if let Ok(val) = serde_json::from_str::<Value>(body) {
            if let Some(status_str) = val.get("status").and_then(|v| v.as_str()) {
                return Err(DhanError::HttpStatus {
                    status,
                    body: format!("auth error: {status_str}"),
                });
            }
        }
        Err(DhanError::HttpStatus {
            status,
            body: body.to_owned(),
        })
    }
}
