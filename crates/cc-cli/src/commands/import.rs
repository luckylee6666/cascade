use anyhow::Result;
use colored::Colorize;
use std::io::Read;

pub fn execute(files: &[String], group: Option<&str>, dry_run: bool, overwrite: bool) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;
    let repo = cc_store::config_repo::ConfigRepo::new(&store);

    if files.is_empty() {
        anyhow::bail!("No input files. Usage: cascade import <files...> (use `-` for stdin)");
    }

    let mut all = Vec::new();
    for f in files {
        let (text, name) = if f == "-" {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            (s, "stdin".to_string())
        } else {
            let path = std::path::Path::new(f);
            if !path.exists() {
                anyhow::bail!("File not found: {}", f);
            }
            (std::fs::read_to_string(path)?, f.clone())
        };
        let format = cc_core::import::format_from_path(&name);
        let entries = cc_core::import::parse_import_text(&text, format)?;
        println!("  parsed {}: {} entries", name.dimmed(), entries.len());
        all.extend(entries);
    }

    let secret_of = |e: &cc_core::import::ImportEntry| {
        e.secret.unwrap_or_else(|| {
            cc_core::import::is_likely_secret_key(&e.key)
                || cc_core::import::is_likely_secret_value(&e.value)
        })
    };

    if dry_run {
        let index = cc_store::import::env_key_index(&store)?;
        for e in &all {
            let secret = secret_of(e);
            let status = match cc_store::import::find_existing(&repo, &index, &e.key)? {
                Some(_) => {
                    if overwrite {
                        "overwrite".yellow()
                    } else {
                        "skip".dimmed()
                    }
                }
                None => "new".green(),
            };
            let shown = if secret {
                format!("{} {}", "••••••".dimmed(), "(secret)".red())
            } else {
                e.value.clone()
            };
            println!("  {:>10}  {} = {}", status, e.key, shown);
        }
        println!(
            "\n{} ({} entries, {} likely secrets)",
            "Dry run: nothing written".yellow(),
            all.len(),
            all.iter().filter(|e| secret_of(e)).count()
        );
        return Ok(());
    }

    let report = cc_store::import::import_entries(
        &store,
        &all,
        &cc_store::import::ImportOptions {
            group: group.map(|g| g.to_string()),
            overwrite,
        },
    )?;
    println!(
        "{}",
        format!(
            "Imported: {} added, {} updated, {} skipped, {} secrets encrypted",
            report.added, report.updated, report.skipped, report.secrets
        )
        .green()
    );
    Ok(())
}
