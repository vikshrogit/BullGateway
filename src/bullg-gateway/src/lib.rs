use anyhow::Result;
use bullg_core::{ AppliedPlugin, GatewayState, Route, Service };
use bullg_memory::Store;
use bullg_plugin_api::{ BullGContext, Phase, Plugin };
use bytes::Bytes;
use dashmap::DashMap;
use http::{ Request, Response, StatusCode, Uri, header::HeaderValue };
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use http_body_util::{ BodyExt, Full };
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use hyper_util::rt::tokio::TokioIo;
use tracing::{ error, info, debug };
use url::Url;
//use uuid::Uuid;
use std::time::Instant;
use chrono::{Datelike, Utc};

#[derive(Clone)]
pub struct Gateway {
    state: Arc<DashMap<String, Service>>,
    global_plugins: Arc<tokio::sync::RwLock<Vec<AppliedPlugin>>>, // interior mutability
    store: Arc<Store>,
    plugins: Arc<Vec<Box<dyn Plugin>>>,
    client: reqwest::Client,
}

// Inject app name & version at compile-time from Cargo.toml
const APP_NAME: &str = env!("APP_NAME");
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

impl Gateway {
    pub fn new(store: Store) -> Self {
        Self {
            state: Arc::new(DashMap::new()),
            global_plugins: Arc::new(tokio::sync::RwLock::new(vec![])),
            store: Arc::new(store),
            plugins: Arc::new(bullg_plugins::builtin()),
            client: reqwest::Client::new(),
        }
    }

    pub fn get_store(&self) -> Arc<Store> {
        self.store.clone()
    }

    pub async fn update_state(&self, s: GatewayState) {
        //debug!("updating state with {} services", s.services.len());
        debug!("current state: {:?}", s);
        self.state.clear();
        for svc in s.services {
            self.state.insert(svc.id.clone(), svc);
        }
        let mut gp = self.global_plugins.write().await;
        *gp = s.global_plugins;
        debug!("state updated: {} services", self.state.len());
    }

    fn match_route(&self, uri: &Uri) -> Option<(Service, Route)> {
        let path = uri.path();
        debug!("matching route for path: {}", path);
        debug!("current state: {:?}", self.state);
        for svc in self.state.iter() {
            for r in &svc.routes {
                if path.starts_with(&r.path) {
                    return Some((svc.clone(), r.clone()));
                }
            }
        }
        None
    }

    async fn run_plugins(&self, phase: Phase, ctx: &BullGContext, list: &[AppliedPlugin]) {
        for ap in list {
            if
                let Some(p) = self.plugins
                    .iter()
                    .find(|p| p.name() == ap.name && p.phase() == phase)
            {
                if let Err(e) = p.apply(ctx, &ap.config) {
                    error!("plugin {} failed: {e}", ap.name);
                }
                if ctx.status.read().is_some() && phase == Phase::Pre {
                    break;
                }
            }
        }
    }

