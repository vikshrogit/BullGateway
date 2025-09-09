// Here we will implement Gateway Cache support, allowing us to store and retrieve service configurations efficiently.
// This will involve converting our existing data structures into formats compatible with the cache system.

use std::{net::SocketAddr, path::Path, sync::Arc, time::Duration, };
use std::fs::File;
use std::io::BufReader;
use tokio::{net::TcpListener, sync::{broadcast, watch, RwLock}};
use parking_lot::RwLock as ParkingRwLock;
use tokio::task::JoinHandle;
use tracing::{error, info};
use rustls::{ServerConfig};
use tokio_rustls::{rustls, TlsAcceptor, TlsConnector};
use rustls_pemfile::{certs, read_one, Item};
use rustls::pki_types::{CertificateDer,PrivateKeyDer};
use crate::{BullGRouter, ToServicesMapperVec, RuntimeSnapshot, GlobalApplied, Memory, GatewayNode, Service, ToServiceMapper};
use anyhow::{Result, Context};


pub fn make_tls_config(cert_path: &str, key_path: &str) -> Result<ServerConfig> {
    // --- Load certificates ---
    let cert_file = File::open(cert_path)
        .with_context(|| format!("cannot open certificate file: {}", cert_path))?;
    let mut cert_reader = BufReader::new(cert_file);

    let certs: Vec<CertificateDer<'static>> = certs(&mut cert_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse certificates from: {}", cert_path))?;

    // --- Load private key ---
    let key_file = File::open(key_path)
        .with_context(|| format!("cannot open private key file: {}", key_path))?;
    let mut key_reader = BufReader::new(key_file);

    let key = loop {
        match read_one(&mut key_reader)
            .with_context(|| format!("failed to parse key file: {}", key_path))?
        {
            Some(Item::Pkcs8Key(k)) => break PrivateKeyDer::Pkcs8(k),
            Some(Item::Pkcs1Key(k)) => break PrivateKeyDer::Pkcs1(k),
            Some(Item::Sec1Key(k))  => break PrivateKeyDer::Sec1(k),
            Some(_) => continue, // skip unrelated PEM blocks
            None => anyhow::bail!("no keys found in {}", key_path),
        }
    };

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("invalid certificate/key pair")?;

    Ok(config)
}


struct BullConfig {
    gateway: GatewayNode,
    router: RwLock<BullGRouter>,
    global: Arc<GlobalApplied>,
    snapshots: Arc<RuntimeSnapshot>,
}
#[derive(Clone)]
pub struct BullG {
    config: Arc<RwLock<BullConfig>>,
    memory: Arc<Memory>,
    // channel to broadcast shutdown
    shutdown_tx: broadcast::Sender<()>,
    // for notifying background tasks of config changes
    cfg_tx: watch::Sender<RuntimeSnapshot>,
    // store join handles if we want to await them
    handles: Arc<RwLock<Vec<JoinHandle<()>>>>,
}

impl BullG {
    /// Create a new BullG instance with initial configuration and a router.
    pub async fn new(snapshot:RuntimeSnapshot,memory:Memory) -> anyhow::Result<Self> {
        // create router and initialize from config
        let gateway = snapshot.config.gateway.clone();
        let global = Arc::new(snapshot.services.global.clone());
        let mut router = BullGRouter::new();
        let _ =router.add_service_mapper(snapshot.services.get_services_map_vec().services);
        let router = RwLock::new(router);
        let memory = Arc::new(memory);
        let snapshots = Arc::new(snapshot.clone());
        let (shutdown_tx, _) = broadcast::channel(1);
        let (cfg_tx, _) = watch::channel(snapshot);
        let config = Arc::new(RwLock::new(BullConfig{
            gateway,
            router,
            global,
            snapshots,
        }));

        Ok(Self {
            config,
            memory,
            shutdown_tx,
            cfg_tx,
            handles: Arc::new(RwLock::new(Vec::new())),
        })
    }

