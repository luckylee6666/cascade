use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;
use std::process::Command;

pub async fn execute(
    port: u16,
    open: bool,
    listen: &str,
    token: Option<&str>,
    readonly: bool,
) -> Result<()> {
    let bin = server_binary()?;
    let mut cmd = Command::new(&bin);
    cmd.arg("--port")
        .arg(port.to_string())
        .arg("--listen")
        .arg(listen);
    if let Some(t) = token {
        cmd.arg("--token").arg(t);
    }
    if readonly {
        cmd.arg("--readonly");
    }

    if open {
        let url = format!("http://localhost:{}/", port);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(800));
            open_browser(&url);
        });
    }

    println!(
        "{}",
        format!("Starting {} --port {} --listen {}", bin.display(), port, listen).green()
    );
    let status = cmd.status().map_err(|e| {
        anyhow::anyhow!(
            "failed to launch {}: {} (build it with: cargo build -p cc-server)",
            bin.display(),
            e
        )
    })?;
    if !status.success() {
        anyhow::bail!("cascade-server exited with {}", status);
    }
    Ok(())
}

/// Prefer the cascade-server next to the running binary, else PATH.
/// Falls back to any legacy `cc-server` for older installs.
fn server_binary() -> Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["cascade-server", "cc-server"] {
                let cand = dir.join(format!("{}{}", name, std::env::consts::EXE_SUFFIX));
                if cand.exists() {
                    return Ok(cand);
                }
            }
        }
    }
    Ok(PathBuf::from("cascade-server"))
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).status();
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(url).status();
    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd").args(["/C", "start", url]).status();
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let _ = url;
}
