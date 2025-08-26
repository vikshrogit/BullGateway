use anyhow::{ Result };
use bullg_plugin_api::{ BullGContext, Phase, Plugin };
use bytes::Bytes;
use http::StatusCode;
//use tracing::info;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

pub struct Cors;
impl Plugin for Cors {
    fn name(&self) -> &'static str {
        "cors"
    }
    fn phase(&self) -> Phase {
        Phase::Pre
    }
    fn apply(&self, ctx: &BullGContext, cfg: &serde_json::Value) -> Result<()> {
        let allow_origin = cfg
            .get("allow_origin")
            .and_then(|v| v.as_str())
            .unwrap_or("*");
        ctx.header_put("access-control-allow-origin", allow_origin);
        Ok(())
    }
}

pub struct RequestTermination;
impl Plugin for RequestTermination {
    fn name(&self) -> &'static str {
        "request_termination"
    }
    fn phase(&self) -> Phase {
        Phase::Pre
    }
    fn apply(&self, ctx: &BullGContext, cfg: &serde_json::Value) -> Result<()> {
        if
            cfg
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        {
            let status = cfg
                .get("status")
                .and_then(|v| v.as_u64())
                .unwrap_or(403);
            let body = cfg
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("Request terminated");
            ctx.set_status(StatusCode::from_u16(status as u16).unwrap());
            ctx.set_body(Bytes::from(body.as_bytes().to_vec()));
        }
        Ok(())
    }
}

pub struct HttpLog;

impl Plugin for HttpLog {
    fn name(&self) -> &'static str {
        "http_log"
    }
    fn phase(&self) -> Phase {
        Phase::Post
    }
    fn apply(&self, ctx: &BullGContext, cfg: &serde_json::Value) -> Result<()> {
        if let Some(endpoint) = cfg.get("endpoint").and_then(|v| v.as_str()) {
            // Own it as String so it can live inside tokio::spawn
            let endpoint = endpoint.to_string();

            let client = ctx.tools.client.clone();

            tokio::spawn(async move {
                let _ = client
                    .post(endpoint)
                    .json(
                        &serde_json::json!({
                    "message": "Hello from plugin!"
                })
                    )
                    .send().await;
            });
        }

        if let Some(b64) = cfg.get("b64").and_then(|v| v.as_str()) {
            // Own it too
            let b64 = b64.to_string();

            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&b64) {
                let decoded = String::from_utf8_lossy(&bytes);
                println!("Decoded base64: {}", decoded);
            }
        }

        Ok(())
    }
}

pub struct BasicAuth;
impl Plugin for BasicAuth {
    fn name(&self) -> &'static str {
        "basic_auth"
    }
    fn phase(&self) -> Phase {
        Phase::Pre
    }
    fn apply(&self, ctx: &BullGContext, cfg: &serde_json::Value) -> Result<()> {
        let expected_user = cfg
            .get("user")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let expected_pass = cfg
            .get("pass")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if expected_user.is_empty() {
            return Ok(());
        }
        if let Some(auth) = ctx.header_get("authorization") {
            if let Some(b64) = auth.strip_prefix("Basic ") {
                if let Ok(bytes) = STANDARD.decode(b64) {
                    if let Ok(s) = String::from_utf8(bytes) {
                        let mut parts = s.splitn(2, ':');
                        let u = parts.next().unwrap_or("");
                        let p = parts.next().unwrap_or("");
                        if u == expected_user && p == expected_pass {
                            return Ok(());
                        }
                    }
                }
            }
        }
        ctx.set_status(StatusCode::UNAUTHORIZED);
        ctx.set_body(
            Bytes::from(
                cfg
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unauthorized request: Failed")
                    .as_bytes()
                    .to_vec()
            )
        );
        Ok(())
    }
}

pub struct SecurityHeadersPlugin;

impl Plugin for SecurityHeadersPlugin {
    fn name(&self) -> &'static str {
        "security_headers"
    }
    fn phase(&self) -> Phase {
        Phase::Post
    }

    fn apply(&self, ctx: &BullGContext, _config: &serde_json::Value) -> Result<()> {
        ctx.headers.write().insert("x-content-type-options", "nosniff".parse().unwrap());
        ctx.headers.write().insert("x-frame-options", "DENY".parse().unwrap());
        ctx.headers.write().insert(
            "strict-transport-security",
            "max-age=63072000; includeSubDomains; preload".parse().unwrap()
        );
        Ok(())
    }
}

// impl Plugin for LoggingPlugin {
//     fn name(&self) -> &str {
//         "logging"
//     }
//     fn phase(&self) -> Phase {
//         Phase::Pre
//     }

//     fn apply(&self, ctx: &BullGContext, _config: &serde_json::Value) -> PluginResult {
//         //let start = Instant::now();
//         //println!("[{}] {} {}", ctx.request_id(), ctx.method(), ctx.uri().path());
//         //ctx.set_var("start_time", start.elapsed().as_nanos());
//         Ok(())
//     }
// }

pub fn builtin() -> Vec<Box<dyn Plugin>> {
    vec![
        Box::new(Cors),
        Box::new(RequestTermination),
        Box::new(HttpLog),
        Box::new(BasicAuth),
        Box::new(SecurityHeadersPlugin),
       // Box::new(LoggingPlugin),
    ]
}
