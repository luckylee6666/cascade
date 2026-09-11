//! Cascade SDK — blocking HTTP + SSE client with snapshot fallback.
//!
//! ```no_run
//! use cascade_sdk::CascadeClient;
//!
//! # fn main() -> anyhow::Result<()> {
//! let cc = CascadeClient::from_url(
//!     "cascade://localhost:7070/project/<id>?env=prod&token=xxx",
//! )?;
//!
//! println!("{}", cc.get("database.host")?.unwrap_or_default());
//!
//! // Change subscription (SSE, auto-reconnect) on a background thread.
//! let _handle = cc.watch(|event| println!("changed: {event}"));
//! # Ok(())
//! # }
//! ```
//!
//! Secrets come back as `None` unless [`CascadeClient::with_reveal`] is set
//! AND the token has admin permission. The snapshot file stores exactly what
//! the server returned.
//!
//! Cargo dependency (no crates.io release yet):
//!
//! ```toml
//! cascade-sdk = { git = "https://github.com/luckylee6666/cascade" }
//! ```

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const CACHE_TTL: Duration = Duration::from_secs(5);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);
const DEFAULT_PORT: u16 = 7070;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedConfig {
    pub id: String,
    pub key: String,
    /// `None` for secrets that were not revealed.
    pub value: Option<String>,
    pub secret: bool,
    /// Which layer produced the value: `base` / `env:<name>` / `project:<name>`.
    pub source: String,
    pub group: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Yaml,
    Json,
    Dotenv,
}

pub struct CascadeClient {
    server: String,
    project_id: String,
    token: Option<String>,
    env: Option<String>,
    reveal: bool,
    snapshot_path: PathBuf,
    cache: Mutex<Option<(Instant, Vec<ResolvedConfig>)>>,
}

impl CascadeClient {
    pub fn new(server: impl Into<String>, project_id: impl Into<String>) -> Self {
        let project_id = project_id.into();
        let snapshot_path = dirs::home_dir()
            .map(|h| h.join(".cascade").join("sdk").join(format!("{project_id}.json")))
            .unwrap_or_else(|| PathBuf::from(format!("{project_id}.json")));
        Self {
            server: server.into().trim_end_matches('/').to_string(),
            project_id,
            token: None,
            env: None,
            reveal: false,
            snapshot_path,
            cache: Mutex::new(None),
        }
    }

    /// Parse a project link.
    ///
    /// Canonical: `cascade://<host>[:port]/project/<id>?env=<name>&token=<t>[&reveal=1]`
    /// Shorthand: `cascade://project/<id>` (server defaults to `http://localhost:7070`)
    /// Legacy `cc://` links parse identically (the scheme is ignored).
    pub fn from_url(url: &str) -> Result<Self> {
        let parsed = url::Url::parse(url).with_context(|| format!("invalid url: {url}"))?;
        let mut token = None;
        let mut env = None;
        let mut reveal = false;
        for (k, v) in parsed.query_pairs() {
            match k.as_ref() {
                "token" => token = Some(v.into_owned()),
                "env" => env = Some(v.into_owned()),
                "reveal" => reveal = matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"),
                _ => {}
            }
        }

        let host = parsed.host_str().unwrap_or("");
        let (server, project_id) = if host.is_empty() || host == "project" {
            let id = parsed
                .path_segments()
                .and_then(|s| s.filter(|p| !p.is_empty()).next_back())
                .unwrap_or("");
            ("http://localhost:7070".to_string(), id.to_string())
        } else {
            let port = parsed.port().unwrap_or(DEFAULT_PORT);
            let id = parsed
                .path()
                .trim_start_matches('/')
                .trim_start_matches("project/")
                .trim_matches('/')
                .to_string();
            (format!("http://{host}:{port}"), id)
        };
        if project_id.is_empty() {
            bail!("cannot parse project id from {url:?}");
        }

        let mut client = Self::new(server, project_id);
        client.token = token;
        client.env = env;
        client.reveal = reveal;
        Ok(client)
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn with_env(mut self, env: impl Into<String>) -> Self {
        self.env = Some(env.into());
        self
    }

    /// Reveal secret values (requires an admin token on the server).
    pub fn with_reveal(mut self, reveal: bool) -> Self {
        self.reveal = reveal;
        self
    }

    pub fn with_snapshot_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.snapshot_path = path.into();
        self
    }

