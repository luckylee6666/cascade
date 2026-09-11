use anyhow::Result;
use colored::Colorize;

pub fn execute(key: &str, limit: i64) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::config_repo::ConfigRepo;
    use cc_store::history_repo::HistoryRepo;

    let config_repo = ConfigRepo::new(&store);
    let history_repo = HistoryRepo::new(&store);

    let config = config_repo.get_by_key(key)?
        .ok_or_else(|| anyhow::anyhow!("Key '{}' not found", key))?;

    let history = history_repo.list(&config.id, limit)?;

    if history.is_empty() {
        println!("{}", "No history found".yellow());
        return Ok(());
    }

    println!("{:<8} {:<10} {:<40} {:<30}", "ID", "ACTION", "VALUE", "CREATED");
    println!("{}", "-".repeat(90));

    for h in &history {
        let action_str = match h.action {
            cc_core::config::HistoryAction::Create => "CREATE".green(),
            cc_core::config::HistoryAction::Update => "UPDATE".yellow(),
            cc_core::config::HistoryAction::Delete => "DELETE".red(),
        };

        println!(
            "{:<8} {:<10} {:<40} {:<30}",
            h.id,
            action_str,
            h.value.as_deref().unwrap_or("-"),
            h.created_at.to_rfc3339()
        );
    }

    Ok(())
}


