use anyhow::Result;
use colored::Colorize;

pub fn execute(key: &str, env_name: Option<&str>) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::config_repo::ConfigRepo;
    let repo = ConfigRepo::new(&store);

    let config = match repo.get_by_key(key)? {
        Some(c) => c,
        None => {
            println!("{}", format!("Key '{}' not found", key).red());
            return Ok(());
        }
    };

    if let Some(env_name) = env_name {
        use cc_store::env_repo::EnvRepo;
        use cc_store::env_value_repo::EnvValueRepo;

        let envs = EnvRepo::new(&store).list()?;
        let env = envs
            .iter()
            .find(|e| e.name == env_name || e.id == env_name)
            .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", env_name))?;
        EnvValueRepo::new(&store).remove(&config.id, &env.id)?;
        println!(
            "{}",
            format!(
                "Removed '{}' from env '{}' (falls back to base/parent)",
                key, env.name
            )
            .green()
        );
        return Ok(());
    }

    repo.delete(&config.id)?;
    println!("{}", format!("Deleted '{}'", key).green());
    Ok(())
}
