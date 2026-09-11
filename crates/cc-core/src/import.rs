//! Import parsers: turn existing config files (dotenv / JSON / YAML) into
//! importable entries, with secret heuristics. Shared by the CLI, the HTTP
//! endpoint and the desktop app.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportEntry {
    pub key: String,
    pub value: String,
    /// Explicit flag from the source (e.g. `{"key": "...", "secret": true}`).
    /// `None` = auto-detect by key/value heuristics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    Auto,
    Dotenv,
    Json,
    Yaml,
}

pub fn format_from_path(path: &str) -> ImportFormat {
    let name = std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.starts_with(".env") {
        return ImportFormat::Dotenv;
    }
    match name.rsplit('.').next().unwrap_or("") {
        "json" => ImportFormat::Json,
        "yaml" | "yml" => ImportFormat::Yaml,
        "env" | "dotenv" => ImportFormat::Dotenv,
        _ => ImportFormat::Auto,
    }
}

pub fn parse_import_text(text: &str, format: ImportFormat) -> Result<Vec<ImportEntry>> {
    let entries = match format {
        ImportFormat::Dotenv => parse_dotenv(text),
        ImportFormat::Json => parse_json(text)?,
        ImportFormat::Yaml => parse_yaml(text)?,
        ImportFormat::Auto => parse_auto(text)?,
    };
    // Drop blanks / empty keys, de-duplicate by key (last wins).
    let mut seen = std::collections::HashMap::new();
    let mut out: Vec<ImportEntry> = Vec::new();
    for e in entries {
        if e.key.trim().is_empty() {
            continue;
        }
        match seen.get(&e.key).copied() {
            Some(idx) => out[idx] = e,
            None => {
                seen.insert(e.key.clone(), out.len());
                out.push(e);
            }
        }
    }
    Ok(out)
}

fn parse_auto(text: &str) -> Result<Vec<ImportEntry>> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(entries) = parse_json(text) {
            if !entries.is_empty() {
                return Ok(entries);
            }
        }
    }
    // Decide dotenv vs YAML by line shape first: a dotenv line has '=' before
    // any ': ' (values may contain colons: URLs, quoted timestamps, …).
    if looks_like_dotenv(text) {
        let entries = parse_dotenv(text);
        if !entries.is_empty() {
            return Ok(entries);
        }
    }
    if text
        .lines()
        .any(|l| !l.trim_start().starts_with('#') && l.contains(':'))
    {
        if let Ok(entries) = parse_yaml(text) {
            if !entries.is_empty() {
                return Ok(entries);
            }
        }
    }
    let entries = parse_dotenv(text);
    if entries.is_empty() {
        bail!("cannot parse input as dotenv / JSON / YAML");
    }
    Ok(entries)
}

fn looks_like_dotenv(text: &str) -> bool {
    let mut dotenv = 0i32;
    let mut yaml = 0i32;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        match (l.find('='), l.find(": ")) {
            (Some(e), Some(c)) => {
                if e < c {
                    dotenv += 1;
                } else {
                    yaml += 1;
                }
            }
            (Some(_), None) => dotenv += 1,
            (None, Some(_)) => yaml += 1,
            (None, None) => {}
        }
    }
    dotenv > 0 && dotenv >= yaml
}

/* ── dotenv ────────────────────────────────────────────── */

fn parse_dotenv(text: &str) -> Vec<ImportEntry> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = normalise_env_key(k);
        if key.is_empty() {
            continue;
        }
        let mut value = v.trim().to_string();
        let quoted = (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
            || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2);
        if quoted {
            value = value[1..value.len() - 1].to_string();
        } else if let Some(i) = value.find(" #") {
            value = value[..i].trim_end().to_string();
        }
        out.push(ImportEntry {
            key,
            value,
            secret: None,
        });
    }
    out
}

/// `DATABASE_HOST` → `database.host` (matches the dotenv export mapping).
fn normalise_env_key(k: &str) -> String {
    k.trim().to_lowercase().replace('_', ".")
}

/* ── JSON / YAML ───────────────────────────────────────── */

