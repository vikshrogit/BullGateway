use anyhow::Result;
use bullg_core::{load_all, Memory, ToServicesMapperVec, BullGRouter};
use clap::Parser;
use tokio::time::Instant;

#[derive(Parser, Debug)]
#[command(version, about = "BullG — 10x Faster API & AI Gateway")]
struct Args {
    /// Path to config file (yaml/json/toml)
    #[arg(short, long, default_value = "./config.yaml")]
    config: String,
    #[arg(short, long, default_value = "")]
    services: String,
    #[arg(short, long, default_value = "")]
    plugins: String,
    #[arg(long, default_value = "")]
    consumers: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = load_all(&args.config, &args.plugins, &args.consumers, &args.services);
    //println!("{:#?}", config);
    let _memory = if config.config.gateway.memory.engine == "lmdb" {
        Memory::open_lmdb(&config.config.gateway.memory.path)?
    } else {
        Memory::memory()
    };

    // let mut start = Instant::now();
    // let servicemaps = config.services.get_services_map_vec().services;
    // println!("Service Map Length: {}", servicemaps.len());
    // let mut router = BullGRouter::new();
    // let _ = router.add_service_mapper(servicemaps);
    // 
    // println!("Service mappers loaded in {}ms", start.elapsed().as_millis());
    // start = Instant::now();
    // let mut r_path = "/dummy/test/users";
    // if let Some((svc, params)) = router.find_service(r_path) {
    //     println!("Services: {:?}", svc.context_paths);
    //     let mut sub_path = r_path.trim_start_matches(&svc.context_paths.paths[0].path);
    //     if !sub_path.starts_with("/") { sub_path = format!("/{}", sub_path).leak(); }
    //     if let Some(route) = svc.router.find_route(sub_path){
    //         println!("Route: {:?}", route.config);
    //     }
    //     println!("Parameters: {:?}", params);
    // }
    // 
    // println!("Service mappers loaded in {}ms", start.elapsed().as_millis());

    Ok(())
}
