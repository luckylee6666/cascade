use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;
use std::path::Path;

pub fn execute(target: Option<&str>, project: &str, env_name: Option<&str>, force: bool) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    // Include only configs attached to the project; env chain + overrides applied.
    let scope = cc_store::resolve::resolve_project_scope(&store, project, env_name)?;

    // Filter MCP configs
    let mcp_configs: HashMap<String, String> = scope.configs.iter()
        .filter(|c| c.config.key.starts_with("mcpServers."))
        .map(|c| (c.config.key.clone(), c.effective_value.clone()))
        .collect();

    // Get targets (comma-separated --target supported).
    let targets: Vec<Box<dyn cc_core::render::target::RenderTarget>> = if let Some(target_name) = target {
        let mut list = Vec::new();
        for name in target_name.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            list.push(
                cc_core::render::mcp::get_target(name)
                    .ok_or_else(|| anyhow::anyhow!("Unknown target: {}", name))?,
            );
        }
        list
    } else {
        cc_core::render::mcp::get_all_targets()
    };

    use cc_core::render::target::{content_hash, RenderManifest, TargetState};

    let mut manifest = RenderManifest::load();

    // Render
    for t in targets {
        let file_path = t.file_path()?;
        let Some(parent) = std::path::Path::new(&file_path).parent() else {
            println!("{}", format!("  Skipping {} (bad path)", t.name()).yellow());
            continue;
        };

        if !parent.exists() {
            println!("{}", format!("  Skipping {} (not installed)", t.name()).yellow());
            continue;
        }

        let before = std::fs::read_to_string(&file_path).unwrap_or_else(|_| "{}".into());
        let mut doc: serde_json::Value =
            serde_json::from_str(&before).unwrap_or(serde_json::json!({}));
        if doc.get("mcpServers").is_none() {
            doc["mcpServers"] = serde_json::json!({});
        }
        // Safe: just ensured above.
        let servers = doc["mcpServers"]
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("{}: mcpServers is not an object", file_path))?;

        let entry = manifest
            .targets
            .entry(t.name().to_string())
            .or_insert_with(|| TargetState {
                file_path: file_path.clone(),
                keys: Default::default(),
            });
        entry.file_path = file_path.clone();

        let mut written = 0;
        let mut unchanged = 0;
        let mut conflicts: Vec<String> = vec![];
        let mut pruned = 0;

        // Source keys present in the vault.
        let mut wanted: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (key, raw) in &mcp_configs {
            let Some(name) = key.strip_prefix("mcpServers.") else {
                continue;
            };
            wanted.insert(name.to_string());
            let src_val: serde_json::Value = match serde_json::from_str(raw) {
                Ok(v) => v,
                Err(_) => {
                    println!("{}", format!("  Skipping {}: not valid JSON", key).yellow());
                    continue;
                }
            };
            let norm = t.normalize(&src_val);
            let nhash = content_hash(&norm);

            match servers.get(name) {
                None => {
                    servers.insert(name.to_string(), norm);
                    entry.keys.insert(name.to_string(), nhash);
                    written += 1;
                }
                Some(cur) if *cur == norm => {
                    // Already in the desired state; adopt into the manifest
                    // so future hand-edits are detectable.
                    entry.keys.insert(name.to_string(), nhash);
                    unchanged += 1;
                }
                Some(cur) => {
                    let owned = entry
                        .keys
                        .get(name)
                        .map(|h| *h == content_hash(cur))
                        .unwrap_or(false);
                    if owned || force {
                        servers.insert(name.to_string(), norm);
                        entry.keys.insert(name.to_string(), nhash);
                        written += 1;
                    } else {
                        conflicts.push(name.to_string());
                    }
                }
            }
        }

        // Keys cc owns but the source no longer has: clean up,
        // unless the user hand-modified them after we wrote.
        let owned_names: Vec<String> = entry.keys.keys().cloned().collect();
        for name in owned_names {
            if wanted.contains(&name) {
                continue;
            }
            let recorded = entry.keys.get(&name).cloned().unwrap_or_default();
            let current_ok = servers
                .get(&name)
                .map(|v| content_hash(v) == recorded)
                .unwrap_or(false);
            if current_ok {
                servers.remove(&name);
                entry.keys.remove(&name);
                pruned += 1;
            } else if servers.contains_key(&name) {
                conflicts.push(format!("{} (deleted in source, modified locally)", name));
            } else {
                // Already gone (user deleted); drop from manifest quietly.
                entry.keys.remove(&name);
            }
        }

        let after = serde_json::to_string_pretty(&doc)?;
        if after != before {
            if Path::new(&file_path).exists() {
                let backup = format!("{}.cc-backup", file_path);
                std::fs::copy(&file_path, &backup)?;
            }
            std::fs::write(&file_path, &after)?;
        }
        manifest.save()?;

        println!(
            "  {}: {} written, {} unchanged, {} pruned",
            t.name().green(),
            written,
            unchanged,
            pruned
        );
        for c in &conflicts {
            println!(
                "  {} conflict (kept local value, use --force to take over): {}",
                t.name().yellow(),
                c
            );
        }
    }

    Ok(())
}


