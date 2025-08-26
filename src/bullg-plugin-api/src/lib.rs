
use anyhow::Result;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use http::header::HeaderName;

#[derive(Clone)]
pub struct BullGTools {
    pub client: reqwest::Client,
}

impl BullGTools {
    pub fn new() -> Self {
        let client = reqwest::Client::new();
        Self { client }
    }
    pub async fn httpx_get(&self, url: &str) -> Result<String> {
        let resp = self.client.get(url).send().await?;
        Ok(resp.text().await?)
    }
    pub async fn httpx_post(&self, url: &str, body: Bytes) -> Result<String> {
        let resp = self.client.post(url).body(body).send().await?;
        Ok(resp.text().await?)
    }
    pub async fn httpx_put(&self, url: &str, body: Bytes) -> Result<String> {
        let resp = self.client.put(url).body(body).send().await?;
        Ok(resp.text().await?)
    }

    pub async fn httpx_delete(&self, url: &str) -> Result<String> {
        let resp = self.client.delete(url).send().await?;
        Ok(resp.text().await?)
    }

    pub async fn httpx_patch(&self, url: &str, body: Bytes) -> Result<String> {
        let resp = self.client.patch(url).body(body).send().await?;
        Ok(resp.text().await?)
    }

    pub async fn httpx_head(&self, url: &str) -> Result<String> {
        let resp = self.client.head(url).send().await?;
        Ok(resp.text().await?)
    }

    pub async fn httpx_request(&self, method: Method, url: &str, body: Bytes) -> Result<String> {
        let resp = self.client.request(method, url).body(body).send().await?;
        Ok(resp.text().await?)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct UserVars(serde_json::Value);

#[derive(Clone)]
pub struct BullGContext {
    pub id: Uuid,
    pub method: Method,
    pub uri: Uri,
    pub headers: Arc<RwLock<HeaderMap>>,
    pub body: Arc<RwLock<Bytes>>,
    pub status: Arc<RwLock<Option<StatusCode>>>,
    pub vars: Arc<RwLock<UserVars>>,
    pub tools: Arc<BullGTools>,
}

impl BullGContext {
    pub fn new(method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Self {
        Self {
            id: Uuid::new_v4(),
            method,
            uri,
            headers: Arc::new(RwLock::new(headers)),
            body: Arc::new(RwLock::new(body)),
            status: Arc::new(RwLock::new(None)),
            vars: Arc::new(RwLock::new(UserVars::default())),
            tools: Arc::new(BullGTools::new()),
        }
    }

    pub fn get_id(&self) -> Uuid {
        self.id
    }

    pub fn header_get(&self, k: &str) -> Option<String> {
        self.headers.read().get(k).map(|v| v.to_str().ok()).flatten().map(|s| s.to_string())
    }
    pub fn header_put(&self, k: &str, v: &str) {
        self.headers.write().insert(HeaderName::from_bytes(k.as_bytes()).unwrap(), v.parse().unwrap());
    }

    pub fn set_headers(&self, headers: HeaderMap) {
        *self.headers.write() = headers;
    }

    pub fn header_remove(&self, k: &str) {
        self.headers.write().remove(k);
    }
    pub fn set_status(&self, code: StatusCode) {
        *self.status.write() = Some(code);
    }
    pub fn get_body(&self) -> Bytes { self.body.read().clone() }
    pub fn set_body(&self, b: Bytes) { *self.body.write() = b; }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase { Pre, Post, Intermediate }

pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn phase(&self) -> Phase;
    fn apply(&self, ctx: &BullGContext, config: &serde_json::Value) -> Result<()>;
}