fn parse_json(text: &str) -> Result<Vec<ImportEntry>> {
    let v: serde_json::Value = serde_json::from_str(text)?;
    collect_json_root(&v)
}

fn parse_yaml(text: &str) -> Result<Vec<ImportEntry>> {
    let v: serde_yaml::Value = serde_yaml::from_str(text)?;
    let json = serde_json::to_value(&v)?;
    collect_json_root(&json)
}

fn collect_json_root(v: &serde_json::Value) -> Result<Vec<ImportEntry>> {
    let mut out = Vec::new();
    match v {
        serde_json::Value::Array(items) => {
            // [{ "key": "...", "value": ..., "secret": true? }, ...]
            for item in items {
                let Some(obj) = item.as_object() else { continue };
                let Some(key) = obj.get("key").and_then(|k| k.as_str()) else {
                    continue;
                };
                let value = obj.get("value").map(json_scalar).unwrap_or_default();
                let secret = obj.get("secret").and_then(|s| s.as_bool());
                out.push(ImportEntry {
                    key: key.to_string(),
                    value,
                    secret,
                });
            }
        }
        serde_json::Value::Object(_) => collect_json(v, "", &mut out),
        _ => bail!("unsupported JSON shape: expected object or array of entries"),
    }
    if out.is_empty() {
        bail!("no importable entries found");
    }
    Ok(out)
}

fn collect_json(v: &serde_json::Value, prefix: &str, out: &mut Vec<ImportEntry>) {
    match v {
        serde_json::Value::Object(map) => {
            // An object carrying key/value is an explicit entry.
            if let (Some(k), Some(val)) = (
                map.get("key").and_then(|x| x.as_str()),
                map.get("value"),
            ) {
                out.push(ImportEntry {
                    key: k.to_string(),
                    value: json_scalar(val),
                    secret: map.get("secret").and_then(|s| s.as_bool()),
                });
                return;
            }
            // Flatten at most two levels (`group.name`); deeper objects are
            // kept whole as JSON strings (e.g. mcpServers.blender → {...}).
            if prefix.contains('.') {
                out.push(ImportEntry {
                    key: prefix.to_string(),
                    value: v.to_string(),
                    secret: None,
                });
                return;
            }
            for (k, val) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                collect_json(val, &key, out);
            }
        }
        other => {
            if !prefix.is_empty() {
                out.push(ImportEntry {
                    key: prefix.to_string(),
                    value: json_scalar(other),
                    secret: None,
                });
            }
        }
    }
}

fn json_scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/* ── secret heuristics ─────────────────────────────────── */

const SECRET_SEGMENTS: &[&str] = &[
    "key",
    "keys",
    "token",
    "secret",
    "password",
    "passwd",
    "pwd",
    "credential",
    "credentials",
    "apikey",
    "accesskey",
    "privatekey",
];

const SECRET_SUBSTRINGS: &[&str] = &[
    "apikey",
    "secret",
    "token",
    "password",
    "passwd",
    "credential",
    "accesskey",
    "privatekey",
    "signingkey",
];

const SECRET_VALUE_PREFIXES: &[&str] = &[
    "sk-",
    "sk_",
    "ghp_",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "AKIA",
    "AIza",
    "-----BEGIN",
];

pub fn is_likely_secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    if k.split(['.', '-', '_']).any(|seg| SECRET_SEGMENTS.contains(&seg)) {
        return true;
    }
    SECRET_SUBSTRINGS.iter().any(|s| k.contains(s))
}

pub fn is_likely_secret_value(value: &str) -> bool {
    let v = value.trim();
    SECRET_VALUE_PREFIXES.iter().any(|p| v.starts_with(p))
}

