use std::collections::HashMap;

pub fn flatten_to_env(
    configs: &[(String, String)], // (key, value) pairs
) -> HashMap<String, String> {
    let mut result = HashMap::new();

    for (key, value) in configs {
        let env_key = key_to_env_name(key);
        result.insert(env_key, value.clone());
    }

    result
}

pub fn key_to_env_name(key: &str) -> String {
    key.replace('.', "_")
        .replace('-', "_")
        .to_uppercase()
}

pub fn env_name_to_key(env_name: &str) -> String {
    env_name
        .to_lowercase()
        .replace('_', ".")
}

pub fn flatten_value(value: &str) -> String {
    // null -> empty string
    if value.is_empty() {
        return String::new();
    }
    value.to_string()
}

pub fn flatten_complex_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_to_env_name() {
        assert_eq!(key_to_env_name("database.host"), "DATABASE_HOST");
        assert_eq!(key_to_env_name("redis.port"), "REDIS_PORT");
        assert_eq!(key_to_env_name("api-key"), "API_KEY");
    }

    #[test]
    fn test_flatten_to_env() {
        let configs = vec![
            ("database.host".into(), "localhost".into()),
            ("database.port".into(), "5432".into()),
        ];

        let result = flatten_to_env(&configs);
        assert_eq!(result["DATABASE_HOST"], "localhost");
        assert_eq!(result["DATABASE_PORT"], "5432");
    }
}
