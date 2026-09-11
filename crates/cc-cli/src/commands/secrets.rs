use anyhow::Result;
use colored::Colorize;

use super::SecretsCommands;

pub fn execute(command: SecretsCommands) -> Result<()> {
    match command {
        SecretsCommands::Init => execute_init(),
        SecretsCommands::Encrypt => execute_encrypt(),
        SecretsCommands::Ls => execute_ls(),
        SecretsCommands::Squash { yes } => execute_squash(yes),
    }
}

fn execute_init() -> Result<()> {
    let key = cc_core::crypto::get_or_create_master_key()?;
    println!("{}", "Master key initialized".green());
    println!("  Key: {}...", base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &key[..8]
    ));
    Ok(())
}

fn execute_encrypt() -> Result<()> {
    let store = cc_store::db::Store::open_default()?;
    let master_key = cc_core::crypto::get_or_create_master_key()?;

    use cc_store::config_repo::ConfigRepo;
    let repo = ConfigRepo::new(&store);

    let configs = repo.list(None)?;
    let mut encrypted = 0;

    for config in &configs {
        if config.secret && config.value.is_some() && config.encrypted.is_none() {
            let plaintext = config.value.as_ref().unwrap();
            if !plaintext.starts_with("cc-enc:v1:") {
                let encrypted_val = cc_core::crypto::encrypt(plaintext, &master_key)?;
                repo.update(&config.id, cc_core::config::ConfigUpdate {
                    value: Some(encrypted_val),
                    secret: None,
                    group: None,
                    description: None,
                })?;
                encrypted += 1;
            }
        }
    }

    println!("{} configs encrypted", encrypted);
    Ok(())
}

fn execute_ls() -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::config_repo::ConfigRepo;
    let repo = ConfigRepo::new(&store);

    let configs = repo.list(None)?;
    let secrets: Vec<_> = configs.iter().filter(|c| c.secret).collect();

    if secrets.is_empty() {
        println!("{}", "No secrets found".yellow());
        return Ok(());
    }

    println!("{:<40} {:<20} {}", "KEY", "STATUS", "VALUE");
    println!("{}", "-".repeat(80));

    for config in &secrets {
        // NOTE: encrypted values live in `value` as cc-enc:v1:... envelopes.
        // The `encrypted` column is legacy and always NULL — check the value.
        let stored = config.value.as_deref().unwrap_or_default();
        let (status, value) = if cc_core::crypto::is_encrypted(stored) {
            ("encrypted".green(), format!("{}...", &stored[..20.min(stored.len())]))
        } else {
            ("plaintext".red(), "***".to_string())
        };

        println!("{:<40} {:<20} {}", config.key, status, value);
    }

    Ok(())
}

fn execute_squash(yes: bool) -> Result<()> {
    if !yes {
        anyhow::bail!(
            "This permanently deletes ALL config history (past values become unrecoverable). Re-run with --yes to confirm."
        );
    }

    let store = cc_store::db::Store::open_default()?;
    let n = cc_store::history_repo::HistoryRepo::new(&store).clear()?;
    println!(
        "{}",
        format!(
            "Cleared {} history rows. Current values untouched; old values are unrecoverable.",
            n
        )
        .green()
    );
    Ok(())
}


