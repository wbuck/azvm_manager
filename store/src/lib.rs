use serde::{Deserialize, Serialize};
use tokio::fs;


const STORE_FILE: &'static str = "store.json";

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Store{
    resource_group: Option<String>,
    subscription_id: Option<String>
}

impl Store {
    pub async fn get_or_create() -> Result<Self, Box<dyn std::error::Error>> {
        match Self::get_store().await {
            Ok(store) => Ok(store),
            Err(_) => {
                let store = Self::default();
                store.save().await?;
                Ok(store)
            }
        }
    }

    pub async fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let contents = serde_json::to_string(self)?;
        fs::write(STORE_FILE, contents).await?;
        Ok(())
    }

    pub fn set_resource_group(&mut self, resource_group: &str) {
        self.resource_group = Some(resource_group.to_owned());
    }

    pub fn get_resource_group(&self) -> Option<&str> {
        self.resource_group.as_deref()
    }

    pub fn set_subscription_id(&mut self, subscription_id: &str) {
        self.subscription_id = Some(subscription_id.to_owned());
    }

    pub fn get_subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }

    async fn get_store() -> Result<Store, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(STORE_FILE).await?;
        Ok(serde_json::from_str::<Store>(&contents)?)
    }
}