    // ── reads ───────────────────────────────────────────────

    /// Effective value for this project/env.
    /// `None` when the key is missing or its secret is hidden.
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        for c in self.data(false)? {
            if c.key == key {
                return Ok(c.value);
            }
        }
        Ok(None)
    }

    pub fn get_or(&self, key: &str, default: impl Into<String>) -> Result<String> {
        Ok(self.get(key)?.unwrap_or_else(|| default.into()))
    }

    /// All resolved configs (briefly cached, snapshot fallback on failure).
    pub fn list(&self) -> Result<Vec<ResolvedConfig>> {
        self.data(false)
    }

    /// Force a fetch, falling back to the snapshot when the server is down.
    pub fn refresh(&self) -> Result<Vec<ResolvedConfig>> {
        self.data(true)
    }

    // ── writes ──────────────────────────────────────────────

    /// Create or update a raw config entry (requires an admin token).
    pub fn set(&self, key: &str, value: &str, secret: bool) -> Result<()> {
        let url = format!("{}/api/configs", self.server);
        let mut req = ureq::post(&url);
        if let Some(token) = &self.token {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        req.send_json(serde_json::json!({
            "key": key,
            "value": value,
            "secret": secret,
        }))
        .with_context(|| format!("POST {url} failed"))?;
        self.invalidate_cache();
        Ok(())
    }

    // ── realtime ────────────────────────────────────────────

    /// Stream SSE change events on a background thread with auto-reconnect.
    /// The callback receives the raw event, e.g. `"config_updated:<id>"`.
    pub fn watch<F>(&self, callback: F) -> WatchHandle
    where
        F: Fn(String) + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let url = format!("{}/api/sse/configs", self.server);
        let auth = self.token.clone();
        let handle_stop = stop.clone();
        std::thread::spawn(move || {
            while !handle_stop.load(Ordering::Relaxed) {
                let mut req = ureq::get(&url);
                if let Some(token) = &auth {
                    req = req.set("Authorization", &format!("Bearer {token}"));
                }
                if let Ok(resp) = req.call() {
                    let reader = BufReader::new(resp.into_reader());
                    for line in reader.lines() {
                        if handle_stop.load(Ordering::Relaxed) {
                            return;
                        }
                        match line {
                            Ok(l) => {
                                if let Some(rest) = l.strip_prefix("data:") {
                                    callback(rest.trim().to_string());
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
                // Reconnect backoff.
                std::thread::sleep(RECONNECT_DELAY);
            }
        });
        WatchHandle { stop }
    }

    // ── exports ─────────────────────────────────────────────

    pub fn export_file(&self, path: &str, format: Format) -> Result<()> {
        let pairs = self.export_pairs()?;
        let content = match format {
            Format::Json => serde_json::to_string_pretty(&pairs)?,
            Format::Yaml => pairs
                .iter()
                .map(|(k, v)| format!("{k}: \"{}\"", v.replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join("\n"),
            Format::Dotenv => {
                let mut lines = vec!["# generated by cascade".to_string()];
                for (k, v) in &pairs {
                    let name = k.replace(['.', '-'], "_").to_ascii_uppercase();
                    let escaped = v
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"")
                        .replace('\n', "\\n");
                    lines.push(format!("{name}=\"{escaped}\""));
                }
                lines.join("\n")
            }
        };
        std::fs::write(path, content).with_context(|| format!("writing {path}"))?;
        Ok(())
    }

    pub fn export_env(&self, path: &str) -> Result<()> {
        self.export_file(path, Format::Dotenv)
    }

    /// Path of the snapshot file used as an offline fallback.
    pub fn snapshot_path(&self) -> &std::path::Path {
        &self.snapshot_path
    }

    // ── internals ───────────────────────────────────────────

    fn resolved_url(&self) -> String {
        let mut url = format!("{}/api/projects/{}/resolved", self.server, self.project_id);
        let mut params: Vec<String> = Vec::new();
        if let Some(env) = &self.env {
            params.push(format!("env={}", urlencode(env)));
        }
        if self.reveal {
            params.push("reveal=true".to_string());
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        url
    }

    fn fetch(&self) -> Result<Vec<ResolvedConfig>> {
        let url = self.resolved_url();
        let mut req = ureq::get(&url);
        if let Some(token) = &self.token {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        let data: Vec<ResolvedConfig> = req
            .call()
            .with_context(|| format!("GET {url} failed"))?
            .into_json()
            .with_context(|| format!("invalid JSON from {url}"))?;
        let _ = self.write_snapshot(&data);
        *self.cache.lock().expect("cache lock") = Some((Instant::now(), data.clone()));
        Ok(data)
    }

    fn data(&self, force: bool) -> Result<Vec<ResolvedConfig>> {
        if !force {
            if let Some((at, cached)) = self.cache.lock().expect("cache lock").as_ref() {
                if at.elapsed() < CACHE_TTL {
                    return Ok(cached.clone());
                }
            }
        }
        match self.fetch() {
            Ok(data) => Ok(data),
            Err(fetch_err) => match self.read_snapshot() {
                Some(snapshot) => {
                    *self.cache.lock().expect("cache lock") =
                        Some((Instant::now(), snapshot.clone()));
                    Ok(snapshot)
                }
                None => Err(fetch_err.context("no snapshot available for offline fallback")),
            },
        }
    }

    fn invalidate_cache(&self) {
        *self.cache.lock().expect("cache lock") = None;
    }

    fn write_snapshot(&self, data: &[ResolvedConfig]) -> Result<()> {
        if let Some(parent) = self.snapshot_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.snapshot_path, serde_json::to_vec(data)?)?;
        Ok(())
    }

    fn read_snapshot(&self) -> Option<Vec<ResolvedConfig>> {
        let bytes = std::fs::read(&self.snapshot_path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn export_pairs(&self) -> Result<HashMap<String, String>> {
        let mut out = HashMap::new();
        for c in self.refresh()? {
            let value = c.value.unwrap_or_else(|| {
                format!("${{{}}}", c.key.replace(['.', '-'], "_").to_ascii_uppercase())
            });
            out.insert(c.key, value);
        }
        Ok(out)
    }
}

/// Handle returned by [`CascadeClient::watch`]; stops the listener on drop.
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
}

impl WatchHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_url_canonical() {
        let c = CascadeClient::from_url(
            "cascade://192.168.1.23:7071/project/proj-1?env=prod&token=t0k&reveal=1",
        )
        .unwrap();
        assert_eq!(c.server, "http://192.168.1.23:7071");
        assert_eq!(c.project_id, "proj-1");
        assert_eq!(c.env.as_deref(), Some("prod"));
        assert_eq!(c.token.as_deref(), Some("t0k"));
        assert!(c.reveal);
    }

    #[test]
    fn test_from_url_shorthand_and_legacy_scheme() {
        let c = CascadeClient::from_url("cascade://project/abc").unwrap();
        assert_eq!(c.server, "http://localhost:7070");
        assert_eq!(c.project_id, "abc");
        assert!(!c.reveal);

        // Legacy cc:// scheme parses identically.
        let c = CascadeClient::from_url("cc://project/abc").unwrap();
        assert_eq!(c.project_id, "abc");
    }

    #[test]
    fn test_from_url_default_port() {
        let c = CascadeClient::from_url("cascade://cfg.example.com/project/x").unwrap();
        assert_eq!(c.server, "http://cfg.example.com:7070");
    }

    #[test]
    fn test_from_url_rejects_garbage() {
        assert!(CascadeClient::from_url("not a url").is_err());
        assert!(CascadeClient::from_url("cascade://project/").is_err());
    }

    #[test]
    fn test_urlencode() {
        assert_eq!(urlencode("a b/c"), "a%20b%2Fc");
    }
}
