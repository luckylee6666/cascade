use super::schema::*;
use anyhow::{Result, bail};

/// Resolve the effective value of every config.
///
/// Layer order (later wins):
///   1. base value on the config itself
///   2. env-chain values (root → leaf) from config × env values
///   3. project override for the *leaf* env (project_configs)
pub fn resolve_inheritance(
    configs: &[Config],
    env_chain: &[Environment],
    env_values: &[ConfigEnvValue],
    project_overrides: &[ProjectConfig],
) -> Vec<ConfigWithValue> {
    let leaf_env = env_chain.last();

    configs
        .iter()
        .map(|config| {
            let mut value = config.value.clone().unwrap_or_default();
            let mut source = ConfigSource::Base;

            for env in env_chain {
                if let Some(ev) = env_values
                    .iter()
                    .find(|v| v.config_id == config.id && v.env_id == env.id)
                {
                    value = ev.value.clone().unwrap_or_default();
                    source = ConfigSource::Environment(env.name.clone());
                }
            }

            if let Some(env) = leaf_env {
                if let Some(o) = project_overrides
                    .iter()
                    .find(|o| o.config_id == config.id && o.env_id == env.id)
                {
                    if let Some(v) = &o.override_value {
                        value = v.clone();
                        source = ConfigSource::ProjectOverride(env.name.clone());
                    }
                }
            }

            ConfigWithValue {
                config: config.clone(),
                effective_value: value,
                source,
            }
        })
        .collect()
}

pub fn validate_inheritance_chain(envs: &[Environment]) -> Result<()> {
    // NOTE: `visited` is per-chain. A shared set across chains would falsely
    // flag diamonds (two envs sharing one parent) as cycles.
    for env in envs {
        let mut visited = std::collections::HashSet::new();
        let mut current = env;
        while let Some(parent_id) = &current.parent_id {
            if !visited.insert(parent_id.clone()) {
                bail!(
                    "Circular inheritance detected involving environment '{}'",
                    env.name
                );
            }
            match envs.iter().find(|e| &e.id == parent_id) {
                Some(parent) => current = parent,
                // Dangling parent: rejected by the DB foreign key; stop here.
                None => break,
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_config(id: &str, key: &str, value: &str) -> Config {
        Config {
            id: id.into(),
            key: key.into(),
            value: Some(value.into()),
            secret: false,
            encrypted: None,
            group: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_simple_inheritance() {
        let configs = vec![test_config("1", "db.host", "localhost")];
        let result = resolve_inheritance(&configs, &[], &[], &[]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].effective_value, "localhost");
        assert!(matches!(result[0].source, ConfigSource::Base));
    }

    #[test]
    fn test_env_value_override() {
        let configs = vec![test_config("1", "db.host", "localhost")];
        let env_chain = vec![test_env("base", None), test_env("prod", Some("base"))];
        let env_values = vec![ConfigEnvValue {
            config_id: "1".into(),
            env_id: "prod".into(),
            value: Some("prod.db.example.com".into()),
        }];

        let result = resolve_inheritance(&configs, &env_chain, &env_values, &[]);
        assert_eq!(result[0].effective_value, "prod.db.example.com");
        match &result[0].source {
            ConfigSource::Environment(name) => assert_eq!(name, "prod"),
            other => panic!("expected Environment source, got {:?}", other),
        }
    }

    #[test]
    fn test_project_override_beats_env_value() {
        let configs = vec![test_config("1", "db.host", "localhost")];
        let env_chain = vec![test_env("base", None), test_env("prod", Some("base"))];
        let env_values = vec![ConfigEnvValue {
            config_id: "1".into(),
            env_id: "prod".into(),
            value: Some("prod.db.example.com".into()),
        }];
        let overrides = vec![ProjectConfig {
            project_id: "p1".into(),
            config_id: "1".into(),
            env_id: "prod".into(),
            override_value: Some("just-this-project.example.com".into()),
            override_encrypted: None,
        }];

        let result = resolve_inheritance(&configs, &env_chain, &env_values, &overrides);
        assert_eq!(result[0].effective_value, "just-this-project.example.com");
        assert!(matches!(result[0].source, ConfigSource::ProjectOverride(_)));
    }

    fn test_env(id: &str, parent_id: Option<&str>) -> Environment {
        Environment {
            id: id.into(),
            name: id.into(),
            parent_id: parent_id.map(|s| s.into()),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_chain_valid() {
        let envs = vec![
            test_env("base", None),
            test_env("staging", Some("base")),
            test_env("prod", Some("staging")),
        ];
        assert!(validate_inheritance_chain(&envs).is_ok());
    }

    #[test]
    fn test_chain_diamond_shared_parent_ok() {
        // Two envs sharing one parent is NOT a cycle.
        let envs = vec![
            test_env("base", None),
            test_env("a", Some("base")),
            test_env("b", Some("base")),
        ];
        assert!(validate_inheritance_chain(&envs).is_ok());
    }

    #[test]
    fn test_chain_self_cycle_rejected() {
        let envs = vec![test_env("a", Some("a"))];
        assert!(validate_inheritance_chain(&envs).is_err());
    }

    #[test]
    fn test_chain_two_cycle_rejected() {
        let envs = vec![test_env("a", Some("b")), test_env("b", Some("a"))];
        assert!(validate_inheritance_chain(&envs).is_err());
    }
}
