use serde::{Deserialize, Serialize};

// ---------- plugins Structure Models (catalog, not applied) ----------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginsCatalog {
    pub plugins: CatalogPlugins,
    pub policies: Vec<CatalogPolicy>,
    pub required_policies: Vec<String>,
    pub required_plugins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogPlugins {
    pub builtin: Vec<BuiltinPluginSpec>,
    pub custom: Option<Vec<CustomPluginSpec>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinPluginSpec {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPluginSpec {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDecl {
    pub id: String,
    pub name: String,
    pub description: String,
    pub r#type: String,
    pub properties: serde_json::Map<String, serde_json::Value>,
    pub required: Option<Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerDecl {
    pub id: String,
    pub name: String,
    pub language: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogPolicy {
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