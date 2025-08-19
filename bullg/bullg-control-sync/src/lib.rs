
use anyhow::Result;
//use futures_util::{StreamExt, SinkExt};
use bullg_core::GatewayState;
use moka::sync::Cache;
use reqwest::Client;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::connect_async;
use tracing::{error, info};
//use core::slice::SlicePattern;
use bullg_crypto::BullGCrypto;

pub struct SyncClient {
    ws_url: String,
    https_url: String,
    cp_id: String,
    client: Client,
    token_cache: Cache<&'static str, (String, i64)>,
    bcrypt: BullGCrypto,
}

impl SyncClient {
    pub fn new(ws_url: String, https_url: String, cp_id: String, bcrypt: BullGCrypto) -> Self {
        Self {
            ws_url,
            https_url,
            cp_id,
            client: Client::new(),
            token_cache: Cache::new(10),
            bcrypt: bcrypt,
        }
    }

    pub async fn run<F>(&self, on_state: F)
    where
        F: Fn(GatewayState) + Send + Sync + 'static + Clone,
    {
        // Prefer websocket; on failure, fallback to polling HTTPS every 5s
        loop {
            match self.try_ws(on_state.clone()).await {
                Ok(_) => {}
                Err(e) => {
                    error!("WS sync failed: {e}. Falling back to HTTPS polling");
                    self.poll_https(on_state.clone()).await;
                }
            }
            sleep(Duration::from_secs(1)).await;
        }
    }

    async fn try_ws<F>(&self, on_state: F) -> Result<()>
    where
        F: Fn(GatewayState) + Send + Sync + 'static + Clone,
    {
        let (ws, _resp) = connect_async(&self.ws_url).await?;
        info!("WS connected to control-plane");
        let (_write, mut read) = ws.split();
        use futures_util::StreamExt;
        while let Some(msg) = read.next().await {
            let msg = msg?;
            if msg.is_binary() {
                let decrypted = bullg_utils::custom_decrypt(msg.into_data().as_ref())?;
                let state: GatewayState = serde_json::from_slice(&decrypted)?;
                on_state(state);
            }
        }
        Ok(())
    }

    async fn poll_https<F>(&self, on_state: F)
    where
        F: Fn(GatewayState) + Send + Sync + 'static + Clone,
    {
        loop {
            match self.pull_once().await {
                Ok(state) => on_state(state),
                Err(e) => error!("HTTPS pull failed: {e}"),
            }
            sleep(Duration::from_secs(5)).await;
        }
    }

    async fn pull_once(&self) -> Result<GatewayState> {
        let token = if let Some((t, _exp)) = self.token_cache.get("token") {
            // TODO check exp refresh; simplified here
            t
        } else {
            let t = self
                .client
                .post(format!("{}/token", self.https_url))
                .json(&serde_json::json!({
                    "id": self.cp_id,
                    "pub": "public-key-here"
                }))
                .send()
                .await?
                .text()
                .await?;
            self.token_cache.insert("token", (t.clone(), 0));
            t
        };
        let bytes = self
            .client
            .get(format!("{}/state", self.https_url))
            .bearer_auth(token)
            .send()
            .await?
            .bytes()
            .await?;
        let decrypted = bullg_utils::custom_decrypt(&bytes)?;
        Ok(serde_json::from_slice(&decrypted)?)
    }
}
