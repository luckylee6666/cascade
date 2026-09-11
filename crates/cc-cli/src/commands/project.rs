use anyhow::Result;
use colored::Colorize;

use super::ProjectCommands;

pub fn execute(command: ProjectCommands) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::project_repo::ProjectRepo;
    let repo = ProjectRepo::new(&store);

    match command {
        ProjectCommands::Create { name, description } => {
            let project = repo.create(cc_core::config::ProjectCreate {
                name,
                description,
            })?;
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
        ProjectCommands::Delete { id } => {
            repo.delete(&id)?;
            println!("{}", format!("Deleted project '{}'", id).green());
        }
        ProjectCommands::AddConfig { project_id, config_id, env_id, value } => {
            repo.add_config(&project_id, cc_core::config::ProjectConfigCreate {
                config_id,
                env_id,
                override_value: value,
            })?;
            println!("{}", "Config added to project".green());
        }
        ProjectCommands::RemoveConfig { project_id, config_id, env_id } => {
            repo.remove_config(&project_id, &config_id, &env_id)?;
            println!("{}", "Config removed from project".green());
        }
        ProjectCommands::Configs { project_id } => {
            let configs = repo.list_configs(&project_id)?;
            if configs.is_empty() {
                println!("{}", "No configs in project".yellow());
                return Ok(());
            }

            println!("{:<40} {:<40} {}", "CONFIG_ID", "ENV_ID", "OVERRIDE");
            println!("{}", "-".repeat(100));

            for c in &configs {
                println!(
                    "{:<40} {:<40} {}",
                    c.config_id,
                    c.env_id,
                    c.override_value.as_deref().unwrap_or("-")
                );
            }
        }
    }

    Ok(())
}


