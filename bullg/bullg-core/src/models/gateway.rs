use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// ---------- Gateway System Configurations Models ----------
fn def_host() -> String {
    "0.0.0.0".into()
}
fn def_port() -> u16 {
    8000
}
fn def_ssl_port() -> u16 {
    8443
}
fn def_logging() -> String {
    "debug".into()
}

fn def_bool() -> bool {
    false
}

fn def_health_path() -> String {
    "/healthz".into()
}

fn def_metrics_path() -> String {
    "/metrics".into()
}


pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

pub fn app_name() -> String {
    env!("APP_NAME").into()
}


pub fn gateway_name() -> String {
    app_name() + " Gateway"
}

pub fn random_id() -> String {
    Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BullGType {
    Dataplane,
    #[default]
    Tenantplane,
    Controlplane,
}

fn gateway_type() -> BullGType {
    BullGType::Tenantplane
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayConfig {
    pub gateway: GatewayNode,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayNode {
    #[serde(default = "gateway_name")]
    pub name: String,
    #[serde(default = "random_id")]
    pub id: String,
    #[serde(default = "gateway_type")]
    pub r#type: BullGType, // dataplane|tenantplane
    #[serde(default = "app_version")]
    pub version: String,
    pub tags: Vec<String>,
    pub labels: serde_json::Map<String, Value>,
    pub description: String,
    pub proxy: Proxy,
    #[serde(default = "def_host")]
    pub host: String,
    #[serde(default = "def_port")]
    pub port: u16,
    #[serde(default = "def_bool")]
    pub ssl: bool,
    #[serde(default = "def_bool")]
    pub http3: bool,
    #[serde(default = "def_ssl_port")]
    pub ssl_port: u16,
    #[serde(default)]
    pub cert: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub ca: String,
    #[serde(default = "def_logging")]
    pub logging_mode: String,
    pub access_log: AccessLog,
    pub metrics: MetricsCfg,
    pub health_check: HealthCfg,
    pub tracing: TraceCfg,
    pub control_plane: ControlPlaneCfg,
    pub builtin: BuiltinCfg,
    pub database: DbCfg,
    pub memory: MemoryCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Proxy {
    #[serde(default = "def_bool")]
    pub enabled: bool,
    pub domains: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccessLog {
    #[serde(default = "def_bool")]
    pub enabled: bool,
    #[serde(default = "def_bool")]
    pub sname: bool,
    pub path: String,
    pub format: String,
    pub max_size: u64,
    pub max_backups: u32,
    pub max_age: u32,
    #[serde(default = "def_bool")]
    pub compress: bool,
}

fn def_metrics_port() -> u16 {
    9090
}

fn def_metrics_format() -> String {
    "prometheus".into()
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsCfg {
    #[serde(default = "def_bool")]
    pub enabled: bool,
    #[serde(default = "def_metrics_path")]
    pub path: String,
    #[serde(default = "def_metrics_port")]
    pub port: u16,
    #[serde(default = "def_metrics_format")]
    pub format: String,
}

fn def_health_port() -> u16 {
    8080
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HealthCfg {
    #[serde(default = "def_bool")]
    pub enabled: bool,
    #[serde(default = "def_health_path")]
    pub path: String,
    #[serde(default = "def_health_port")]
    pub port: u16,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraceCfg {
    #[serde(default = "def_bool")]
    pub enabled: bool,
    pub otlp_endpoint: String,
    pub service_name: String,
}

fn def_controlplane_host() -> String {
    "localhost".into()
}

fn def_controlplane_protocols() -> Vec<String> {
    ["http".into(), "ws".into()].to_vec()
}

fn def_controlplane_poll() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ControlPlaneCfg {
    #[serde(default = "def_bool")]
    pub enabled: bool,
    #[serde(default = "def_controlplane_protocols")]
    pub protocols: Vec<String>,
    #[serde(default = "def_controlplane_host")]
    pub host: String,
    #[serde(default = "random_id")]
    pub id: String,
    pub mtls_cert: String,
    pub mtls_key: String,
    #[serde(default = "def_controlplane_poll")]
    pub poll_interval_sec: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuiltinCfg {
    #[serde(default = "def_bool")]
    pub enabled: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DbCfg {
    #[serde(default = "def_bool")]
    pub enabled: bool,
    pub r#type: String,
}

fn def_engine() -> String { "lmdb".into() }
fn def_memory_path() -> String { "Memory".into() }
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryCfg {
    #[serde(default = "def_engine")]
    pub engine: String,
    #[serde(default = "def_memory_path")]
    pub path: String,
}