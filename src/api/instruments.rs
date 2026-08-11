//! Instrument-master CSV downloads.

use bytes::Bytes;

use crate::client::DhanClient;
use crate::error::{DhanError, Result};
use crate::types::instruments::InstrumentSegment;

const COMPACT_INSTRUMENTS_URL: &str = "https://images.dhan.co/api-data/api-scrip-master.csv";
const DETAILED_INSTRUMENTS_URL: &str =
    "https://images.dhan.co/api-data/api-scrip-master-detailed.csv";

async fn download_public_csv(url: &str) -> Result<Bytes> {
    // These CDN downloads carry no account headers, so following a bounded
    // public redirect does not risk forwarding Dhan credentials.
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;
    let response = http.get(url).send().await?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|source| DhanError::ResponseBody {
            status,
            source: source.without_url(),
        })?;
    if status.is_success() {
        Ok(body)
    } else {
        Err(DhanError::HttpStatus {
            status,
            body: String::from_utf8_lossy(&body).into_owned(),
        })
    }
}

impl DhanClient {
    /// Download Dhan's compact all-segment instrument master as raw CSV bytes.
    pub async fn download_compact_instruments_csv() -> Result<Bytes> {
        download_public_csv(COMPACT_INSTRUMENTS_URL).await
    }

    /// Download Dhan's detailed all-segment instrument master as raw CSV bytes.
    pub async fn download_detailed_instruments_csv() -> Result<Bytes> {
        download_public_csv(DETAILED_INSTRUMENTS_URL).await
    }

    /// Download the detailed instrument CSV for one exchange segment.
    ///
    /// **Endpoint:** `GET /v2/instrument/{exchangeSegment}`
    pub async fn download_segment_instruments_csv(
        &self,
        segment: InstrumentSegment,
    ) -> Result<Bytes> {
        let url = format!("{}/v2/instrument/{}", self.base_url(), segment.as_str());
        let response = self.http().get(url).send().await?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|source| DhanError::ResponseBody { status, source })?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(self.parse_error_body(status, &String::from_utf8_lossy(&body)))
        }
    }
}
