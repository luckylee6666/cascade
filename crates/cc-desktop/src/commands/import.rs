use serde::Serialize;
use tauri::State;

use cc_core::import::{
    format_from_path, is_likely_secret_key, is_likely_secret_value, parse_import_text,
    ImportEntry, ImportFormat,
};

#[derive(Serialize)]
pub struct PreviewEntry {
    pub key: String,
    /// Masked for detected secrets — the webview never receives them.
    pub value: String,
    pub secret: bool,
    pub exists: bool,
}

fn gather(text: Option<&str>, paths: &[String]) -> Result<Vec<ImportEntry>, String> {
    let mut all = Vec::new();
    if let Some(t) = text {
        if !t.trim().is_empty() {
            all.extend(parse_import_text(t, ImportFormat::Auto).map_err(|e| e.to_string())?);
        }
    }
    for p in paths {
        let content = std::fs::read_to_string(p).map_err(|e| format!("{}: {}", p, e))?;
        let entries =
            parse_import_text(&content, format_from_path(p)).map_err(|e| format!("{}: {}", p, e))?;
        all.extend(entries);
    }
    Ok(all)
}

/// Native file picker for config files.
#[tauri::command(rename_all = "snake_case")]
pub async fn pick_import_files() -> Result<Vec<String>, String> {
    let picked = rfd::AsyncFileDialog::new()
        .set_title("Import config files")
        .add_filter("Config files", &["env", "json", "yaml", "yml", "txt"])
        .pick_files()
        .await;
    Ok(picked
        .map(|files| {
            files
                .into_iter()
                .map(|f| f.path().display().to_string())
                .collect()
        })
        .unwrap_or_default())
}

/// Parse text and/or files into a preview (secret values masked).
#[tauri::command(rename_all = "snake_case")]
pub fn preview_import(
    store: State<'_, cc_store::db::Store>,
    text: Option<String>,
    paths: Option<Vec<String>>,
) -> Result<Vec<PreviewEntry>, String> {
    let entries = gather(text.as_deref(), paths.as_deref().unwrap_or(&[]))?;
    let repo = cc_store::config_repo::ConfigRepo::new(store.inner());
    let index = cc_store::import::env_key_index(store.inner()).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for e in entries {
        let secret = e
            .secret
            .unwrap_or_else(|| is_likely_secret_key(&e.key) || is_likely_secret_value(&e.value));
        let exists = cc_store::import::find_existing(&repo, &index, &e.key)
            .map_err(|e| e.to_string())?
            .is_some();
        out.push(PreviewEntry {
            key: e.key,
            value: if secret { "••••••".to_string() } else { e.value },
            secret,
            exists,
        });
    }
    Ok(out)
}

/// Parse and import for real. Secrets are encrypted before storage.
#[tauri::command(rename_all = "snake_case")]
pub fn run_import(
    store: State<'_, cc_store::db::Store>,
    text: Option<String>,
    paths: Option<Vec<String>>,
    group: Option<String>,
    overwrite: bool,
) -> Result<cc_store::import::ImportReport, String> {
    let entries = gather(text.as_deref(), paths.as_deref().unwrap_or(&[]))?;
    if entries.is_empty() {
        return Err("no importable entries found".to_string());
    }
    cc_store::import::import_entries(
        store.inner(),
        &entries,
        &cc_store::import::ImportOptions { group, overwrite },
    )
    .map_err(|e| e.to_string())
}
