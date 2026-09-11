use anyhow::Result;
use colored::Colorize;

pub fn execute() -> Result<()> {
    let store_path = cc_store::db::Store::vault_path()?;

    if store_path.exists() {
        println!("{}", "Vault already initialized".yellow());
        println!("  Path: {}", store_path.display());
        return Ok(());
    }

    if let Some(parent) = store_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let store = cc_store::db::Store::open(&store_path)?;

    // Create default environment
    use cc_store::env_repo::EnvRepo;
    use cc_core::config::EnvironmentCreate;

    let env_repo = EnvRepo::new(&store);
    env_repo.create(EnvironmentCreate {
        name: "base".into(),
        parent_id: None,
    })?;

    println!("{}", "Vault initialized successfully".green());
    println!("  Path: {}", store_path.display());

    Ok(())
}
