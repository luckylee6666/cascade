use anyhow::Result;
use colored::Colorize;
use std::path::Path;

pub fn execute(files: &[String], group: Option<&str>, dry_run: bool) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::config_repo::ConfigRepo;
    let repo = ConfigRepo::new(&store);

    let mut imported = 0;
    let mut skipped = 0;

    for file_path in files {
        let path = Path::new(file_path);
        if !path.exists() {
            println!("{}", format!("File not found: {}", file_path).red());
            skipped += 1;
            continue;
        }

        let content = std::fs::read_to_string(path)?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let configs = match ext {
            "env" => parse_dotenv(&content),
            "json" => parse_json(&content)?,
            "yaml" | "yml" => parse_yaml(&content)?,
            _ => {
                println!("{}", format!("Unsupported format: {}", ext).red());
                skipped += 1;
                continue;
            }
        };

        for (key, value) in configs {
            if dry_run {
                println!("  Would import: {} = {}", key, value);
            } else {
                match repo.get_by_key(&key)? {
                    Some(_) => {
                        println!("{}", format!("  Skipped (exists): {}", key).yellow());
                    }
                    None => {
                        repo.create(cc_core::config::ConfigCreate {
                            key,
                            value: Some(value),
                            secret: false,
                            group: group.map(|g| g.to_string()),
                            description: None,
                        })?;
                        imported += 1;
                    }
                }
            }
        }
    }

    if dry_run {
        println!("\n{}", "Dry run complete".yellow());
    } else {
        println!("\n{} configs imported, {} skipped", imported, skipped);
    }

    Ok(())
}

fn parse_dotenv(content: &str) -> Vec<(String, String)> {
    let mut configs = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_lowercase().replace('_', ".");
            let value = value.trim().trim_matches('"').trim_matches('\'');
            configs.push((key, value.to_string()));
        }
    }
    configs
}

fn parse_json(content: &str) -> Result<Vec<(String, String)>> {
    let mut configs = Vec::new();
    let value: serde_json::Value = serde_json::from_str(content)?;

    fn flatten(prefix: &str, value: serde_json::Value, configs: &mut Vec<(String, String)>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, val) in map {
                    let new_key = if prefix.is_empty() { key } else { format!("{}.{}", prefix, key) };
                    flatten(&new_key, val, configs);
                }
            }
            serde_json::Value::Array(arr) => {
                let json_str = serde_json::to_string(&arr).unwrap_or_default();
                configs.push((prefix.to_string(), json_str));
            }
            _ => {
                let val_str = match value {
                    serde_json::Value::Null => String::new(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s,
                    _ => String::new(),
                };
                configs.push((prefix.to_string(), val_str));
            }
        }
    }

    flatten("", value, &mut configs);
    Ok(configs)
}

fn parse_yaml(content: &str) -> Result<Vec<(String, String)>> {
    let mut configs = Vec::new();
    let value: serde_yaml::Value = serde_yaml::from_str(content)?;

    fn flatten(prefix: &str, value: serde_yaml::Value, configs: &mut Vec<(String, String)>) {
        match value {
            serde_yaml::Value::Mapping(map) => {
                for (key, val) in map {
                    if let serde_yaml::Value::String(key_str) = key {
                        let new_key = if prefix.is_empty() { key_str } else { format!("{}.{}", prefix, key_str) };
                        flatten(&new_key, val, configs);
                    }
                }
            }
            serde_yaml::Value::Sequence(seq) => {
                let json_str = serde_yaml::to_string(&seq).unwrap_or_default();
                configs.push((prefix.to_string(), json_str));
            }
            _ => {
                let val_str = match value {
                    serde_yaml::Value::Null => String::new(),
                    serde_yaml::Value::Bool(b) => b.to_string(),
                    serde_yaml::Value::Number(n) => n.to_string(),
                    serde_yaml::Value::String(s) => s,
                    _ => String::new(),
                };
                configs.push((prefix.to_string(), val_str));
            }
        }
    }

    flatten("", value, &mut configs);
    Ok(configs)
}


