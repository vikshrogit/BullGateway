use bytes::Bytes;
use futures_util::stream;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use multer::Multipart;
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, json, to_string, to_value, to_vec, Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct BullGTools {
    pub client: reqwest::Client,
    // We will add more Tools such that developers can use in Scripts as well as in Custom Plugins
}

impl BullGTools {
    pub fn new() -> Self {
        let client = reqwest::Client::new();
        Self { client }
    }
    pub async fn httpx_get(&self, url: &str) -> anyhow::Result<String> {
        let resp = self.client.get(url).send().await?;
        Ok(resp.text().await?)
    }
    pub async fn httpx_post(&self, url: &str, body: Bytes) -> anyhow::Result<String> {
        let resp = self.client.post(url).body(body).send().await?;
        Ok(resp.text().await?)
    }
    pub async fn httpx_put(&self, url: &str, body: Bytes) -> anyhow::Result<String> {
        let resp = self.client.put(url).body(body).send().await?;
        Ok(resp.text().await?)
    }

    pub async fn httpx_delete(&self, url: &str) -> anyhow::Result<String> {
        let resp = self.client.delete(url).send().await?;
        Ok(resp.text().await?)
    }

    pub async fn httpx_patch(&self, url: &str, body: Bytes) -> anyhow::Result<String> {
        let resp = self.client.patch(url).body(body).send().await?;
        Ok(resp.text().await?)
    }

    pub async fn httpx_head(&self, url: &str) -> anyhow::Result<String> {
        let resp = self.client.head(url).send().await?;
        Ok(resp.text().await?)
    }

