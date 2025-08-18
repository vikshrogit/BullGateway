
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: String,
    pub name: String,
    pub url: String, // upstream base
    #[serde(default)]
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedPlugin {
    pub name: String,
    pub phase: String, // pre | post | intermediate
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayState {
    pub services: Vec<Service>,
    pub global_plugins: Vec<AppliedPlugin>,
}

impl Default for Service {
    fn default() -> Self {
        Self { id: Uuid::new_v4().to_string(), name: "svc".into(), url: String::new(), routes: vec![] }
    }
}
