use serde::Serialize;
use std::sync::Mutex;

/// Display path of the active vault (home replaced with ~).
#[tauri::command(rename_all = "snake_case")]
pub fn vault_path() -> Result<String, String> {
    let path = cc_store::db::Store::vault_path().map_err(|e| e.to_string())?;
    let mut display = path.display().to_string();
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        if let Some(rest) = display.strip_prefix(&home_str) {
            display = format!("~{}", rest);
        }
    }
    Ok(display)
}

#[derive(Serialize)]
pub struct ToolchainPaths {
    pub cli: String,
    pub sdk_python: Option<String>,
    pub sdk_typescript: Option<String>,
    pub sdk_go: Option<String>,
}

/// Locate the sibling `cascade` binary and the repo's SDK folders so the
/// "hand it to your AI" prompt can contain real local paths.
#[tauri::command(rename_all = "snake_case")]
pub fn toolchain_paths() -> ToolchainPaths {
    let exe = std::env::current_exe().ok();

    // Bundle sidecar is named `cascade-cli` (a bare `cascade` would case-collide
    // with the main `Cascade` binary on macOS/Windows file systems); PATH
    // installs of the CLI use `cascade`.
    let cli = exe
        .as_ref()
        .and_then(|p| p.parent())
        .and_then(|dir| {
            ["cascade-cli", "cascade"]
                .iter()
                .map(|name| dir.join(name))
                .find(|candidate| candidate.exists())
        })
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "cascade".to_string());

    // target/{debug,release}/cc-desktop -> repo root -> sdk/
    let repo_sdk = exe
        .as_ref()
        .and_then(|p| p.parent().and_then(|d| d.parent()).and_then(|d| d.parent()))
        .map(|repo| repo.join("sdk"))
        .filter(|sdk| sdk.exists());

    let pick = |name: &str, marker: &str| -> Option<String> {
        repo_sdk
            .as_ref()
            .map(|s| s.join(name))
            .filter(|p| p.join(marker).exists())
            .map(|p| p.display().to_string())
    };

    ToolchainPaths {
        cli,
        sdk_python: pick("python", "pyproject.toml"),
        sdk_typescript: pick("typescript", "package.json"),
        sdk_go: pick("go", "go.mod"),
    }
}

/* ── LAN sharing ───────────────────────────────────────────────────── */

#[derive(Debug, Clone, Serialize)]
pub struct SharePayload {
    /// "local" | "lan"
    pub mode: String,
    pub port: u16,
    /// http base URL reachable by clients of this share.
    pub url: String,
    /// Access token (LAN shares only; local shares are loopback-open).
    pub token: Option<String>,
    pub lan: bool,
}

pub struct ShareState(pub Mutex<Option<(std::process::Child, SharePayload)>>);

impl Default for ShareState {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

/// Best-effort LAN IP via the UDP-connect trick (no packets are sent).
fn lan_ip() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    Some(sock.local_addr().ok()?.ip().to_string())
}

fn pick_port(start: u16) -> Result<u16, String> {
    for port in start..start.saturating_add(20) {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    Err(format!("no free port in {}..{}", start, start + 20))
}

fn server_binary() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join(format!("cascade-server{}", std::env::consts::EXE_SUFFIX));
            if cand.exists() {
                return cand;
            }
        }
    }
    std::path::PathBuf::from("cascade-server")
}

fn spawn_server(port: u16, lan: bool, token: Option<&str>) -> Result<std::process::Child, String> {
    let log_path = dirs::home_dir()
        .map(|h| h.join(".cascade").join("server.log"))
        .ok_or_else(|| "no home dir".to_string())?;
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| e.to_string())?;

    let mut cmd = std::process::Command::new(server_binary());
    cmd.arg("--port")
        .arg(port.to_string())
        .arg("--listen")
        .arg(if lan { "0.0.0.0" } else { "127.0.0.1" });
    if let Some(tok) = token {
        cmd.arg("--token").arg(tok);
    }
    cmd.stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);

    let child = cmd.spawn().map_err(|e| {
        format!("failed to launch cascade-server: {} (see {})", e, log_path.display())
    })?;
    std::thread::sleep(std::time::Duration::from_millis(700));
    Ok(child)
}

/// Kill the share server and revoke its token (tokens must not outlive the
/// share — otherwise a captured link keeps working against future servers).
pub fn kill_share_with_store(state: &ShareState, store: Option<&cc_store::db::Store>) {
    if let Ok(mut guard) = state.0.lock() {
        if let Some((mut child, payload)) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
            if let (Some(store), Some(token)) = (store, payload.token.as_deref()) {
                let _ = cc_store::token_repo::TokenRepo::new(store).revoke(token);
            }
        }
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn share_status(state: tauri::State<'_, ShareState>) -> Option<SharePayload> {
    let mut guard = state.0.lock().ok()?;
    if let Some((child, payload)) = guard.as_mut() {
        match child.try_wait() {
            Ok(Some(_)) => {
                // Process died; report as stopped.
                *guard = None;
                None
            }
            _ => Some(payload.clone()),
        }
    } else {
        None
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn share_start(
    store: tauri::State<'_, cc_store::db::Store>,
    state: tauri::State<'_, ShareState>,
    mode: String,
    allow_reveal: bool,
) -> Result<SharePayload, String> {
    kill_share_with_store(&state, Some(store.inner())); // restart if already running

    let lan = mode == "lan";
    let port = pick_port(7070)?;

    let token = if lan {
        let tok = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let permissions = if allow_reveal { "admin" } else { "read" };
        cc_store::token_repo::TokenRepo::new(&store)
            .ensure(&tok, permissions)
            .map_err(|e| e.to_string())?;
        Some(tok)
    } else {
        None
    };

    let mut child = match spawn_server(port, lan, token.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            if let Some(tok) = token.as_deref() {
                let _ = cc_store::token_repo::TokenRepo::new(&store).revoke(tok);
            }
            return Err(e);
        }
    };
    if let Ok(Some(status)) = child.try_wait() {
        if let Some(tok) = token.as_deref() {
            let _ = cc_store::token_repo::TokenRepo::new(&store).revoke(tok);
        }
        return Err(format!(
            "cascade-server exited immediately ({}). See ~/.cascade/server.log",
            status
        ));
    }

    let url = if lan {
        let ip = lan_ip().ok_or_else(|| {
            // Revoke before bailing so a dead share leaves no valid token.
            if let Some(tok) = token.as_deref() {
                let _ = cc_store::token_repo::TokenRepo::new(&store).revoke(tok);
            }
            "could not detect a LAN IP — check your network connection".to_string()
        })?;
        format!("http://{}:{}", ip, port)
    } else {
        format!("http://localhost:{}", port)
    };

    let payload = SharePayload {
        mode: if lan { "lan".into() } else { "local".into() },
        port,
        url,
        token,
        lan,
    };
    *state.0.lock().map_err(|_| "state lock poisoned".to_string())? =
        Some((child, payload.clone()));
    Ok(payload)
}

#[tauri::command(rename_all = "snake_case")]
pub fn share_stop(
    store: tauri::State<'_, cc_store::db::Store>,
    state: tauri::State<'_, ShareState>,
) -> Result<(), String> {
    kill_share_with_store(&state, Some(store.inner()));
    Ok(())
}
