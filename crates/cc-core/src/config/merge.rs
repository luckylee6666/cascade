use std::collections::HashMap;
use serde_json::Value;

pub fn deep_merge(base: &mut Value, override_val: &Value) {
    match (base, override_val) {
        (Value::Object(base_map), Value::Object(override_map)) => {
            for (key, value) in override_map {
                if let Some(base_val) = base_map.get_mut(key) {
                    deep_merge(base_val, value);
                } else {
                    base_map.insert(key.clone(), value.clone());
                }
            }
        }
        (base, override_val) => {
            *base = override_val.clone();
        }
    }
}

pub fn merge_maps(
    base: &HashMap<String, Value>,
    overrides: &HashMap<String, Value>,
) -> HashMap<String, Value> {
    let mut result = base.clone();
    for (key, value) in overrides {
        if let Some(base_val) = result.get_mut(key) {
            deep_merge(base_val, value);
        } else {
            result.insert(key.clone(), value.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deep_merge() {
        let mut base = json!({
            "db": {
                "host": "localhost",
                "port": 5432
            }
        });
        let override_val = json!({
            "db": {
                "port": 5433
            }
        });

        deep_merge(&mut base, &override_val);
        assert_eq!(base["db"]["host"], "localhost");
        assert_eq!(base["db"]["port"], 5433);
    }

    #[test]
    fn test_merge_maps() {
        let mut base = HashMap::new();
        base.insert("a".into(), json!(1));
        base.insert("b".into(), json!({"x": 1}));

        let mut overrides = HashMap::new();
        overrides.insert("b".into(), json!({"y": 2}));
        overrides.insert("c".into(), json!(3));

        let result = merge_maps(&base, &overrides);
        assert_eq!(result["a"], json!(1));
        assert_eq!(result["b"]["x"], json!(1));
        assert_eq!(result["b"]["y"], json!(2));
        assert_eq!(result["c"], json!(3));
    }
}
