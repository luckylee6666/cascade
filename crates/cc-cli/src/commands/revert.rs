use anyhow::Result;
use colored::Colorize;

pub fn execute(history_id: i64) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::history_repo::HistoryRepo;
    let repo = HistoryRepo::new(&store);

    repo.revert(history_id)?;
    println!("{}", format!("Reverted to history #{}", history_id).green());

    Ok(())
}


