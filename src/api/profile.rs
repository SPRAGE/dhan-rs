//! User profile endpoint.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::profile::UserProfile;

impl DhanClient {
    /// Retrieve the user profile.
    ///
    /// Can also be used to validate that an access token is still active.
    ///
    /// **Endpoint:** `GET /v2/profile`
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use dhan_rs::client::DhanClient;
    /// # #[tokio::main]
    /// # async fn main() -> dhan_rs::error::Result<()> {
    /// let client = DhanClient::new("1000000001", "your-access-token");
    /// let profile = client.get_profile().await?;
    /// println!("Token valid until: {}", profile.token_validity);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_profile(&self) -> Result<UserProfile> {
        self.get("/v2/profile").await
    }
}
