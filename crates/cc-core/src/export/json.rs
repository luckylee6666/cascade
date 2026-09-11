use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;

pub fn export_json(
    configs: &[(String, String)],
    revision: u64,
) -> Result<String> {
    let mut root = json!({});

    // Group configs by prefix
    let mut groups: HashMap<String, Vec<(&str, &str)>> = HashMap::new();

    for (key, value) in configs {
        let parts: Vec<&str> = key.split('.').collect();
        let group = if parts.len() > 1 {
            parts[0].to_string()
        } else {
            "root".to_string()
        };
        groups.entry(group).or_default().push((key, value));
    }

    // Build nested structure
    for (group, items) in &groups {
        if group == "root" {
            for (key, value) in items {
                let simple_key = key.split('.').last().unwrap_or(key);
                root[simple_key] = json!(value);
            }
        } else {
            let mut sub_obj = json!({});
            for (key, value) in items {
                let simple_key = key.split('.').last().unwrap_or(key);
                sub_obj[simple_key] = json!(value);
            }
            root[&group] = sub_obj;
        }
    }

    // Add metadata
    let output = json!({
        "_meta": {
            "generator": "cascade",
            "revision": revision
        },
        "config": root
    });

    Ok(serde_json::to_string_pretty(&output)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn test_export_json() {
        let configs = vec![
            ("database.host".into(), "localhost".into()),
            ("database.port".into(), "5432".into()),
        ];

        let output = export_json(&configs, 1).unwrap();
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["_meta"]["revision"], 1);
        assert_eq!(parsed["config"]["database"]["host"], "localhost");
    }
}
