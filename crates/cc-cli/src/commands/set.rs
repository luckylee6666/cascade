use anyhow::Result;
use colored::Colorize;

pub fn execute(key: &str, value: &str, secret: bool, env_name: Option<&str>) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::config_repo::ConfigRepo;
    let repo = ConfigRepo::new(&store);

    if let Some(env_name) = env_name {
        // Set the value for one environment only (config × env layer).
        use cc_store::env_repo::EnvRepo;
        use cc_store::env_value_repo::EnvValueRepo;

        let config = repo.get_by_key(key)?.ok_or_else(|| {
            anyhow::anyhow!(
                "Key '{}' not found — create the base entry first: cc set {} <value>",
                key,
                key
            )
        })?;
        let envs = EnvRepo::new(&store).list()?;
        let env = envs
            .iter()
            .find(|e| e.name == env_name || e.id == env_name)
            .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", env_name))?;

        let stored = if secret || config.secret {
            let master_key = cc_core::crypto::get_or_create_master_key()?;
            cc_core::crypto::encrypt(value, &master_key)?
        } else {
            value.to_string()
        };
        EnvValueRepo::new(&store).set(&config.id, &env.id, &stored)?;
        println!(
            "{}",
            format!("Set '{}' for env '{}'", key, env.name).green()
        );
        return Ok(());
    }

    let final_value = if secret {
        let master_key = cc_core::crypto::get_or_create_master_key()?;
        cc_core::crypto::encrypt(value, &master_key)?
    } else {
        value.to_string()
    };

    match repo.get_by_key(key)? {
        Some(config) => {
            repo.update(
                &config.id,
                cc_core::config::ConfigUpdate {
                    value: Some(final_value),
                    secret: Some(secret),
                    group: None,
                    description: None,
                },
            )?;
            println!("{}", format!("Updated '{}'", key).green());
        }
        None => {
            repo.create(cc_core::config::ConfigCreate {
                key: key.to_string(),
                value: Some(final_value),
                secret,
                group: None,
                description: None,
            })?;
            println!("{}", format!("Created '{}'", key).green());
        }
    }

    Ok(())
}
