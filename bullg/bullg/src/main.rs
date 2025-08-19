use anyhow::Result;
use bullg_config::{load_config, to_state};
use bullg_control_sync::SyncClient;
use bullg_gateway::Gateway;
use bullg_memory::Store;
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
//use tokio::sync::RwLock;
use tracing::info;
use bullg_crypto::BullGCrypto;

#[derive(Parser, Debug)]
#[command(version, about="BullG — 10x Faster API & AI Gateway")]
struct Args {
    /// Path to config file (yaml/json/toml)
    #[arg(short, long, default_value = "./config.yaml")]
    config: String,
    #[arg(short, long, default_value = "")]
    service: String,
    #[arg(short, long, default_value = "")]
    plugins: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    info!("BullG Gateway starting with args: {:?}", args);
    let file = load_config(&args.config)?;

    // tracing
    bullg_tracing::init(
        &file.tracing.service_name,
        Some(&file.tracing.otlp_endpoint),
        &file.gateway.logging_mode,
    )?;

    // memory store
    let store = if file.memory.engine == "lmdb" {
        Store::open_lmdb(&file.memory.path)?
    } else {
        Store::memory()
    };

    // gateway wrapped in Arc for shared ownership
    let gw = Arc::new(Gateway::new(store));

    // initial state update (requires mutability, so lock or interior mutability inside Gateway)
    gw.update_state(to_state(&file)).await;

    let bcrypt = BullGCrypto::new(env!("CARGO_PKG_NAME"),env!("CARGO_PKG_VERSION"));

    // control-plane sync
    let sync = SyncClient::new(
        file.controlplane.url.clone(),
        file.controlplane.https_fallback_url.clone(),
        file.controlplane.id.clone(),
        bcrypt,
    );
    let gw2 = gw.clone();
    tokio::spawn(async move {
        sync.run(move |state| {
            let gw2 = gw2.clone();
            tokio::spawn(async move {
                gw2.update_state(state).await;
            });
        })
        .await;
    });

    // bind and serve without locks
    let addr: SocketAddr = format!("{}:{}", file.gateway.host, file.gateway.port).parse()?;
    let gw_for_serve = gw.clone();
    let server_task = tokio::spawn(async move {
        let _ = gw_for_serve.serve(addr).await;
    });

    tokio::select! {
        _ = server_task => {},
        _ = signal::ctrl_c() => { info!("Shutting down") }
    }

    Ok(())
}
