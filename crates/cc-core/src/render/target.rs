use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;

pub trait RenderTarget {
    fn name(&self) -> &str;
    fn file_path(&self) -> Result<String>;
    fn render(&self, configs: &HashMap<String, String>) -> Result<String>;
    /// Normalize one source entry into the target dialect.
    /// Used for comparison and manifest hashing so that semantically
    /// equal values (e.g. Claude's defaulted `type`/`env`) don't
    /// false-positive as conflicts.
    fn normalize(&self, value: &serde_json::Value) -> serde_json::Value {
        value.clone()
    }
}

/// Ownership ledger: which keys in each target file were written by cc,
/// and what normalized content was last written (fnv1a hex).
/// A key whose current content differs from the recorded hash was
/// hand-edited by the user — never silently overwritten.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RenderManifest {
    pub targets: HashMap<String, TargetState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TargetState {
    pub file_path: String,
    pub keys: HashMap<String, String>,
}

/// `~/.cascade/render-manifest.json`, falling back to legacy `~/.cc/`.
fn manifest_path(read_only: bool) -> Result<std::path::PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home dir"))?;
    let new_path = home.join(".cascade").join("render-manifest.json");
    if read_only && !new_path.exists() {
        let legacy = home.join(".cc").join("render-manifest.json");
        if legacy.exists() {
            return Ok(legacy);
        }
    }
    Ok(new_path)
}

impl RenderManifest {
    pub fn load() -> Self {
        let path = match manifest_path(true) {
            Ok(p) => p,
            Err(_) => return Self::default(),
        };

        if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = manifest_path(false)?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// Deterministic content hash (FNV-1a 64, hex). std's DefaultHasher is
/// randomly seeded per process — unusable for a manifest.
pub fn content_hash(v: &serde_json::Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}
