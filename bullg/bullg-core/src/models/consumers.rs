use serde::{Deserialize, Serialize};
use uuid::Uuid;
//----------------- Consumers Structure ----------------------

fn def_consumer_id() -> String {
    Uuid::new_v4().into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConsumersTemplate {
    pub consumers: Vec<Consumer>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Consumer {
    #[serde(default = "def_consumer_id")]
    pub id: String,
    pub apps: Option<Vec<App>>,
    pub metadata: Option<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct App {
    #[serde(default = "def_consumer_id")]
    pub id: String,
    pub keys: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}