pub mod config;
pub mod plugins;
pub mod services;
pub mod consumers;
pub mod globals;
pub mod gateway;

pub use consumers::*;
pub use config::*;
pub use globals::*;
pub use plugins::*;
pub use services::*;
pub use gateway::*;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json;
use serde_yml;
use std::fs;
use toml;
use uuid::Uuid;
use std::fmt::Debug;

fn def_snapid() -> Uuid {
    Uuid::new_v4()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSnapshot {
    pub config: GatewayConfig,
    pub plugins_catalog: PluginsCatalog,
    pub consumers: ConsumersTemplate,
    pub services: ServicesTemplate,
    #[serde(default = "def_snapid")]
    pub snapshot_id: Uuid,
}

impl RuntimeSnapshot {
    pub fn new(
        config: GatewayConfig,
        plugins_catalog: PluginsCatalog,
        consumers: ConsumersTemplate,
        services: ServicesTemplate,
    ) -> Self {
        Self {
            config,
            plugins_catalog,
            consumers,
            services,
            snapshot_id: Uuid::new_v4(),
        }
    }

    pub fn reload_config(&mut self, config: GatewayConfig) -> &mut Self {
        self.config = config;
        //self.snapshot_id = Uuid::new_v4();
        self
    }

    pub fn reload_plugins_catalog(&mut self, plugins_catalog: PluginsCatalog) -> &mut Self {
        self.plugins_catalog = plugins_catalog;
        //self.snapshot_id = Uuid::new_v4();
        self
    }

    pub fn reload_consumers(&mut self, consumers: ConsumersTemplate) -> &mut Self {
        self.consumers = consumers;
        //self.snapshot_id = Uuid::new_v4();
        self
    }

    pub fn reload_services(&mut self, services: ServicesTemplate) -> &mut Self {
        self.services = services;
        //self.snapshot_id = Uuid::new_v4();
        self
    }

    pub fn get_gateway_node(&mut self) -> GatewayNode {
        self.config.gateway.clone()
    }

    pub fn get_global(&mut self) -> GlobalApplied {
        self.services.global.clone()
    }

    pub fn get_global_plugins(&mut self) -> Vec<AppliedPlugin> {
        self.services.global.plugins.clone()
    }

    pub fn get_global_policies(&mut self) -> Vec<AppliedPolicy> {
        self.services.global.policies.clone()
    }

    pub fn get_services(&mut self) -> Vec<Service> {
        self.services.services.clone()
    }

    pub fn get_plugins_catalog(&mut self) -> PluginsCatalog {
        self.plugins_catalog.clone()
    }

    pub fn get_list_plugins(&mut self) -> CatalogPlugins {
        self.plugins_catalog.plugins.clone()
    }

    pub fn get_list_policies(&mut self) -> Vec<CatalogPolicy> {
        self.plugins_catalog.policies.clone()
    }

    pub fn get_required_plugins(&mut self) -> Vec<String> {
        self.plugins_catalog.required_plugins.clone()
    }

    pub fn get_required_policies(&mut self) -> Vec<String> {
        self.plugins_catalog.required_policies.clone()
    }

    pub fn get_builtin_plugins(&mut self) -> Vec<BuiltinPluginSpec> {
        self.plugins_catalog.plugins.builtin.clone()
    }

    pub fn get_custom_plugins(&mut self) -> Option<Vec<CustomPluginSpec>> {
        self.plugins_catalog.plugins.custom.clone()
    }

    pub fn get_consumers(&mut self) -> Vec<Consumer> {
        self.consumers.consumers.clone()
    }
}

fn read_file<T>(path: &str) -> T
where
    T: DeserializeOwned + Default + Debug,
{
    let content = fs::read_to_string(path);
    //println!("🔍 Loading Content: {:?}", content);

    match content {
        Ok(content) => {
            let parsed: Result<T, anyhow::Error> = if path.ends_with(".yaml") || path.ends_with(".yml") {
                serde_yml::from_str(&content).map_err(Into::into)
            } else if path.ends_with(".json") {
                serde_json::from_str(&content).map_err(Into::into)
            } else if path.ends_with(".toml") {
                toml::from_str(&content).map_err(Into::into)
            } else {
                Ok(T::default())
            };

            match parsed {
                Ok(val) => {
                    val
                },
                Err(_e) => {
                    eprintln!("⚠️ Failed to parse config file `{}`: {}", path, _e);
                    T::default()
                }
            }
        }
        Err(_e) => {
            //eprintln!("⚠️ Failed to read config file `{}`: {}", path, _e);
            T::default()
        }
    }
}

pub fn load_all(config_p: &str, plugins_p: &str, consumers_p: &str, services_p: &str) -> RuntimeSnapshot {
    let config: GatewayConfig = read_file(config_p);
    let plugins_catalog: PluginsCatalog = read_file(plugins_p);
    let consumers: ConsumersTemplate = read_file(consumers_p);
    let services: ServicesTemplate = read_file(services_p);

    // Here we can validate Plugins, service (Supported Gateway versions extra)

    RuntimeSnapshot {
        config,
        plugins_catalog,
        consumers,
        services,
        snapshot_id: Uuid::new_v4(),
    }
}