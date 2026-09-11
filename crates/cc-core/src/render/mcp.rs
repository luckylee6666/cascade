use super::target::*;
use anyhow::Result;
use std::collections::HashMap;

pub struct QwenCodeTarget;

impl RenderTarget for QwenCodeTarget {
    fn name(&self) -> &str {
        "qwen-code"
    }

    fn file_path(&self) -> Result<String> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home dir"))?;
        Ok(home.join(".qwen").join("settings.json").to_string_lossy().to_string())
    }

    fn render(&self, configs: &HashMap<String, String>) -> Result<String> {
        let mut settings: serde_json::Value = if let Ok(content) = std::fs::read_to_string(self.file_path()?) {
            serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        // Update mcpServers
        if settings.get("mcpServers").is_none() {
            settings["mcpServers"] = serde_json::json!({});
        }
        let mcp_servers = settings["mcpServers"]
            .as_object_mut()
            .expect("mcpServers ensured as object above");

        for (key, value) in configs {
            if let Some(server_name) = key.strip_prefix("mcpServers.") {
                if let Some(server_config) = serde_json::from_str::<serde_json::Value>(value).ok() {
                    mcp_servers.insert(server_name.to_string(), self.normalize(&server_config));
                }
            }
        }

        Ok(serde_json::to_string_pretty(&settings)?)
    }
}

pub struct ClaudeCodeTarget;

impl RenderTarget for ClaudeCodeTarget {
    fn name(&self) -> &str {
        "claude-code"
    }

    fn file_path(&self) -> Result<String> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home dir"))?;
        Ok(home.join(".claude.json").to_string_lossy().to_string())
    }

    fn render(&self, configs: &HashMap<String, String>) -> Result<String> {
        let mut settings: serde_json::Value = if let Ok(content) = std::fs::read_to_string(self.file_path()?) {
            serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if settings.get("mcpServers").is_none() {
            settings["mcpServers"] = serde_json::json!({});
        }
        let mcp_servers = settings["mcpServers"]
            .as_object_mut()
            .expect("mcpServers ensured as object above");

        for (key, value) in configs {
            if let Some(server_name) = key.strip_prefix("mcpServers.") {
                if let Some(server_config) = serde_json::from_str::<serde_json::Value>(value).ok() {
                    mcp_servers.insert(server_name.to_string(), self.normalize(&server_config));
                }
            }
        }

        Ok(serde_json::to_string_pretty(&settings)?)
    }

    fn normalize(&self, value: &serde_json::Value) -> serde_json::Value {
        // Claude requires type: stdio (+ empty env) on command entries.
        // Centralized here so compare/write/manifest all see the same value.
        let mut v = value.clone();
        if v.get("command").is_some() {
            v["type"] = serde_json::json!("stdio");
            if v.get("env").is_none() {
                v["env"] = serde_json::json!({});
            }
        }
        v
    }
}

pub struct CursorTarget;

impl RenderTarget for CursorTarget {
    fn name(&self) -> &str {
        "cursor"
    }

    fn file_path(&self) -> Result<String> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home dir"))?;
        Ok(home.join(".cursor").join("mcp.json").to_string_lossy().to_string())
    }

    fn render(&self, configs: &HashMap<String, String>) -> Result<String> {
        let mut settings: serde_json::Value = if let Ok(content) = std::fs::read_to_string(self.file_path()?) {
            serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if settings.get("mcpServers").is_none() {
            settings["mcpServers"] = serde_json::json!({});
        }
        let mcp_servers = settings["mcpServers"]
            .as_object_mut()
            .expect("mcpServers ensured as object above");

        for (key, value) in configs {
            if let Some(server_name) = key.strip_prefix("mcpServers.") {
                if let Some(server_config) = serde_json::from_str::<serde_json::Value>(value).ok() {
                    mcp_servers.insert(server_name.to_string(), self.normalize(&server_config));
                }
            }
        }

        Ok(serde_json::to_string_pretty(&settings)?)
    }
}

pub fn get_all_targets() -> Vec<Box<dyn RenderTarget>> {
    vec![
        Box::new(QwenCodeTarget),
        Box::new(ClaudeCodeTarget),
        Box::new(CursorTarget),
    ]
}

pub fn get_target(name: &str) -> Option<Box<dyn RenderTarget>> {
    match name {
        "qwen-code" => Some(Box::new(QwenCodeTarget)),
        "claude-code" => Some(Box::new(ClaudeCodeTarget)),
        "cursor" => Some(Box::new(CursorTarget)),
        _ => None,
    }
}
