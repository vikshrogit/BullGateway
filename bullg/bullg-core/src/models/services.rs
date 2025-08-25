use serde::{Deserialize, Serialize};

// ---------- services.yaml ----------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesTemplate {
    pub gateway: String,
    pub version: String,
    #[serde(rename = "release")]
    pub release_channel: String,
    #[serde(rename = "bullg-versions")]
    pub bullg_versions: Vec<String>,
    pub developer: String,
    pub global: GlobalApplied,
    pub services: Vec<Service>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalApplied {
    pub plugins: Vec<AppliedPlugin>,
    pub policies: Vec<AppliedPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub protocols: Vec<String>,
    pub spec: Option<ServiceSpec>,
    pub versions: Vec<ServiceVersion>,
    pub upstreams: Vec<Upstream>,
    #[serde(rename = "contextPaths")]
    pub context_paths: ServiceContextPaths,
    pub plugins: Vec<AppliedPlugin>,
    pub policies: Vec<AppliedPolicy>,
    pub consumers: Vec<ServiceConsumer>,
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSpec {
    pub enabled: bool,
    pub route: String,
    pub versions: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceVersion {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub description: String,
    pub deprecated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upstream {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub protocols: Vec<String>,
    pub host: String,
    pub port: u16,
    pub enabled: bool,
    pub versions: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceContextPaths {
    pub enable: bool,
    pub paths: Vec<ContextPath>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPath {
    pub path: String,
    pub versions: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedPlugin {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub r#type: String,
    pub tags: Vec<String>,
    pub phase: Option<String>,
    pub enabled: bool,
    pub version: Option<String>,
    pub versions: Option<Vec<String>>,
    pub config: Option<serde_json::Value>,
    pub order: Option<u32>,
    pub priority: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedPolicy {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub r#type: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub version: Option<Vec<String>>,
    pub config: Option<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConsumer {
    pub id: String,
    pub enabled: bool,
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub versions: Vec<String>,
    pub config: RouteConfig,
    pub plugins: Vec<AppliedPlugin>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteConfig {
    pub protocols: Vec<String>,
    pub path: String,
    pub backend: String,
    pub methods: Vec<String>,
}