    // fn make_tls_config(&self, cert_path: &str, key_path: &str) -> anyhow::Result<RustlsServerConfig> {
    //     // Load certificates
    //     let mut cert_reader = BufReader::new(File::open(cert_path)?);
    //     let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
    //         .collect::<Result<_, _>>()?;
    //
    //     // Load private keys
    //     let mut key_reader = BufReader::new(File::open(key_path)?);
    //     let mut keys: Vec<PrivateKeyDer<'static>> = Vec::new();
    //
    //     for item in rustls_pemfile::read_all(&mut key_reader) {
    //         match item {
    //             Item::Pkcs8Key(key) => keys.push(PrivateKeyDer::from(key)),
    //             Item::RsaKey(key) => keys.push(PrivateKeyDer::from(key)),
    //             Item::EcKey(key) => keys.push(PrivateKeyDer::from(key)),
    //             _ => {}
    //         }
    //     }
    //
    //     let cert_chain = certs;
    //     let priv_key = keys
    //         .into_iter()
    //         .next()
    //         .ok_or_else(|| anyhow::anyhow!("no private key found"))?;
    //
    //     let config = RustlsServerConfig::builder()
    //         .with_no_client_auth()
    //         .with_single_cert(cert_chain, priv_key)?;
    //
    //     Ok(config)
    // }

    /// Start background listeners for all configured protocols.
    /// This spawns tasks and returns immediately.
    pub async fn start(&self) -> anyhow::Result<()> {
        let config = self.config.read().await;
        let gcfg = config.gateway.clone();
        drop(config);

        let address = format!("{}:{}", gcfg.host, gcfg.port);
        if gcfg.ssl {
           let  tls_acceptor = TlsAcceptor::from(Arc::new(make_tls_config(gcfg.cert.as_str(),gcfg.key.as_str())?));
        }else {
            let  tls_connector: std::option::Option<TlsAcceptor> = None;
        }

        // spawn HTTP server if configured
        // if let Some(http_addr) = cfg.http_addr {
        //     let handle = self.spawn_http_server(http_addr).await?;
        //     self.handles.write().await.push(handle);
        // }
        //
        // // spawn WS server if configured
        // if let Some(ws_addr) = cfg.ws_addr {
        //     let handle = self.spawn_ws_server(ws_addr).await?;
        //     self.handles.write().await.push(handle);
        // }
        //
        // // spawn gRPC server if configured
        // if let Some(grpc_addr) = cfg.grpc_addr {
        //     let handle = self.spawn_grpc_server(grpc_addr).await?;
        //     self.handles.write().await.push(handle);
        // }
        //
        // // spawn TCP server if configured
        // if let Some(tcp_addr) = cfg.tcp_addr {
        //     let handle = self.spawn_tcp_server(tcp_addr).await?;
        //     self.handles.write().await.push(handle);
        // }

        // http3/quic: placeholder (requires quinn + setup). Add spawn_http3_server similarly.

        Ok(())
    }

    /// Atomically update the configuration. The router is updated and the new config
    /// is broadcast to running listeners (they should watch and reload as needed).
    pub async fn update_config(&self, snapshot: RuntimeSnapshot) -> anyhow::Result<()> {
        // update router
        {
            let mut config = self.config.write().await;
            config.gateway = snapshot.config.gateway.clone();
            {
                let mut router = config.router.write().await;
                router.add_service_mapper(snapshot.services.get_services_map_vec().services)?;
            }
            config.global = Arc::new(snapshot.services.global.clone());
            config.snapshots = Arc::new(snapshot.clone());
        }

        // notify watchers
        let _ = self.cfg_tx.send(snapshot);

        info!("BullG configuration updated and broadcasted");
        Ok(())
    }

    pub async fn update_service(&self, service: Service) -> anyhow::Result<()> {
        {
            let mapper = service.get_service_maps();
            let config = self.config.read().await;
            let mut router = config.router.write().await;
            let _ = router.update_service_mappers(mapper);
        }

        Ok(())
    }

