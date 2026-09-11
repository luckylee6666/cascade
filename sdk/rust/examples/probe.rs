//! Live probe: cargo run -p cascade-sdk --example probe -- <cascade-link>

use cascade_sdk::CascadeClient;

fn main() -> anyhow::Result<()> {
    let url = std::env::args()
        .nth(1)
        .expect("usage: probe <cascade://...>");
    let cc = CascadeClient::from_url(&url)?;

    println!("get database.host = {:?}", cc.get("database.host")?);
    println!("list = {} entries", cc.list()?.len());

    let handle = cc.watch(|e| println!("SSE: {e}"));
    std::thread::sleep(std::time::Duration::from_millis(500));
    cc.set("rust.probe", "1", false)?;
    std::thread::sleep(std::time::Duration::from_secs(2));
    drop(handle);

    cc.export_env("/tmp/cc-e2e/rust-sdk.env")?;
    let exported = std::fs::read_to_string("/tmp/cc-e2e/rust-sdk.env")?;
    println!("export ok, first line: {:?}", exported.lines().next());
    Ok(())
}
