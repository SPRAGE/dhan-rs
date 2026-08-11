//! Authentication endpoint implementations.
//!
//! These methods hit the **auth.dhan.co** domain (not the regular API base URL)
//! except for `renew_token` which uses the standard `/v2/RenewToken` endpoint.

use reqwest::header::HeaderValue;
use serde_json::Value;

use crate::client::{DhanClient, required_query_value};
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
        let client_id = required_query_value("client_id", client_id)?;
        let pin = required_query_value("pin", pin)?;
        let totp = required_query_value("totp", totp)?;
        let url = format!(
            "{}/app/generateAccessToken?dhanClientId={}&pin={}&totp={}",
            AUTH_BASE_URL, client_id, pin, totp
        );

        tracing::debug!("POST generate_access_token");

        let http = auth_http_client()?;
        let resp = http
            .post(&url)
            .send()
            .await
            .map_err(sanitize_auth_http_error)?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|source| DhanError::ResponseBody {
                status,
                source: source.without_url(),
            })?;

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
    ///
    /// # Note
    ///
    /// The RenewToken endpoint uses `dhanClientId` as its client
    /// identification header, unlike most other endpoints that use `client-id`.
    /// This method handles the difference automatically.
    pub async fn renew_token(&mut self) -> Result<TokenResponse> {
        let url = format!("{}/v2/RenewToken", self.base_url());

        tracing::debug!("GET renew_token");

        let mut access_token = HeaderValue::from_str(self.access_token())?;
        access_token.set_sensitive(true);
        let mut client_id = HeaderValue::from_str(self.client_id())?;
        client_id.set_sensitive(true);

        let resp = self
            .http()
            .get(&url)
            .header("access-token", access_token)
            .header("dhanClientId", client_id)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(sanitize_auth_http_error)?;

        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|source| DhanError::ResponseBody {
                status,
                source: source.without_url(),
            })?;

        if status.is_success() {
            let token: TokenResponse = serde_json::from_slice(&bytes).map_err(DhanError::Json)?;
            // Update the client's token so subsequent calls use the new one.
            self.try_set_access_token(&token.access_token)?;
            Ok(token)
        } else {
            let body = String::from_utf8_lossy(&bytes);
            Err(self.parse_error_body(status, &body))
        }
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
        let client_id = required_query_value("client_id", client_id)?;
        let url = format!(
            "{}/app/generate-consent?client_id={}",
            AUTH_BASE_URL, client_id
        );

        tracing::debug!("POST generate_consent");

        let http = auth_http_client()?;
        let resp = http
            .post(&url)
            .header("app_id", sensitive_header_value(app_id)?)
            .header("app_secret", sensitive_header_value(app_secret)?)
            .send()
            .await
            .map_err(sanitize_auth_http_error)?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|source| DhanError::ResponseBody {
                status,
                source: source.without_url(),
            })?;

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
    /// use dhan_rs::DhanClient;
    /// let url = DhanClient::consent_login_url("940b0ca1-3ff4-4476-b46e-03a3ce7dc55d");
    /// // → "https://auth.dhan.co/login/consentApp-login?consentAppId=940b0ca1-..."
    /// ```
    pub fn consent_login_url(consent_app_id: &str) -> String {
        let consent_app_id =
            url::form_urlencoded::byte_serialize(consent_app_id.as_bytes()).collect::<String>();
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
        let token_id = required_query_value("token_id", token_id)?;
        let url = format!(
            "{}/app/consumeApp-consent?tokenId={}",
            AUTH_BASE_URL, token_id
        );

        tracing::debug!("POST consume_consent");

        let http = auth_http_client()?;
        let resp = http
            .post(&url)
            .header("app_id", sensitive_header_value(app_id)?)
            .header("app_secret", sensitive_header_value(app_secret)?)
            .send()
            .await
            .map_err(sanitize_auth_http_error)?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|source| DhanError::ResponseBody {
                status,
                source: source.without_url(),
            })?;

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

        tracing::debug!("POST partner_generate_consent");

        let http = auth_http_client()?;
        let resp = http
            .post(&url)
            .header("partner_id", sensitive_header_value(partner_id)?)
            .header("partner_secret", sensitive_header_value(partner_secret)?)
            .send()
            .await
            .map_err(sanitize_auth_http_error)?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|source| DhanError::ResponseBody {
                status,
                source: source.without_url(),
            })?;

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
        let consent_id =
            url::form_urlencoded::byte_serialize(consent_id.as_bytes()).collect::<String>();
        format!("{}/consent-login?consentId={consent_id}", AUTH_BASE_URL)
    }

    /// **Step 3 (Partner):** Consume the partner consent to obtain an access token.
    ///
    /// **Endpoint:** `POST https://auth.dhan.co/partner/consume-consent?tokenId={tokenId}`
    pub async fn partner_consume_consent(
        token_id: &str,
        partner_id: &str,
        partner_secret: &str,
    ) -> Result<TokenResponse> {
        let token_id = required_query_value("token_id", token_id)?;
        let url = format!(
            "{}/partner/consume-consent?tokenId={}",
            AUTH_BASE_URL, token_id
        );

        tracing::debug!("POST partner_consume_consent");

        let http = auth_http_client()?;
        let resp = http
            .post(&url)
            .header("partner_id", sensitive_header_value(partner_id)?)
            .header("partner_secret", sensitive_header_value(partner_secret)?)
            .send()
            .await
            .map_err(sanitize_auth_http_error)?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|source| DhanError::ResponseBody {
                status,
                source: source.without_url(),
            })?;

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

fn sensitive_header_value(value: &str) -> Result<HeaderValue> {
    let mut value = HeaderValue::from_str(value)?;
    value.set_sensitive(true);
    Ok(value)
}

fn auth_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(DhanError::Http)
}

fn sanitize_auth_http_error(error: reqwest::Error) -> DhanError {
    DhanError::Http(error.without_url())
}

#[cfg(test)]
mod tests {
    use super::sanitize_auth_http_error;

    #[tokio::test]
    async fn auth_transport_errors_do_not_retain_sensitive_urls() {
        let pin = "1234";
        let totp = "654321";
        let token_id = "sensitive-consent-token";
        let url = format!(
            "ftp://127.0.0.1/app/generateAccessToken?dhanClientId=1&pin={pin}&totp={totp}&tokenId={token_id}"
        );
        let error = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .expect_err("reqwest must reject the unsupported URL scheme");
        let error = sanitize_auth_http_error(error);
        let display = error.to_string();
        let debug = format!("{error:?}");
        for secret in [pin, totp, token_id] {
            assert!(!display.contains(secret));
            assert!(!debug.contains(secret));
        }
    }
}
