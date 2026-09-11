use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;

pub async fn execute(
    project: &str,
    output: Option<&str>,
    format: &str,
    env_name: Option<&str>,
    watch: bool,
    reveal: bool,
    interval_secs: u64,
) -> Result<()> {
    match format {
        "yaml" | "json" | "dotenv" => {}
        _ => anyhow::bail!("Unknown format: {}", format),
    }
    if watch && output.is_none() {
        anyhow::bail!("--watch requires -o/--output: there is no file to keep updated");
    }

    let mut last_content = String::new();
    loop {
        let content = render_once(project, env_name, format, reveal)?;
        if content != last_content {
            if let Some(output_path) = output {
                atomic_write(output_path, &content)?;
                if last_content.is_empty() {
                    println!("{}", format!("Exported to {}", output_path).green());
                } else {
                    println!("{}", format!("Updated {}", output_path).green());
                }
            } else {
                println!("{}", content);
            }
            last_content = content;
        }
        if !watch {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs.max(1))).await;
    }

    Ok(())
}

/// Render once with a fresh store handle (required for --watch to see
/// changes made by other processes).
fn render_once(
    project: &str,
    env_name: Option<&str>,
    format: &str,
    reveal: bool,
) -> Result<String> {
    let store = cc_store::db::Store::open_default()?;

    // Include only configs attached to the project; env chain + overrides applied.
    let scope = cc_store::resolve::resolve_project_scope(&store, project, env_name)?;
    let resolved = &scope.configs;

    // Prepare output
    let mut pairs: Vec<(String, String)> = Vec::new();
    for c in resolved {
        let value = if reveal {
            if cc_core::crypto::is_encrypted(&c.effective_value) {
                let master_key = cc_core::crypto::get_or_create_master_key()?;
                cc_core::crypto::decrypt(&c.effective_value, &master_key)?
            } else {
                c.effective_value.clone()
            }
        } else if c.config.secret {
            // Check if encrypted and mask
            if cc_core::crypto::is_encrypted(&c.effective_value) {
                format!("${{{}}}", c.config.key.replace('.', "_").to_uppercase())
            } else {
                c.effective_value.clone()
            }
        } else {
            c.effective_value.clone()
        };
        pairs.push((c.config.key.clone(), value));
    }

    // Monotonic revision: bumps on every recorded change.
    let revision: u64 = store
        .conn()
        .query_row("SELECT COALESCE(MAX(id), 0) FROM config_history", [], |r| {
            r.get(0)
        })?;

    // Export
    let content = match format {
        "yaml" => cc_core::export::yaml::export_yaml(&pairs, revision)?,
        "json" => cc_core::export::json::export_json(&pairs, revision)?,
        "dotenv" => cc_core::export::dotenv::export_dotenv(&pairs, reveal)?,
        _ => anyhow::bail!("Unknown format: {}", format),
    };

    Ok(content)
}

/// Write-temp-then-rename: readers never see a half-written file.
fn atomic_write(path: &str, content: &str) -> Result<()> {
    let p = PathBuf::from(path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = format!("{}.cc-tmp-{}", path, std::process::id());
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}