    pub async fn httpx_request(&self, method: Method, url: &str, body: Bytes) -> anyhow::Result<String> {
        let resp = self.client.request(method, url).body(body).send().await?;
        Ok(resp.text().await?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserVars(Value);

impl UserVars {
    pub fn new() -> Self {
        Self(json!({}))
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub fn insert(&mut self, key: &str, value: Value) {
        if let Some(obj) = self.0.as_object_mut() {
            obj.insert(key.to_string(), value);
        }
    }

    pub fn remove(&mut self, key: &str) {
        if let Some(obj) = self.0.as_object_mut() {
            obj.remove(key);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Request {
    pub id: String,
    pub method: Method,
    pub url: Uri,
    pub schema: String,
    pub path: String,
    pub query: Value,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub json: Value,
    pub form: Value,
    pub files: HashMap<String, Vec<u8>>,
}


impl Request {
    pub fn new(method: Method, url: Uri, body: Bytes, headers: HeaderMap) -> Self {
        // Generate unique request ID
        let id = Uuid::new_v4().to_string();

        // Extract scheme
        let schema = url.scheme_str().unwrap_or("http").to_string();

        // Extract path
        let path = url.path().to_string();

        // Extract query params as JSON
        let query = url
            .query()
            .map(|q| {
                let mut map = Map::new();
                for (k, v) in form_urlencoded::parse(q.as_bytes()) {
                    map.insert(k.to_string(), json!(v.to_string()));
                }
                Value::Object(map)
            })
            .unwrap_or_else(|| json!({}));


        let (json_val, form_val, files) = {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(Self::parse_body(&headers, body.clone()))
        };

        //let (json_val, form_val, files) = Self::parse_body(&headers, body.clone()).await;

        Self {
            id,
            method,
            url,
            schema,
            path,
            query,
            headers,
            body,
            json: json_val,
            form: form_val,
            files,
        }
    }

    pub async fn parse_body(headers: &HeaderMap, body: Bytes) -> (Value, Value, HashMap<String, Vec<u8>>) {
        let mut json_val = Value::Null;
        let mut form_val = Value::Null;
        let mut files = HashMap::new();

        if let Some(content_type) = headers.get(http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
            if content_type.contains("application/json") {
                json_val = from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
            } else if content_type.contains("application/x-www-form-urlencoded") {
                let mut map = Map::new();
                for (k, v) in form_urlencoded::parse(&body) {
                    map.insert(k.to_string(), json!(v.to_string()));
                }
                form_val = Value::Object(map);
            } else if content_type.contains("multipart/form-data") {
                // Extract boundary
                if let Some(boundary) = content_type.split("boundary=").nth(1) {
                    let stream = stream::once(async move { Ok::<Bytes, std::io::Error>(body) });
                    let mut multipart = Multipart::new(stream, boundary);
                    let mut map = Map::new();

                    while let Some(field) = multipart.next_field().await.unwrap() {
                        let name = field.name().unwrap_or("").to_string();

                        if let Some(_filename) = field.file_name() {
                            // Treat as file
                            let data = field.bytes().await.unwrap().to_vec();
                            files.insert(name, data);
                        } else {
                            // Treat as form field
                            let text = field.text().await.unwrap_or_default();
                            map.insert(name, json!(text));
                        }
                    }

                    if !map.is_empty() {
                        form_val = Value::Object(map);
                    }
                }
            }
        }

        (json_val, form_val, files)
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub json: Value,
}

impl Default for Response {
    fn default() -> Self {
        Self {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: Bytes::new(),
            json: json!({}),
        }
    }
}

impl Response {
    /// Create a new empty response
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set status
    pub fn status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// Builder: set header
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
        http::header::HeaderName: TryFrom<K>,
        <http::header::HeaderName as TryFrom<K>>::Error: Into<http::Error>,
    {
        if let (Ok(k), Ok(v)) = (
            http::header::HeaderName::try_from(key),
            HeaderValue::try_from(value),
        ) {
            self.headers.insert(k, v);
        }
        self
    }

    /// Builder: set body (raw bytes)
    pub fn body<B: Into<Bytes>>(mut self, body: B) -> Self {
        self.body = body.into();
        self
    }

    /// Builder: set JSON (auto serializes)
    pub fn json<T: serde::Serialize>(mut self, value: &T) -> Self {
        self.json = to_value(value).unwrap_or_else(|_| json!({}));
        if let Ok(text) = to_string(value) {
            self.body = Bytes::from(text);
            self.headers
                .insert("Content-Type", HeaderValue::from_static("application/json"));
        }
        self
    }

    /// Builder: plain text response
    pub fn text<S: Into<String>>(mut self, text: S) -> Self {
        let s = text.into();
        self.body = Bytes::from(s.clone());
        self.headers
            .insert("Content-Type", HeaderValue::from_static("text/plain; charset=utf-8"));
        self.json = json!({ "text": s });
        self
    }

    /// Builder: html response
    pub fn html<S: Into<String>>(mut self, html: S) -> Self {
        let s = html.into();
        self.body = Bytes::from(s.clone());
        self.headers
            .insert("Content-Type", HeaderValue::from_static("text/html; charset=utf-8"));
        self.json = json!({ "html": s });
        self
    }

    /// Check if body is empty
    pub fn is_empty(&self) -> bool {
        self.body.is_empty()
    }

    /// Create from raw bytes
    pub fn from_bytes<B: Into<Bytes>>(status: StatusCode, body: B) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: body.into(),
            json: json!({}),
        }
    }

    /// Create from text
    pub fn from_text<S: Into<String>>(status: StatusCode, text: S) -> Self {
        let text = text.into();
        let mut res = Self::from_bytes(status, Bytes::from(text));
        res.headers
            .insert(http::header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
        res
    }

    /// Create from JSON
    pub fn from_json<T: serde::Serialize>(status: StatusCode, value: T) -> Self {
        let body = to_vec(&value).unwrap_or_default();
        let mut res = Self::from_bytes(status, body);
        res.json = to_value(value).unwrap_or_else(|_| json!({}));
        res.headers
            .insert(http::header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
        res
    }

    /// Shortcut: 200 OK with JSON
    pub fn ok_json<T: serde::Serialize>(value: T) -> Self {
        Self::from_json(StatusCode::OK, value)
    }

    /// Shortcut: 200 OK with text
    pub fn ok_text<S: Into<String>>(text: S) -> Self {
        Self::from_text(StatusCode::OK, text)
    }

    /// Shortcut: 404 Not Found
    pub fn not_found<S: Into<String>>(msg: S) -> Self {
        Self::from_text(StatusCode::NOT_FOUND, msg)
    }

    /// Shortcut: 400 Bad Request
    pub fn bad_request<S: Into<String>>(msg: S) -> Self {
        Self::from_text(StatusCode::BAD_REQUEST, msg)
    }

    /// Shortcut: 500 Internal Server Error
    pub fn internal_error<S: Into<String>>(msg: S) -> Self {
        Self::from_text(StatusCode::INTERNAL_SERVER_ERROR, msg)
    }
}


#[derive(Debug, Clone, Default)]
pub struct BullGCtx {
    pub id: Uuid,
    pub request: Arc<RwLock<Request>>,
    pub response: Arc<RwLock<Response>>,
    pub params: Option<Value>,
    pub vars: Arc<RwLock<UserVars>>,
    pub tools: Arc<BullGTools>,
}


impl BullGCtx {
    pub fn new(method: Method, uri: Uri, headers: HeaderMap, body: Bytes, params: Option<Value>) -> Self {
        let req = Request::new(method, uri, body, headers);
        let resp = Response::new();

        Self {
            id: Uuid::new_v4(),
            request: Arc::new(RwLock::new(req)),
            response: Arc::new(RwLock::new(resp)),
            params,
            vars: Arc::new(RwLock::new(UserVars::default())),
            tools: Arc::new(BullGTools::new()),
        }
    }

    pub fn get_id(&self) -> Uuid {
        self.id
    }

    // ------------------ Request helpers ------------------

    pub async fn get_header(&self, k: &str) -> Option<String> {
        let req = self.request.read().await;
        req.headers.get(k).and_then(|v| v.to_str().ok()).map(|s| s.to_string())
    }

    pub async fn set_header(&self, k: &str, v: &str) {
        let mut req = self.request.write().await;
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k.as_bytes()),
            v.parse()
        ) {
            req.headers.insert(name, val);
        }
    }

    pub async fn remove_header(&self, k: &str) {
        let mut req = self.request.write().await;
        req.headers.remove(k);
    }

    pub async fn get_body(&self) -> Bytes {
        self.request.read().await.body.clone()
    }

    pub async fn get_json(&self) -> Value {
        self.request.read().await.json.clone()
    }

    // ------------------ Response helpers ------------------

    pub async fn set_status(&self, code: StatusCode) {
        let mut resp = self.response.write().await;
        resp.status = code;
    }

    pub async fn set_response_body(&self, body: Bytes) {
        let mut resp = self.response.write().await;
        resp.body = body;
    }

    pub async fn set_response_json<T: serde::Serialize>(&self, val: &T) {
        let mut resp = self.response.write().await;
        *resp = Response::new().status(StatusCode::OK).json(val);
    }

    pub async fn set_response_text(&self, text: &str) {
        let mut resp = self.response.write().await;
        *resp = Response::new().status(StatusCode::OK).text(text);
    }

    // ---------------- UserVars helpers ----------------
    pub async fn var_get(&self, key: &str) -> Option<Value> {
        self.vars.read().await.get(key).cloned()
    }

    pub async fn var_put(&self, key: &str, val: Value) {
        self.vars.write().await.insert(key, val);
    }

    pub async fn var_remove(&self, key: &str) {
        self.vars.write().await.remove(key);
    }
}