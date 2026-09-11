use anyhow::Result;
use colored::Colorize;

pub fn execute(key: &str, reveal: bool) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::config_repo::ConfigRepo;
    let repo = ConfigRepo::new(&store);

    match repo.get_by_key(key)? {
        Some(config) => {
            println!("Key: {}", config.key);
            let stored = config.value.unwrap_or_default();
            if config.secret {
                if reveal {
                    let master_key = cc_core::crypto::get_or_create_master_key()?;
                    let plain = if cc_core::crypto::is_encrypted(&stored) {
                        cc_core::crypto::decrypt(&stored, &master_key)?
                    } else {
                        stored
                    };
                    println!("Value: {}", plain);
                } else {
                    println!("Value: {} {}", "••••••••".dimmed(), "(secret — use --reveal)".dimmed());
                }
            } else {
                println!("Value: {}", stored);
            }
            if config.secret {
                println!("Type: {}", "secret".red());
            }
            if let Some(group) = &config.group {
                println!("Group: {}", group);
            }
            if let Some(desc) = &config.description {
                println!("Description: {}", desc);
            }
            println!("Created: {}", config.created_at);
            println!("Updated: {}", config.updated_at);
        }
        None => {
            println!("{}", format!("Key '{}' not found", key).red());
        }
    }

    Ok(())
}
