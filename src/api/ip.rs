//! Static IP management endpoints.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::auth::{IpInfo, IpSetResponse, SetIpRequest};

impl DhanClient {
    /// Set a primary or secondary static IP for the account.
    ///
    /// Once set, the IP cannot be modified for the next 7 days.
    ///
    /// **Endpoint:** `POST /v2/ip/setIP`
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use dhan_rs::client::DhanClient;
    /// # use dhan_rs::types::auth::SetIpRequest;
    /// # use dhan_rs::types::enums::IpFlag;
    /// # #[tokio::main]
    /// # async fn main() -> dhan_rs::error::Result<()> {
    /// let client = DhanClient::new("1000000001", "your-access-token");
    /// let req = SetIpRequest {
    ///     dhan_client_id: "1000000001".into(),
    ///     ip: "10.200.10.10".into(),
    ///     ip_flag: IpFlag::PRIMARY,
    /// };
    /// let resp = client.set_ip(&req).await?;
    /// println!("{}: {}", resp.status, resp.message);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn set_ip(&self, req: &SetIpRequest) -> Result<IpSetResponse> {
        self.post("/v2/ip/setIP", req).await
    }

    /// Modify a previously set primary or secondary static IP.
    ///
    /// Can only be used when IP modification is allowed (once every 7 days).
    ///
    /// **Endpoint:** `PUT /v2/ip/modifyIP`
    pub async fn modify_ip(&self, req: &SetIpRequest) -> Result<IpSetResponse> {
        self.put("/v2/ip/modifyIP", req).await
    }

    /// Get the currently configured static IPs and their modification dates.
    ///
    /// **Endpoint:** `GET /v2/ip/getIP`
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use dhan_rs::client::DhanClient;
    /// # #[tokio::main]
    /// # async fn main() -> dhan_rs::error::Result<()> {
    /// let client = DhanClient::new("1000000001", "your-access-token");
    /// let info = client.get_ip().await?;
    /// if let Some(ip) = &info.primary_ip {
    ///     println!("Primary IP: {ip}");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_ip(&self) -> Result<IpInfo> {
        self.get("/v2/ip/getIP").await
    }
}
