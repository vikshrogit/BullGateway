use rayon::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;
use matchit::{Router};
use anyhow::Result;
use serde::de::{Error, SeqAccess, Visitor};
use serde::ser::SerializeSeq;

fn def_id() -> String {
    Uuid::new_v4().into()
}

fn def_version() -> String {
    "1.0".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServicesMapperVec {
    pub services: Vec<ServiceMapper>,
}

impl IntoIterator for ServicesMapperVec {
    type Item = ServiceMapper;
    type IntoIter = std::vec::IntoIter<ServiceMapper>;

    fn into_iter(self) -> Self::IntoIter {
        self.services.into_iter()
    }
}

pub trait ToServicesMapperVec {
    fn get_services_map_vec_ref(&self) -> ServicesMapperVec;
    fn get_services_map_vec(&self) -> ServicesMapperVec;
}

// ---------- services.yaml ----------
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServicesTemplate {
    pub gateway: String,
    #[serde(default = "def_version")]
    pub version: String,
    #[serde(rename = "release")]
    pub release_channel: String,
    #[serde(rename = "bullg-versions")]
    pub bullg_versions: Vec<String>,
    pub developer: String,
    pub global: GlobalApplied,
    pub services: Vec<Service>,
}

impl ToServicesMapperVec for ServicesTemplate {
    fn get_services_map_vec_ref(&self) -> ServicesMapperVec {
        let mut map: HashMap<String, Service> = HashMap::with_capacity(self.services.len() * 3);

        for svc in &self.services {
            for sm in svc.get_service_maps_ref() {
                map.insert(sm.key.clone(), sm.value.clone());
            }
        }

        let services = map
            .into_iter()
            .map(|(k, v)| ServiceMapper { key: k, value: v })
            .collect();

        ServicesMapperVec { services }
    }

    fn get_services_map_vec(&self) -> ServicesMapperVec {
        let n_services = self.services.len();

        // Small number of services: sequential
        if n_services <= 50 {
            // let mut services_map = Vec::with_capacity(n_services * 2);
            // for service in &self.services {
            //     services_map.extend(service.get_service_maps());
            // }
            // return ServicesMapperVec { services: services_map };

            let mut map: HashMap<String, Service> = HashMap::new();

            for svc in &self.services {
                for m in svc.get_service_maps() {
                    // last wins; change to entry().or_insert if you prefer first-wins
                    map.insert(m.key, m.value);
                }
            }

            let services = map
                .into_iter()
                .map(|(k, v)| ServiceMapper { key: k, value: v })
                .collect();

            return ServicesMapperVec { services };
        }

        // Large number of services: parallel
        let maps: Vec<Vec<ServiceMapper>> = self
            .services
            .par_iter()
            .map(|service| service.get_service_maps())
            .collect();

        // Flatten into one Vec
        let mut services_map = Vec::with_capacity(n_services * 2);
        for map in maps {
            services_map.extend(map);
        }

        // Optional deduplication by key (last wins)
        let mut dedup: HashMap<String, Service> = HashMap::with_capacity(services_map.len());
        for m in services_map {
            dedup.insert(m.key, m.value);
        }

        let services = dedup
            .into_iter()
            .map(|(k, v)| ServiceMapper { key: k, value: v })
            .collect();

        ServicesMapperVec { services }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalApplied {
    pub plugins: Vec<AppliedPlugin>,
    pub policies: Vec<AppliedPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceMapper {
    pub key: String,
    pub value: Service,
}

impl IntoIterator for ServiceMapper {
    type Item = (String, Service);
    type IntoIter = std::iter::Once<(String, Service)>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once((self.key, self.value))
    }

}

pub trait ToServiceMapper {
    fn get_service_maps_ref(&self) -> Vec<ServiceMapper>;
    fn get_service_map(&self) -> ServiceMapper;
    fn get_service_maps(&self) -> Vec<ServiceMapper>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocols {
    #[serde(alias = "http")]
    HTTP,
    #[serde(alias = "https")]
    HTTPS,
    #[serde(alias = "tcp")]
    TCP,
    #[serde(alias = "udp")]
    UDP,
    #[serde(alias = "grpc")]
    GRPC,
    #[serde(alias = "grpcs")]
    GRPCS,
    #[serde(alias = "ws")]
    WS,
    #[serde(alias = "wss")]
    WSS,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Service {
    #[serde(default = "def_id")]
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub protocols: Vec<Protocols>,
    pub spec: Option<ServiceSpec>,
    pub versions: Vec<ServiceVersion>,
    pub upstreams: Vec<Upstream>,
    #[serde(rename = "contextPaths")]
    pub context_paths: ServiceContextPaths,
    pub plugins: Vec<AppliedPlugin>,
    pub policies: Vec<AppliedPolicy>,
    pub consumers: Vec<ServiceConsumer>,
    pub routes: Vec<Route>,
    #[serde(default)]
    pub router: BullGRoute,
}

impl ToServiceMapper for Service {
    fn get_service_maps_ref(&self) -> Vec<ServiceMapper> {
        let mut maps = Vec::with_capacity(self.versions.len());

        let all_versions: Vec<&str> = self.versions.iter().map(|v| v.id.as_str()).collect();

        if all_versions.is_empty() {
            let key = self
                .context_paths
                .paths
                .get(0)
                .map(|cp| cp.path.as_str())
                .unwrap_or(&self.name);
            maps.push(ServiceMapper { key: key.to_string(), value: self.clone() });
            return maps;
        }

        for v in &all_versions {
            let key = self
                .context_paths
                .paths
                .iter()
                .find(|cp| cp.versions.is_empty() || cp.versions.contains(&v.to_string()))
                .map(|cp| cp.path.as_str())
                .unwrap_or(&self.name);

            maps.push(ServiceMapper { key: key.to_string(), value: self.clone()});
        }

        maps
    }

    fn get_service_map(&self) -> ServiceMapper {
        // Keep this simple path for compat; the real work is in get_service_maps.
        let key = if self.context_paths.enable && !self.context_paths.paths.is_empty() {
            self.context_paths.paths[0].path.clone()
        } else {
            format!("/{}", self.name)
        };
        ServiceMapper {
            key,
            value: self.clone(),
        }
    }

    fn get_service_maps(&self) -> Vec<ServiceMapper> {
        let all_versions: Vec<&str> = self.versions.iter().map(|v| v.id.as_str()).collect();
        if all_versions.is_empty() {
            let key = if self.context_paths.enable && !self.context_paths.paths.is_empty() {
                self.context_paths.paths[0].path.clone()
            } else {
                format!("/{}/", self.name)
            };
            let mut own = self.clone();
            let _ =own.build_router();
            return vec![ServiceMapper {
                key,
                value: own,
            }];
        }

        // per-version paths
        let mut per_version_paths: HashMap<&str, Vec<String>> =
            all_versions.iter().map(|v| (*v, Vec::new())).collect();

        // base_path -> versions mapping
        let mut path_to_versions: HashMap<String, Vec<&str>> = HashMap::new();
        if self.context_paths.enable && !self.context_paths.paths.is_empty() {
            for cp in &self.context_paths.paths {
                if cp.versions.is_empty() {
                    for v in &all_versions {
                        path_to_versions.entry(cp.path.clone()).or_default().push(v);
                    }
                } else {
                    for v in &cp.versions {
                        if all_versions.contains(&v.as_str()) {
                            path_to_versions.entry(cp.path.clone()).or_default().push(v);
                        }
                    }
                }
            }
        } else {
            // default base path
            let base = format!("/{}/", self.name);
            for v in &all_versions {
                path_to_versions.entry(base.clone()).or_default().push(v);
            }
        }

        // assign paths per version
        for (base_path, versions_for_path) in &path_to_versions {
            if versions_for_path.len() > 1 {
                for v in versions_for_path {
                    per_version_paths.get_mut(v).unwrap().push(format!(
                        "{}/{}/",
                        base_path.trim_end_matches('/'),
                        v
                    ));
                }
            } else {
                let v = versions_for_path[0];
                per_version_paths
                    .get_mut(v)
                    .unwrap()
                    .push(base_path.clone());
            }
        }

        // build ServiceMapper
        let mut seen_keys = HashSet::new();
        let mut out = Vec::with_capacity(all_versions.len());

        for v in &all_versions {
            let paths = per_version_paths.get(v).unwrap();
            if paths.is_empty() {
                continue;
            }
            let chosen_path = &paths[0];
            if !seen_keys.insert(chosen_path.clone()) {
                continue;
            }

            let has_v =
                |versions: &Vec<String>| versions.is_empty() || versions.iter().any(|x| x == *v);

            let upstreams: Vec<Upstream> = self
                .upstreams
                .iter()
                .filter(|u| has_v(&u.versions))
                .cloned()
                .collect();
            let plugins: Vec<AppliedPlugin> = self
                .plugins
                .iter()
                .filter(|p| match &p.versions {
                    None => true,
                    Some(vs) => vs.is_empty() || vs.iter().any(|x| x == *v),
                })
                .cloned()
                .collect();
            let policies: Vec<AppliedPolicy> = self
                .policies
                .iter()
                .filter(|p| match &p.version {
                    None => true,
                    Some(vs) => vs.is_empty() || vs.iter().any(|x| x == *v),
                })
                .cloned()
                .collect();
            let consumers: Vec<ServiceConsumer> = self
                .consumers
                .iter()
                .filter(|c| has_v(&c.versions))
                .cloned()
                .collect();
            let routes: Vec<Route> = self
                .routes
                .iter()
                .filter(|r| has_v(&r.versions))
                .map(|r| {
                    let r_plugins: Vec<AppliedPlugin> = r
                        .plugins
                        .iter()
                        .filter(|p| match &p.versions {
                            None => true,
                            Some(vs) => vs.is_empty() || vs.iter().any(|x| x == *v),
                        })
                        .cloned()
                        .collect();
                    let mut r2 = r.clone();
                    r2.plugins = r_plugins;
                    r2
                })
                .collect();
            let versions: Vec<ServiceVersion> = self
                .versions
                .iter()
                .filter(|sv| sv.id == **v)
                .cloned()
                .collect();

            let context_paths = ServiceContextPaths {
                enable: true,
                paths: vec![ContextPath {
                    path: chosen_path.clone(),
                    versions: vec![(*v).to_string()],
                }],
            };

            let spec = self.spec.as_ref().map(|s| {
                let mut s2 = s.clone();
                s2.versions.retain(|sv| sv == *v);
                s2
            });

            let mut svc = self.clone();
            svc.spec = spec;
            svc.versions = versions;
            svc.upstreams = upstreams;
            svc.plugins = plugins;
            svc.policies = policies;
            svc.consumers = consumers;
            svc.routes = routes;
            svc.context_paths = context_paths;
            let _ = svc.build_router();

            out.push(ServiceMapper {
                key: chosen_path.clone(),
                value: svc,
            });
        }

        out
    }
}


impl Service {
    pub fn get_version_ids(&self) -> Vec<String> {
        self.versions.iter().map(|v| v.id.clone()).collect()
    }

    pub fn build_router(&mut self)-> Result<()>{
        self.router = BullGRoute::new();
        for r in self.routes.iter() {
            self.router.add_route(Arc::new(r.clone()))?;
        }
        Ok(())
    }

    pub fn remove_router(&mut self, route:Route)-> Result<()>{
        self.router.remove_route(&route.config.path);
        Ok(())
    }

}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceSpec {
    pub enabled: bool,
    pub route: String,
    pub versions: Vec<String>,
}

impl ServiceSpec {
    pub fn is_version_supported(&self, version: &str) -> bool {
        self.versions.is_empty() || self.versions.iter().any(|v| v == version)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn execute_route(&self) -> &str {
        &self.route
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceVersion {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub description: String,
    pub deprecated: bool,
}

impl ServiceVersion {
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_deprecated(&self) -> bool {
        self.deprecated
    }

}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Upstream {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub protocols: Vec<Protocols>,
    pub host: String,
    pub port: u16,
    pub enabled: bool,
    pub versions: Vec<String>,
}


impl Upstream {
    pub fn is_version_supported(&self, version: &str) -> bool {
        self.versions.is_empty() || self.versions.iter().any(|v| v == version)
    }   

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn get_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
    
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceContextPaths {
    pub enable: bool,
    pub paths: Vec<ContextPath>,
}

impl ServiceContextPaths {
    pub fn is_enabled(&self) -> bool {
        self.enable && !self.paths.is_empty()
    }

    pub fn get_all_paths(&self) -> Vec<String> {
        self.paths.iter().map(|cp| cp.path.clone()).collect()
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextPath {
    pub path: String,
    pub versions: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceConsumer {
    pub id: String,
    pub enabled: bool,
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouteConfig {
    pub protocols: Vec<Protocols>,
    pub path: String,
    pub backend: String,
    pub methods: Vec<String>,
}


#[derive(Debug, Clone, Default)]
pub struct BullGRouter {
    pub services: Router<Arc<Service>>,
    pub default_services: Vec<Arc<Service>>,
}

impl BullGRouter {
    pub fn new() -> Self {
        let services:Router<Arc<Service>> = Router::new();
        Self {
            services,
            default_services: Vec::new(),
        }
    }

    pub fn add_service(&mut self, service: Arc<Service>) -> Result<()>{
        if service.context_paths.enable{
            for cp in service.context_paths.paths.iter() {
                self.services.insert(&cp.path, service.clone())?;
            }
        }else{
            self.default_services.push(service.clone());
        }
        Ok(())
    }

    pub fn add_service_mapper(&mut self, servicemaps: Vec<ServiceMapper>) -> Result<()> {
        for map in servicemaps.iter() {
            if map.value.context_paths.enable{
                let mut path = map.key.trim_end_matches("/").to_string();
                if !path.starts_with('/') { path = format!("/{}", path); }
                //let _ = self.services.insert(&path, Arc::new(map.value.clone()));
                path = format!("{}/{}",path,"{*routes}");
                //println!("Map Key: {:?}",path);
                let _ = self.services.insert(&path, Arc::new(map.value.clone()));
                //println!("Map Value Result: {:?}",e);
            }else {
                self.default_services.push(Arc::new(map.value.clone()));
            }
        }
        Ok(())
    }
    
    pub fn update_service_mappers(&mut self, servicemaps: Vec<ServiceMapper>) -> Result<()> {
        // need more logic for update services
        for map in servicemaps.iter() {
            if map.value.context_paths.enable{
                let mut path = map.key.trim_end_matches("/").to_string();
                if !path.starts_with('/') { path = format!("/{}", path); }
                path = format!("{}/{}",path,"{*routes}");
                let _ = self.services.insert(&path, Arc::new(map.value.clone()));
            }
        }
        
        Ok(())
    }

    pub fn find_service(&self, path: &str) -> Option<(Arc<Service>, HashMap<String, String>)> {
        //println!("Finding service: {:?}", path);
        if let Ok(matched) = self.services.at(path) {
            //println!("Matched: {:?}", matched.params);
            let params = matched
                .params
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<HashMap<_, _>>();
            Some((matched.value.clone(), params))
        } else if !self.default_services.is_empty() {
            Some((self.default_services[0].clone(), HashMap::new()))
        }else {
            None
        }
    }
    
    pub fn remove_service(&mut self, path: &str) -> Option<Arc<Service>> {
        self.services.remove(path)
    }
}



#[derive(Debug, Clone, Default)]
pub struct BullGRoute {
    pub routes: Router<Arc<Route>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SerializeRoute{
    path: String,
    routes: Route,
}
impl Serialize for BullGRoute {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Represent routes as Vec<(String, Arc<Route>)>
        let seq: Vec<SerializeRoute> = Vec::new();
        let mut s = serializer.serialize_seq(Some(seq.len()))?;
        for item in seq {
            s.serialize_element(&item)?;
        }
        s.end()
    }
}

impl<'de> Deserialize<'de> for BullGRoute {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BullGRouteVisitor;

        impl<'de> Visitor<'de> for BullGRouteVisitor {
            type Value = BullGRoute;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a list of (path, Route)")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut router = Router::new();
                while let Some((path, route)) = seq.next_element::<(String, Route)>()? {
                    router
                        .insert(path, Arc::new(route))
                        .map_err(|e| A::Error::custom(format!("router insert error: {e}")))?;
                }
                Ok(BullGRoute { routes: router })
            }
        }

        deserializer.deserialize_seq(BullGRouteVisitor)
    }

    fn deserialize_in_place<D>(
        deserializer: D,
        place: &mut Self,
    ) -> Result<(), D::Error>
    where
        D: Deserializer<'de>,
    {
        *place = Deserialize::deserialize(deserializer)?;
        Ok(())
    }
}
impl BullGRoute {
    pub fn new() -> Self {
        let routes: Router<Arc<Route>> = Router::new();
        Self {
            routes
        }
    }
    
    pub fn add_route(&mut self, route: Arc<Route>) -> Result<()> {
        if route.enabled{
            self.routes.insert(&route.config.path, route.clone())?;
        }
        Ok(())
    }
    
    pub fn remove_route(&mut self, path: &str) -> Option<Arc<Route>> {
        self.routes.remove(path)
    }

    pub fn find_route(&self, path: &str) -> Option<Arc<Route>> {
        if let Ok(matched) = self.routes.at(path) {
            Some(matched.value.clone())
        }else{
            None
        }
    }
}
