
use reqwest::Client;
use tracing::info;

pub async fn send(endpoint: &str, payload: serde_json::Value) {
    let client = Client::new();
    let _ = client.post(endpoint).json(&payload).send().await;
    info!("log sent");
}
