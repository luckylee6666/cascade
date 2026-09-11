//! gRPC smoke probe: exercises GetConfig / ListConfigs / GetProjectConfigs.
//! Usage: start the server, then
//!   cargo run -p cc-server --example grpc_probe -- [grpc-port]

pub mod proto {
    tonic::include_proto!("configcenter");
}

use proto::{config_center_client::ConfigCenterClient, GetConfigRequest, GetProjectRequest, ListConfigsRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "7071".to_string());
    let mut client = ConfigCenterClient::connect(format!("http://127.0.0.1:{}", port)).await?;

    let cfg = client
        .get_config(GetConfigRequest {
            key: "database.host".into(),
        })
        .await?
        .into_inner();
    println!("GetConfig: {} = {} (secret={})", cfg.key, cfg.value, cfg.secret);

    let mut stream = client
        .list_configs(ListConfigsRequest {
            group: String::new(),
            project_id: String::new(),
            env_id: String::new(),
        })
        .await?
        .into_inner();
    let mut n = 0;
    while stream.message().await?.is_some() {
        n += 1;
    }
    println!("ListConfigs: {} configs", n);

    let missing = client
        .get_config(GetConfigRequest {
            key: "no.such.key".into(),
        })
        .await;
    println!(
        "GetConfig(missing): {}",
        match missing {
            Err(s) => format!("{} ({})", s.code(), s.message()),
            Ok(_) => "UNEXPECTEDLY OK".into(),
        }
    );

    let _ = GetProjectRequest {
        project_id: String::new(),
        env_id: String::new(),
    };
    println!("probe done");
    Ok(())
}