/* ── tests ─────────────────────────────────────────────── */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dotenv_parsing() {
        let text = r#"
# comment
DATABASE_HOST=localhost
export REDIS_PORT=6379
API_KEY="sk-demo-123"
QUOTED='single'
EMPTY=
"#;
        let entries = parse_import_text(text, ImportFormat::Dotenv).unwrap();
        let get = |k: &str| entries.iter().find(|e| e.key == k).map(|e| e.value.clone());
        assert_eq!(get("database.host").as_deref(), Some("localhost"));
        assert_eq!(get("redis.port").as_deref(), Some("6379"));
        assert_eq!(get("api.key").as_deref(), Some("sk-demo-123"));
        assert_eq!(get("quoted").as_deref(), Some("single"));
        assert_eq!(get("empty").as_deref(), Some(""));
    }

    #[test]
    fn test_json_nested_flatten() {
        let text = r#"{"database": {"host": "localhost", "port": 5432}, "mcpServers": {"blender": {"command": "uvx"}}}"#;
        let entries = parse_import_text(text, ImportFormat::Auto).unwrap();
        let get = |k: &str| entries.iter().find(|e| e.key == k).map(|e| e.value.clone());
        assert_eq!(get("database.host").as_deref(), Some("localhost"));
        assert_eq!(get("database.port").as_deref(), Some("5432"));
        assert_eq!(get("mcpServers.blender").as_deref(), Some(r#"{"command":"uvx"}"#));
    }

    #[test]
    fn test_json_entry_array_with_secret_flag() {
        let text = r#"[{"key": "api.key", "value": "xyz", "secret": true}, {"key": "db.host", "value": "h"}]"#;
        let entries = parse_import_text(text, ImportFormat::Auto).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].secret, Some(true));
        assert_eq!(entries[1].secret, None);
    }

    #[test]
    fn test_yaml_parsing() {
        let text = "database:\n  host: localhost\n  port: 5432\napi_key: abc\n";
        let entries = parse_import_text(text, ImportFormat::Auto).unwrap();
        let get = |k: &str| entries.iter().find(|e| e.key == k).map(|e| e.value.clone());
        assert_eq!(get("database.host").as_deref(), Some("localhost"));
        assert_eq!(get("database.port").as_deref(), Some("5432"));
        assert_eq!(get("api_key").as_deref(), Some("abc"));
    }

    #[test]
    fn test_auto_detect_dotenv_with_equals_sign() {
        let text = "A=1\nB=2\n";
        let entries = parse_import_text(text, ImportFormat::Auto).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_auto_dotenv_with_colon_in_values() {
        // Colons inside dotenv values must not flip detection to YAML.
        let text = "SMTP_PASS=\"p: ssword\"\nDATABASE_URL=postgres://user:pass@host:5432/db\n";
        let entries = parse_import_text(text, ImportFormat::Auto).unwrap();
        let get = |k: &str| entries.iter().find(|e| e.key == k).map(|e| e.value.clone());
        assert_eq!(get("smtp.pass").as_deref(), Some("p: ssword"));
        assert_eq!(
            get("database.url").as_deref(),
            Some("postgres://user:pass@host:5432/db")
        );
    }

    #[test]
    fn test_secret_heuristics() {
        for k in [
            "api.key",
            "database.password",
            "ACCESS_TOKEN",
            "stripe_secret",
            "signingkey",
            "db.pwd",
            "aws.credentials",
        ] {
            assert!(is_likely_secret_key(k), "expected secret: {}", k);
        }
        for k in ["database.host", "database.pool_size", "monkey", "hockey", "redis.port"] {
            assert!(!is_likely_secret_key(k), "expected plain: {}", k);
        }
        assert!(is_likely_secret_value("sk-demo-123"));
        assert!(is_likely_secret_value("ghp_xxxxxxxx"));
        assert!(!is_likely_secret_value("localhost"));
    }

    #[test]
    fn test_format_from_path() {
        assert_eq!(format_from_path("/a/.env"), ImportFormat::Dotenv);
        assert_eq!(format_from_path("/a/.env.production"), ImportFormat::Dotenv);
        assert_eq!(format_from_path("settings.json"), ImportFormat::Json);
        assert_eq!(format_from_path("config.yaml"), ImportFormat::Yaml);
        assert_eq!(format_from_path("notes.txt"), ImportFormat::Auto);
    }
}
