use anyhow::Result;
use colored::Colorize;

use super::EnvCommands;

fn find_env(
    repo: &cc_store::env_repo::EnvRepo,
    name_or_id: &str,
) -> Result<cc_core::config::Environment> {
    repo.list()?
        .into_iter()
        .find(|e| e.name == name_or_id || e.id == name_or_id)
        .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", name_or_id))
}

pub fn execute(command: EnvCommands) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::env_repo::EnvRepo;
    let repo = EnvRepo::new(&store);

    match command {
        EnvCommands::Create { name, parent } => {
            let parent_id = match &parent {
                Some(p) => Some(find_env(&repo, p)?.id),
                None => None,
            };
            let env = repo.create(cc_core::config::EnvironmentCreate { name, parent_id })?;
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
                let parent = match &e.parent_id {
                    Some(pid) => repo
                        .get(pid)?
                        .map(|p| p.name)
                        .unwrap_or_else(|| pid.clone()),
                    None => "-".to_string(),
                };
                println!("{:<40} {:<30} {}", e.id, e.name, parent);
            }
        }
        EnvCommands::Delete { env } => {
            let found = find_env(&repo, &env)?;
            repo.delete(&found.id)?;
            println!("{}", format!("Deleted environment '{}'", found.name).green());
        }
    }

    Ok(())
}
