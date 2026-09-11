use anyhow::Result;
use colored::Colorize;

use super::ModelsCommands;
use cc_core::drift::models::{compute_model_diff, ModelInfo, ModelType};

pub fn execute(command: ModelsCommands) -> Result<()> {
    match command {
        ModelsCommands::Sync { url, key, dry_run } => {
            execute_sync(url.as_deref(), key.as_deref(), dry_run)
        }
    }
}

/// Pull an OpenAI-compatible `/models` endpoint, diff against the vault's
/// `models.*` inventory (group `models`), and add what's missing.
/// Additive only: remote lists are often partial, so local entries are
/// never deleted.
fn execute_sync(url: Option<&str>, key: Option<&str>, dry_run: bool) -> Result<()> {
    let base = url.ok_or_else(|| {
        anyhow::anyhow!("--url <base> is required, e.g. --url https://api.openai.com/v1")
    })?;
    let key = key
        .map(|s| s.to_string())
        .or_else(|| std::env::var("CASCADE_MODELS_KEY").ok())
        .or_else(|| std::env::var("CC_MODELS_KEY").ok());
    let endpoint = format!("{}/models", base.trim_end_matches('/'));

    let mut req = ureq::get(&endpoint);
    if let Some(k) = &key {
        req = req.set("Authorization", &format!("Bearer {}", k));
    }
    let resp: serde_json::Value = req
        .call()
        .map_err(|e| anyhow::anyhow!("GET {} failed: {}", endpoint, e))?
        .into_json()
        .map_err(|e| anyhow::anyhow!("invalid JSON from {}: {}", endpoint, e))?;
    let data = resp
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| {
            anyhow::anyhow!("expected OpenAI-style {{\"data\": [...]}} from {}", endpoint)
        })?;

    let remote: Vec<ModelInfo> = data
        .iter()
        .filter_map(|m| {
            m.get("id")
                .and_then(|id| id.as_str())
                .map(|id| ModelInfo {
                    id: id.to_string(),
                    name: id.to_string(),
                    provider: None,
                    model_type: ModelType::Other,
                })
        })
        .collect();

    let store = cc_store::db::Store::open_default()?;
    let repo = cc_store::config_repo::ConfigRepo::new(&store);
    let local_cfgs = repo.list(Some("models"))?;
    let local_ids: Vec<String> = local_cfgs
        .iter()
        .map(|c| {
            c.key
                .strip_prefix("models.")
                .unwrap_or(&c.key)
                .to_string()
        })
        .collect();

    let diff = compute_model_diff(&remote, &local_ids);

    println!(
        "remote: {} chat-capable, local inventory: {}, to add: {}, kept: {}, skipped (non-chat): {}",
        diff.to_add.len() + diff.to_keep.len(),
        local_ids.len(),
        diff.to_add.len(),
        diff.to_keep.len(),
        diff.to_skip_non_chat.len()
    );
    for m in &diff.to_add {
        println!("  + {}", m.id);
    }
    if dry_run {
        println!("{}", "Dry run: nothing written.".yellow());
        return Ok(());
    }

    for m in &diff.to_add {
        repo.create(cc_core::config::ConfigCreate {
            key: format!("models.{}", m.id),
            value: Some(m.id.clone()),
            secret: false,
            group: Some("models".to_string()),
            description: Some(format!("synced from {}", endpoint)),
        })?;
    }
    println!("{}", format!("Added {} models.", diff.to_add.len()).green());
    Ok(())
}
