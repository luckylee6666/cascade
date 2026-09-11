use anyhow::Result;
use colored::Colorize;

pub fn execute(group: Option<String>) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::config_repo::ConfigRepo;
    let repo = ConfigRepo::new(&store);

    let configs = repo.list(group.as_deref())?;

    if configs.is_empty() {
        println!("{}", "No configs found".yellow());
        return Ok(());
    }

    println!("{:<40} {:<20} {}", "KEY", "GROUP", "VALUE");
    println!("{}", "-".repeat(80));

    for config in &configs {
        let value = if config.secret {
            "***".to_string()
        } else {
            config.value.clone().unwrap_or_default()
        };

        println!(
            "{:<40} {:<20} {}",
            config.key,
            config.group.as_deref().unwrap_or("-"),
            value
        );
    }

    println!("\n{} configs total", configs.len());
    Ok(())
}


