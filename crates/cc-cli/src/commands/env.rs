use anyhow::Result;
use colored::Colorize;

use super::EnvCommands;

pub fn execute(command: EnvCommands) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::env_repo::EnvRepo;
    let repo = EnvRepo::new(&store);

    match command {
        EnvCommands::Create { name, parent } => {
            let env = repo.create(cc_core::config::EnvironmentCreate {
                name,
                parent_id: parent,
            })?;
            println!("{}", format!("Created environment '{}' ({})", env.name, env.id).green());
        }
        EnvCommands::List => {
            let envs = repo.list()?;
            if envs.is_empty() {
                println!("{}", "No environments found".yellow());
                return Ok(());
            }

            println!("{:<40} {:<30} {}", "ID", "NAME", "PARENT");
            println!("{}", "-".repeat(100));

            for e in &envs {
                println!(
                    "{:<40} {:<30} {}",
                    e.id,
                    e.name,
                    e.parent_id.as_deref().unwrap_or("-")
                );
            }
        }
        EnvCommands::Delete { id } => {
            repo.delete(&id)?;
            println!("{}", format!("Deleted environment '{}'", id).green());
        }
    }

    Ok(())
}


