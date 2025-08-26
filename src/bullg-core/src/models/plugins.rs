use serde::{Deserialize, Serialize};
use serde_json;
use uuid::Uuid;

// ---------- plugins Structure Models (catalog, not applied) ----------
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginsCatalog {
    pub plugins: CatalogPlugins,
    pub policies: Vec<CatalogPolicy>,
    pub required_policies: Vec<String>,
    pub required_plugins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CatalogPlugins {
    pub builtin: Vec<BuiltinPluginSpec>,
    pub custom: Option<Vec<CustomPluginSpec>>,
}

fn def_plugin_id() -> String {
    Uuid::new_v4().into()
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuiltinPluginSpec {
    #[serde(default = "def_plugin_id")]
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub version: String,
    pub r#type: String,
    pub phases: Vec<String>,
    pub schema: SchemaDecl,
    pub handler: HandlerDecl,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CustomPluginSpec {
    #[serde(default = "def_plugin_id")]
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub version: String,
    pub r#type: String,
    pub phases: Vec<String>,
    pub schema: SchemaDecl,
    pub handler: HandlerDecl,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaDecl {
    #[serde(default = "def_plugin_id")]
    pub id: String,
    pub name: String,
    pub description: String,
    pub r#type: String,
    pub properties: serde_json::Map<String, serde_json::Value>,
    pub required: Option<Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HandlerDecl {
    #[serde(default = "def_plugin_id")]
    pub id: String,
    pub name: String,
    pub language: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CatalogPolicy {
    #[serde(default = "def_plugin_id")]
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub version: String,
    pub r#type: String,
    pub phases: Vec<String>,
    pub schema: SchemaDecl,
    pub handler: HandlerDecl,
}