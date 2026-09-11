use anyhow::Result;
use colored::Colorize;

use super::ProjectCommands;

fn find_project(
    repo: &cc_store::project_repo::ProjectRepo,
    name_or_id: &str,
) -> Result<cc_core::config::Project> {
    repo.list()?
        .into_iter()
        .find(|p| p.name == name_or_id || p.id == name_or_id)
        .ok_or_else(|| anyhow::anyhow!("Project '{}' not found", name_or_id))
}

fn find_config(
    repo: &cc_store::config_repo::ConfigRepo,
    key_or_id: &str,
) -> Result<cc_core::config::Config> {
    if let Some(c) = repo.get_by_key(key_or_id)? {
        return Ok(c);
    }
    repo.get(key_or_id)?
        .ok_or_else(|| anyhow::anyhow!("Config '{}' not found (pass the key, e.g. database.host)", key_or_id))
}

fn find_env(
    repo: &cc_store::env_repo::EnvRepo,
    name_or_id: &str,
) -> Result<cc_core::config::Environment> {
    repo.list()?
        .into_iter()
        .find(|e| e.name == name_or_id || e.id == name_or_id)
        .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", name_or_id))
}

pub fn execute(command: ProjectCommands) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::project_repo::ProjectRepo;
    let repo = ProjectRepo::new(&store);

    match command {
        ProjectCommands::Create { name, description } => {
            let project = repo.create(cc_core::config::ProjectCreate { name, description })?;
            println!("{}", format!("Created project '{}' ({})", project.name, project.id).green());
        }
        ProjectCommands::List => {
            let projects = repo.list()?;
            if projects.is_empty() {
                println!("{}", "No projects found".yellow());
                return Ok(());
            }

            println!("{:<40} {:<30} {}", "ID", "NAME", "DESCRIPTION");
            println!("{}", "-".repeat(100));

            for p in &projects {
                println!(
                    "{:<40} {:<30} {}",
                    p.id,
                    p.name,
                    p.description.as_deref().unwrap_or("-")
                );
            }
        }
        ProjectCommands::Delete { project } => {
            let found = find_project(&repo, &project)?;
            repo.delete(&found.id)?;
            println!("{}", format!("Deleted project '{}'", found.name).green());
        }
        ProjectCommands::AddConfig { project, config, env, value } => {
            let proj = find_project(&repo, &project)?;
            let cfg = find_config(&cc_store::config_repo::ConfigRepo::new(&store), &config)?;
            let environment = find_env(&cc_store::env_repo::EnvRepo::new(&store), &env)?;
            repo.add_config(
                &proj.id,
                cc_core::config::ProjectConfigCreate {
                    config_id: cfg.id.clone(),
                    env_id: environment.id.clone(),
                    override_value: value,
                },
            )?;
            println!(
                "{}",
                format!(
                    "Attached '{}' to project '{}' (env '{}')",
                    cfg.key, proj.name, environment.name
                )
                .green()
            );
        }
        ProjectCommands::RemoveConfig { project, config, env } => {
            let proj = find_project(&repo, &project)?;
            let cfg = find_config(&cc_store::config_repo::ConfigRepo::new(&store), &config)?;
            let environment = find_env(&cc_store::env_repo::EnvRepo::new(&store), &env)?;
            repo.remove_config(&proj.id, &cfg.id, &environment.id)?;
            println!(
                "{}",
                format!(
                    "Detached '{}' from project '{}' (env '{}')",
                    cfg.key, proj.name, environment.name
                )
                .green()
            );
        }
        ProjectCommands::Configs { project } => {
            let proj = find_project(&repo, &project)?;
            let configs = repo.list_configs(&proj.id)?;
            if configs.is_empty() {
                println!(
                    "{}",
                    format!("No configs attached to '{}' — attach entries to include them in exports", proj.name).yellow()
                );
                return Ok(());
            }

            let config_repo = cc_store::config_repo::ConfigRepo::new(&store);
            let env_repo = cc_store::env_repo::EnvRepo::new(&store);
            println!(
                "{:<40} {:<12} {}",
                "CONFIG", "ENV", "OVERRIDE"
            );
            println!("{}", "-".repeat(90));
            for c in &configs {
                let key = config_repo
                    .get(&c.config_id)?
                    .map(|cfg| cfg.key)
                    .unwrap_or_else(|| c.config_id.clone());
                let env = env_repo
                    .get(&c.env_id)?
                    .map(|e| e.name)
                    .unwrap_or_else(|| c.env_id.clone());
                println!(
                    "{:<40} {:<12} {}",
                    key,
                    env,
                    c.override_value.as_deref().unwrap_or("-")
                );
            }
        }
    }

    Ok(())
}