    pub async fn serve(self: Arc<Self>, addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!("{} listening on {}", APP_NAME, addr);
        loop {
            let (stream, _) = listener.accept().await?;
            let me = self.clone();
            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                let conn = http1::Builder::new().serve_connection(
                    io,
                    service_fn(move |req| {
                        let me = me.clone();
                        async move { me.handle(req).await }
                    })
                );
                if let Err(e) = conn.await {
                    error!("conn error: {e}");
                }
            });
        }
    }

    async fn handle(&self, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let start = Instant::now();
        // Generate a unique request ID
        //let request_id = Uuid::new_v4().to_string();

        let (parts, body) = req.into_parts();
        let body_bytes = body.collect().await?.to_bytes();
        let ctx = BullGContext::new(
            parts.method.clone(),
            parts.uri.clone(),
            parts.headers.clone(),
            body_bytes.clone()
        );
        let request_id = ctx.get_id().to_string();

        info!("Handling request {}: {} {}", request_id, parts.method.clone(), parts.uri.clone());

        let gp = self.global_plugins.read().await;
        self.run_plugins(Phase::Pre, &ctx, &gp).await;
        if let Some(code) = *ctx.status.read() {
            return Ok(self.default_headers(simple(code, ctx.get_body()), &request_id, start));
        }

        let (svc, route) = match self.match_route(&parts.uri) {
            Some(x) => x,
            None => {
                return Ok(
                    self.default_headers(
                        simple(
                            StatusCode::NOT_FOUND,
                            Bytes::from(
                                format!(
                                    "<html><head><title>BullG: Route Not Found</title></head>\
                                    <body><h1>Not Found</h1>\
                                    <h2>Route not found</h2>\
                                    <p><b>Request id:</b> {request_id}</p>\
                                    <br/><hr/> \
                                    <center><p>{app_name} Gateway {app_version} &copy; {year}</p></center>\
                                    </body></html>",
                                    request_id = request_id,
                                    app_name = APP_NAME,
                                    app_version = APP_VERSION,
                                    year = Utc::now().year()
                                )
                            )
                        ),
                        &request_id,
                        start
                    )
                );
            }
        };

        let upstream = format!("{}{}", svc.url, route.path);
        let mut url = Url::parse(&upstream).unwrap();
        url.set_path(parts.uri.path());
        url.set_query(parts.uri.query());

        let upstream_host = url.host_str().unwrap_or_default();

        // Modify headers: preserve original host and set forwarding headers
        {
            let mut headers = ctx.headers.write();
            if let Some(orig_host) = headers.get("host").cloned() {
                headers.insert("x-forwarded-host", orig_host);
            }
            headers.insert("host", HeaderValue::from_str(&upstream_host).unwrap());
            headers.insert("via", HeaderValue::from_str(&APP_NAME).unwrap());
            headers.insert("referer", HeaderValue::from_str(&parts.uri.to_string()).unwrap());
        }

        let mut rb = self.client.request(parts.method.clone(), url.as_str());
        for (k, v) in ctx.headers.read().iter() {
            rb = rb.header(k, v);
        }

        debug!(
            "upstream request: {} {:?} {:?}",
            parts.method.clone(),
            url.as_str(),
            ctx.headers.read()
        );
        let upstart = Instant::now();
        let resp = match rb.body(ctx.get_body().to_vec()).send().await {
            Ok(r) => r,
            Err(e) => {
                error!("upstream error: {e}");
                return Ok(
                    self.default_headers(
                        simple(StatusCode::BAD_GATEWAY, Bytes::from_static(b"upstream error")),
                        &request_id,
                        start
                    )
                );
            }
        };
        info!("upstream Latency: {:?}", upstart.elapsed().as_millis().to_string());
        let status = StatusCode::from_u16(resp.status().as_u16()).unwrap();
        ctx.set_headers(resp.headers().clone());
        let bytes = resp.bytes().await.unwrap_or(Bytes::new());
        debug!("upstream response: {} {:?}", status, bytes);
        ctx.set_body(bytes.clone());

        ctx.set_status(status);

        self.run_plugins(Phase::Post, &ctx, &gp).await;

        Ok(self.default_headers_from_ctx(&ctx, &request_id, start))
    }

    fn default_headers(
        &self,
        mut resp: Response<Full<Bytes>>,
        request_id: &str,
        start: Instant
    ) -> Response<Full<Bytes>> {
        let latency_us = start.elapsed().as_micros().to_string();
        let latency_ms = start.elapsed().as_millis().to_string();

        resp.headers_mut().insert("Via", HeaderValue::from_str(APP_NAME).unwrap());
        if !resp.headers_mut().get("Content-Type").is_some() {
            let accept = resp.headers_mut().get("Accept");

            //println!("Accept = {:?}", accept);
            if ! accept.is_some() {
                resp.headers_mut().insert(
                    "Content-Type",
                    HeaderValue::from_str("text/html").unwrap()
                );
            } else {
                println!("Accept = {:?}", accept);
                //resp.headers_mut().insert("Content-Type", );
            }
        }
        resp.headers_mut().insert(
            "Server",
            HeaderValue::from_str(&format!("{}/{}", APP_NAME, APP_VERSION)).unwrap()
        );
        resp.headers_mut().insert(
            "X-Powered-By",
            HeaderValue::from_str(&format!("{}-{}/VIKSHRO", APP_NAME, APP_VERSION)).unwrap()
        );
        resp.headers_mut().insert(
            "X-Server",
            HeaderValue::from_str(&format!("{}/{}", APP_NAME, APP_VERSION)).unwrap()
        );
        resp.headers_mut().insert(
            "X-Gateway",
            HeaderValue::from_str(&format!("{} Gateway/{}", APP_NAME, APP_VERSION)).unwrap()
        );
        resp.headers_mut().insert("X-Latency-Us", HeaderValue::from_str(&latency_us).unwrap());
        resp.headers_mut().insert("X-Latency", HeaderValue::from_str(&latency_ms).unwrap());
        resp.headers_mut().insert("X-Request-Id", HeaderValue::from_str(request_id).unwrap());

        resp
    }

    fn default_headers_from_ctx(
        &self,
        ctx: &BullGContext,
        request_id: &str,
        start: Instant
    ) -> Response<Full<Bytes>> {
        //debug!("response: {:?}", ctx.get_body());
        let mut resp = Response::builder()
            .status(*ctx.status.read().as_ref().unwrap_or(&StatusCode::OK))
            .body(Full::new(ctx.get_body()))
            .unwrap();

        // Apply headers from context
        for (k, v) in ctx.headers.read().iter() {
            resp.headers_mut().insert(k.clone(), v.clone());
        }

        // Add default headers
        self.default_headers(resp, request_id, start)
    }
}

fn simple(status: StatusCode, body: Bytes) -> Response<Full<Bytes>> {
    Response::builder().status(status).body(Full::new(body)).unwrap()
}
