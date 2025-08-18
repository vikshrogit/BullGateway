
use anyhow::{anyhow, Context, Result};
use bullg_core::{AppliedPlugin, GatewayState, Service};
use serde::{Deserialize, Serialize};
use std::fs;
//use tracing::{debug};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayCfg {
    #[serde(default = "def_host")]
    pub host: String,
    #[serde(default = "def_port")]
    pub port: u16,
    #[serde(default)]
    pub ssl: bool,
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
    #[serde(default)]
    pub name: String,
}
fn def_host() -> String { "0.0.0.0".into() }
fn def_port() -> u16 { 8000 }
fn def_ssl_port() -> u16 { 8443 }
fn def_logging() -> String { "debug".into() }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ControlPlaneCfg {
    pub url: String,
    pub id: String,
    #[serde(default)]
    pub mtls_cert: String,
    #[serde(default)]
    pub mtls_key: String,
    #[serde(default)]
    pub mtls_ca: String,
    #[serde(default)]
    pub https_fallback_url: String,
    #[serde(default = "def_poll")]
    pub poll_interval_sec: u64,
}
fn def_poll() -> u64 { 5 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TracingCfg {
    #[serde(default)]
    pub otlp_endpoint: String,
    #[serde(default)]
    pub service_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryCfg {
    #[serde(default = "def_engine")]
    pub engine: String, // lmdb | memory
    #[serde(default)]
    pub path: String,
}
fn def_engine() -> String { "lmdb".into() }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileConfig {
    pub gateway: GatewayCfg,
    #[serde(default)]
    pub controlplane: ControlPlaneCfg,
    #[serde(default)]
    pub tracing: TracingCfg,
    #[serde(default)]
    pub memory: MemoryCfg,
    #[serde(default)]
    pub services: Vec<Service>,
    #[serde(default)]
    pub plugins: PluginsCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginsCfg {
    #[serde(default)]
    pub global: Vec<AppliedPlugin>,
}

pub fn load_config(path: &str) -> Result<FileConfig> {
    let content = fs::read_to_string(path).with_context(|| format!("read config {}", path))?;
    if path.ends_with(".yaml") || path.ends_with(".yml") {
        Ok(serde_yml::from_str(&content)?)
    } else if path.ends_with(".json") {
        Ok(serde_json::from_str(&content)?)
    } else if path.ends_with(".toml") {
        Ok(toml::from_str(&content)?)
    } else {
        Err(anyhow!("Unknown config extension: {}", path))
    }
}

pub fn to_state(cfg: &FileConfig) -> GatewayState {
    GatewayState {
        services: cfg.services.clone(),
        global_plugins: cfg.plugins.global.clone(),
    }
}