    /// Graceful shutdown: broadcast shutdown signal and await background tasks.
    pub async fn shutdown(&self) {
        info!("BullG initiating shutdown");
        // ignore errors if there are no receivers
        let _ = self.shutdown_tx.send(());

        // wait for join handles
        let mut handles = self.handles.write().await;
        while let Some(h) = handles.pop() {
            match h.await {
                Ok(_) => {}
                Err(e) => error!("Task join error: {:?}", e),
            }
        }
        info!("BullG shutdown complete");
    }

    /// Helper to get current router (shared)
    pub async fn router(&self) -> BullGRouter {
        let router = {
            let cfg_guard = self.config.read().await; // or self.inner.read().await depending on your naming
            cfg_guard.router.read().await.clone()
        }; // cfg_guard dropped here
        router
    }



}


// helper removed after final version

// /// Spawn an HTTP server (HTTP/1 and HTTP/2 via hyper)
// async fn spawn_http_server(&self, addr: SocketAddr) -> anyhow::Result<JoinHandle<()>> {
//     use hyper::service::{service_fn};
//     use hyper::{Body, Request, Response, Server};
//
//     let router = self.router().await;
//     let mut cfg_rx = self.cfg_tx.subscribe();
//     let mut shutdown_rx = self.shutdown_tx.subscribe();
//
//     let make_svc = make_service_fn(move |_conn| {
//         let router = router.clone();
//         async move {
//             Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| {
//                 let router = router.clone();
//                 async move {
//                     // delegate to router — assumes a synchronous return; adapt if async
//                     let resp = router.route_http(req).await;
//                     Ok::<_, hyper::Error>(resp)
//                 }
//             }))
//         }
//     });
//
//     let server = Server::bind(&addr).http2_only(false).serve(make_svc);
//     info!("HTTP server listening on {}", addr);
//
//     // spawn task that reacts to config updates or shutdown; currently we just run the server
//     let handle = tokio::spawn(async move {
//         tokio::select! {
//                 res = server => {
//                     if let Err(e) = res {
//                         error!("HTTP server error: {:?}", e);
//                     }
//                 }
//                 _ = shutdown_rx.recv() => {
//                     info!("HTTP server received shutdown");
//                 }
//             }
//
//         // If config changes require different listen address, you'd need to detect it via cfg_rx,
//         // shutdown and respawn a server bound to the new address. Keeping this simple:
//         while cfg_rx.changed().await.is_ok() {
//             // For more advanced usage: if address changed, break and allow respawn logic in BullG.start/update.
//             // For now, router is already updated via BullG::update_config.
//             info!("HTTP server observed config change (router already updated)");
//         }
//     });
//
//     Ok(handle)
// }
//
// /// Spawn a websocket server endpoint (upgrade using hyper + tokio-tungstenite)
// async fn spawn_ws_server(&self, addr: SocketAddr) -> anyhow::Result<JoinHandle<()>> {
//     use hyper::service::{make_service_fn, service_fn};
//     use hyper::{Body, Request, Response, Server, StatusCode};
//     use tokio_tungstenite::tungstenite::protocol::Message;
//     use tokio_tungstenite::WebSocketStream;
//     use tokio_tungstenite::tungstenite::Error as WsError;
//     use tokio::net::TcpStream;
//     use hyper::upgrade::Upgraded;
//     use futures::{StreamExt, SinkExt};
//
//     let router = self.router().await;
//     let mut shutdown_rx = self.shutdown_tx.subscribe();
//
//     let make_svc = make_service_fn(move |_conn| {
//         let router = router.clone();
//         async move {
//             Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| {
//                 let router = router.clone();
//                 async move {
//                     // naive path check for websocket upgrade
//                     if hyper_tungstenite::is_upgrade_request(&req) {
//                         // accept and upgrade
//                         let (response, upgrade_fut) = hyper_tungstenite::upgrade(&req, None).unwrap();
//                         // spawn a task to handle the websocket after upgrade completes
//                         tokio::spawn(async move {
//                             match upgrade_fut.await {
//                                 Ok(ws) => {
//                                     // ws is a WebSocketStream<Upgraded>
//                                     if let Err(e) = handle_ws_stream(ws, router).await {
//                                         error!("websocket handler err: {:?}", e);
//                                     }
//                                 }
//                                 Err(e) => {
//                                     error!("upgrade error: {:?}", e);
//                                 }
//                             }
//                         });
//
//                         Ok::<_, hyper::Error>(response)
//                     } else {
//                         // non-websocket; reply 400
//                         Ok::<_, hyper::Error>(Response::builder()
//                             .status(StatusCode::BAD_REQUEST)
//                             .body(Body::from("This endpoint only supports websockets"))
//                             .unwrap())
//                     }
//                 }
//             }))
//         }
//     });
//
//     let server = Server::bind(&addr).serve(make_svc);
//     info!("WebSocket server listening on {}", addr);
//
//     let handle = tokio::spawn(async move {
//         tokio::select! {
//                 res = server => {
//                     if let Err(e) = res {
//                         error!("WS server error: {:?}", e);
//                     }
//                 }
//                 _ = shutdown_rx.recv() => {
//                     info!("WS server received shutdown");
//                 }
//             }
//     });
//
//     Ok(handle)
// }
//
// /// Spawn a gRPC server using tonic (basic skeleton)
// async fn spawn_grpc_server(&self, addr: SocketAddr) -> anyhow::Result<JoinHandle<()>> {
//     // NOTE: Actual tonic service implementations must be added.
//     // We present skeleton code that starts a tonic server and can be extended.
//     use tonic::transport::Server;
//
//     let router = self.router().await;
//     let mut shutdown_rx = self.shutdown_tx.subscribe();
//
//     // Here you would build your tonic svc: e.g.
//     // let my_service = MyGrpcService::new(router.clone());
//     // Server::builder().add_service(MyGrpcServer::new(my_service)).serve(addr).await
//
//     let handle = tokio::spawn(async move {
//         info!("gRPC server listening on {}", addr);
//
//         // placeholder: sleep until shutdown
//         tokio::select! {
//                 _ = shutdown_rx.recv() => {
//                     info!("gRPC server received shutdown");
//                 }
//                 // If you want to run tonic server here, replace the following with tonic::Server::builder()...await
//                 _ = tokio::time::sleep(tokio::time::Duration::from_secs(24 * 60 * 60)) => {
//                     // long-running placeholder
//                 }
//             }
//     });
//
//     Ok(handle)
// }
//
// /// Spawn a raw TCP listener that delegates to router for CRUD/payload handling
// async fn spawn_tcp_server(&self, addr: SocketAddr) -> anyhow::Result<JoinHandle<()>> {
//     use tokio::io::{AsyncReadExt, AsyncWriteExt};
//     use tokio::net::TcpListener;
//
//     let router = self.router().await;
//     let mut shutdown_rx = self.shutdown_tx.subscribe();
//
//     let listener = TcpListener::bind(addr).await?;
//     info!("TCP server listening on {}", addr);
//
//     let handle = tokio::spawn(async move {
//         loop {
//             tokio::select! {
//                     accept = listener.accept() => {
//                         match accept {
//                             Ok((mut socket, peer)) => {
//                                 let router = router.clone();
//                                 tokio::spawn(async move {
//                                     let mut buf = [0u8; 4096];
//                                     loop {
//                                         match socket.read(&mut buf).await {
//                                             Ok(0) => break, // closed
//                                             Ok(n) => {
//                                                 // simple echo or delegate to router
//                                                 // In practice, parse frames and pass to router
//                                                 // e.g., router.handle_tcp(&buf[..n], peer).await;
//                                                 if let Err(e) = socket.write_all(&buf[..n]).await {
//                                                     error!("error writing to socket: {:?}", e);
//                                                     break;
//                                                 }
//                                             }
//                                             Err(e) => {
//                                                 error!("tcp read error: {:?}", e);
//                                                 break;
//                                             }
//                                         }
//                                     }
//                                 });
//                             }
//                             Err(e) => error!("accept error: {:?}", e),
//                         }
//                     }
//                     _ = shutdown_rx.recv() => {
//                         info!("TCP server received shutdown");
//                         break;
//                     }
//                 }
//         }
//     });
//
//     Ok(handle)
// }