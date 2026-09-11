use anyhow::Result;
use serde::Serialize;

use crate::config_repo::ConfigRepo;
use crate::db::Store;
use cc_core::config::{ConfigCreate, ConfigUpdate};
use cc_core::import::{is_likely_secret_key, is_likely_secret_value, ImportEntry};

#[derive(Debug, Default, Clone, Serialize)]
pub struct ImportReport {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
    pub secrets: usize,
}

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub group: Option<String>,
    pub overwrite: bool,
}

/// The flattened env-var identity of a key: `database.pool_size` and
/// `database.pool.size` both map to `DATABASE_POOL_SIZE`.
fn env_form(key: &str) -> String {
    key.replace(['.', '-'], "_").to_ascii_uppercase()
}

/// Index of every existing key by its env-var identity.
pub fn env_key_index(store: &Store) -> Result<std::collections::HashMap<String, String>> {
    Ok(ConfigRepo::new(store)
        .list(None)?
        .into_iter()
        .map(|c| (env_form(&c.key), c.key))
        .collect())
}

/// The config a key would land on: exact key first, then env-form alias
/// (`database.pool.size` → existing `database.pool_size`).
pub fn find_existing(
    repo: &ConfigRepo,
    index: &std::collections::HashMap<String, String>,
    key: &str,
) -> Result<Option<cc_core::config::Config>> {
    if let Some(c) = repo.get_by_key(key)? {
        return Ok(Some(c));
    }
    match index.get(&env_form(key)) {
        Some(real) if real != key => repo.get_by_key(real),
        _ => Ok(None),
    }
}

/// Import entries into the config pool.
///
/// - Existing keys: skipped unless `overwrite`. An entry also aliases to an
///   existing key with the same env-var identity (so a dotenv round-trip of
///   `database.pool_size` → `DATABASE_POOL_SIZE` → back does not create a
///   duplicate `database.pool.size`).
/// - Secrets: explicit flag wins; otherwise key/value heuristics apply.
///   Secret values are encrypted before they touch the database.
pub fn import_entries(
    store: &Store,
    entries: &[ImportEntry],
    opts: &ImportOptions,
) -> Result<ImportReport> {
    let repo = ConfigRepo::new(store);
    let mut report = ImportReport::default();
    let mut master_key: Option<[u8; 32]> = None;

    // env-var identity → existing real key
    let by_env = env_key_index(store)?;

    for e in entries {
        if e.key.trim().is_empty() {
            continue;
        }
        let secret = e
            .secret
            .unwrap_or_else(|| is_likely_secret_key(&e.key) || is_likely_secret_value(&e.value));

        let stored_value = if secret {
            if master_key.is_none() {
                master_key = Some(cc_core::crypto::get_or_create_master_key()?);
            }
            cc_core::crypto::encrypt(&e.value, &master_key.expect("key initialised"))?
        } else {
            e.value.clone()
        };

        let existing = find_existing(&repo, &by_env, &e.key)?;

        match existing {
            Some(existing) => {
                if opts.overwrite {
                    repo.update(
                        &existing.id,
                        ConfigUpdate {
                            value: Some(stored_value),
                            secret: Some(secret),
                            group: opts.group.clone(),
                            description: None,
                        },
                    )?;
                    report.updated += 1;
                } else {
                    report.skipped += 1;
                    continue;
                }
            }
            None => {
                repo.create(ConfigCreate {
                    key: e.key.clone(),
                    value: Some(stored_value),
                    secret,
                    group: opts.group.clone(),
                    description: None,
                })?;
                report.added += 1;
            }
        }
        if secret {
            report.secrets += 1;
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, value: &str) -> ImportEntry {
        ImportEntry {
            key: key.into(),
            value: value.into(),
            secret: None,
        }
    }

    #[test]
    fn test_import_add_skip_overwrite() {
        let store = Store::open_memory().unwrap();
        let repo = ConfigRepo::new(&store);
        let opts = ImportOptions {
            group: Some("imported".into()),
            overwrite: false,
        };

        let report =
            import_entries(&store, &[entry("db.host", "a"), entry("db.port", "1")], &opts)
                .unwrap();
        assert_eq!((report.added, report.skipped, report.updated), (2, 0, 0));

        // Second run: both exist → skipped.
        let report =
            import_entries(&store, &[entry("db.host", "b"), entry("db.port", "2")], &opts)
                .unwrap();
        assert_eq!((report.added, report.skipped, report.updated), (0, 2, 0));
        assert_eq!(repo.get_by_key("db.host").unwrap().unwrap().value.as_deref(), Some("a"));

        // Overwrite run updates values and group.
        let report = import_entries(
            &store,
            &[entry("db.host", "b")],
            &ImportOptions {
                group: Some("imported".into()),
                overwrite: true,
            },
        )
        .unwrap();
        assert_eq!((report.added, report.skipped, report.updated), (0, 0, 1));
        let cfg = repo.get_by_key("db.host").unwrap().unwrap();
        assert_eq!(cfg.value.as_deref(), Some("b"));
        assert_eq!(cfg.group.as_deref(), Some("imported"));
    }

    #[test]
    fn test_import_encrypts_detected_secrets() {
        let store = Store::open_memory().unwrap();
        let repo = ConfigRepo::new(&store);

        let report = import_entries(
            &store,
            &[
                entry("api.key", "sk-demo-12345"),
                entry("database.host", "localhost"),
            ],
            &ImportOptions {
                group: None,
                overwrite: false,
            },
        )
        .unwrap();
        assert_eq!(report.secrets, 1);

        let secret_cfg = repo.get_by_key("api.key").unwrap().unwrap();
        assert!(secret_cfg.secret);
        assert!(cc_core::crypto::is_encrypted(
            secret_cfg.value.as_deref().unwrap()
        ));

        let plain_cfg = repo.get_by_key("database.host").unwrap().unwrap();
        assert!(!plain_cfg.secret);
        assert_eq!(plain_cfg.value.as_deref(), Some("localhost"));
    }

    #[test]
    fn test_import_aliases_existing_underscore_keys() {
        let store = Store::open_memory().unwrap();
        let repo = ConfigRepo::new(&store);
        repo.create(ConfigCreate {
            key: "database.pool_size".into(),
            value: Some("10".into()),
            secret: false,
            group: None,
            description: None,
        })
        .unwrap();

        // The importer converts DATABASE_POOL_SIZE → database.pool.size; it must
        // alias back to the existing database.pool_size, not duplicate it.
        let report = import_entries(
            &store,
            &[entry("database.pool.size", "20")],
            &ImportOptions {
                group: None,
                overwrite: false,
            },
        )
        .unwrap();
        assert_eq!(report.skipped, 1);
        assert_eq!(repo.list(None).unwrap().len(), 1);

        let report = import_entries(
            &store,
            &[entry("database.pool.size", "20")],
            &ImportOptions {
                group: None,
                overwrite: true,
            },
        )
        .unwrap();
        assert_eq!(report.updated, 1);
        assert_eq!(
            repo.get_by_key("database.pool_size")
                .unwrap()
                .unwrap()
                .value
                .as_deref(),
            Some("20")
        );
        assert!(repo.get_by_key("database.pool.size").unwrap().is_none());
    }
}
