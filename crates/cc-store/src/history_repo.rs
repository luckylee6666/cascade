use anyhow::Result;
use rusqlite::params;
use chrono::{Utc, DateTime};

use crate::db::Store;
use cc_core::config::{ConfigHistory, HistoryAction};

fn parse_datetime(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

pub struct HistoryRepo<'a> {
    store: &'a Store,
}

impl<'a> HistoryRepo<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub fn list(&self, config_id: &str, limit: i64) -> Result<Vec<ConfigHistory>> {
        let conn = self.store.conn();
        let mut stmt = conn.prepare(
            "SELECT id, config_id, key, value, encrypted, action, operator, created_at
             FROM config_history WHERE config_id = ?1
             ORDER BY id DESC LIMIT ?2"
        )?;

        let rows = stmt.query_map(params![config_id, limit], |row| {
            Ok(ConfigHistory {
                id: row.get(0)?,
                config_id: row.get(1)?,
                key: row.get(2)?,
                value: row.get(3)?,
                encrypted: row.get(4)?,
                action: match row.get::<_, String>(5)?.as_str() {
                    "create" => HistoryAction::Create,
                    "update" => HistoryAction::Update,
                    "delete" => HistoryAction::Delete,
                    _ => HistoryAction::Update,
                },
                operator: row.get(6)?,
                created_at: parse_datetime(row.get(7)?),
            })
        })?;

        let mut history = Vec::new();
        for row in rows {
            history.push(row?);
        }
        Ok(history)
    }

    /// Delete ALL history rows (secret hygiene: past plaintext values are
    /// unrecoverable afterwards). Returns rows removed. Current values untouched.
    pub fn clear(&self) -> Result<i64> {
        let conn = self.store.conn();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM config_history", [], |r| {
            r.get(0)
        })?;
        conn.execute("DELETE FROM config_history", [])?;
        // VACUUM reclaims the pages; cannot run inside a transaction (we aren't).
        conn.execute("VACUUM", [])?;
        Ok(n)
    }

    pub fn revert(&self, history_id: i64) -> Result<()> {
        let conn = self.store.conn();

        // Get the history entry
        let history: ConfigHistory = conn.query_row(
            "SELECT id, config_id, key, value, encrypted, action, operator, created_at
             FROM config_history WHERE id = ?1",
            params![history_id],
            |row| {
                Ok(ConfigHistory {
                    id: row.get(0)?,
                    config_id: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    encrypted: row.get(4)?,
                    action: match row.get::<_, String>(5)?.as_str() {
                        "create" => HistoryAction::Create,
                        "update" => HistoryAction::Update,
                        "delete" => HistoryAction::Delete,
                        _ => HistoryAction::Update,
                    },
                    operator: row.get(6)?,
                    created_at: parse_datetime(row.get(7)?),
                })
            },
        )?;

        // Restore the value
        if let Some(value) = &history.value {
            conn.execute(
                "UPDATE configs SET value = ?2, updated_at = CURRENT_TIMESTAMP
                 WHERE id = ?1",
                params![history.config_id, value],
            )?;

            // Record revert in history
            conn.execute(
                "INSERT INTO config_history (config_id, key, value, action)
                 VALUES (?1, ?2, ?3, ?4)",
                params![history.config_id, history.key, value, "update"],
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;
    use crate::config_repo::ConfigRepo;
    use cc_core::config::{ConfigCreate, ConfigUpdate};

    #[test]
    fn test_history_and_revert() {
        let store = Store::open_memory().unwrap();
        let config_repo = ConfigRepo::new(&store);
        let history_repo = HistoryRepo::new(&store);

        // Create config
        let config = config_repo.create(ConfigCreate {
            key: "test.key".into(),
            value: Some("value1".into()),
            secret: false,
            group: None,
            description: None,
        }).unwrap();

        // Update config
        config_repo.update(&config.id, ConfigUpdate {
            value: Some("value2".into()),
            secret: None,
            group: None,
            description: None,
        }).unwrap();

        // Check history
        let history = history_repo.list(&config.id, 10).unwrap();
        assert_eq!(history.len(), 2); // create + update

        // Revert to first version
        history_repo.revert(history[1].id).unwrap();

        // Verify revert
        let config = config_repo.get(&config.id).unwrap().unwrap();
        assert_eq!(config.value, Some("value1".into()));
    }
}
