use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumersTemplate {
    pub consumers: Vec<Consumer>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consumer {
    pub id: String,
    pub apps: Option<Vec<App>>,
    pub metadata: Option<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct App {
    pub id: String,
    pub keys: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}