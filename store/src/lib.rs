use azure_core::auth::{TokenCredential, TokenResponse};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Store {
    pub auth_token: Option<TokenResponse>,
    pub resource_group: Option<String>,
    pub subscription_id: Option<String>
}
