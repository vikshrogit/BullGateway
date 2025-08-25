use std::env;
use std::fs;

fn main() {
    // Read workspace metadata from workspace Cargo.toml
    let workspace_manifest_path = "../../Cargo.toml"; // relative to this crate
    let manifest_content = fs::read_to_string(workspace_manifest_path).expect("Workspace Cargo.toml not found");
    let app_name = manifest_content
        .lines()
        .find(|l| l.trim_start().starts_with("name"))
        .map(|l| l.split('=').nth(1).unwrap().trim().trim_matches('"'))
        .unwrap_or(env!("CARGO_PKG_NAME"));

    println!("cargo:rustc-env=APP_NAME={}", app_name);
}
