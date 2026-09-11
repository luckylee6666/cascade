use anyhow::Result;
use rusqlite::params;
use uuid::Uuid;
use chrono::{Utc, DateTime};

use crate::db::Store;
use cc_core::config::{Config, ConfigCreate, ConfigUpdate, ConfigHistory};

fn parse_datetime(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

pub struct ConfigRepo<'a> {
    store: &'a Store,
}

impl<'a> ConfigRepo<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub fn create(&self, input: ConfigCreate) -> Result<Config> {
        let conn = self.store.conn();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO configs (id, key, value, secret, group_name, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                input.key,
                input.value,
                input.secret as i32,
                input.group,
                input.description,
                now,
                now,
            ],
        )?;

        // Record history
        conn.execute(
            "INSERT INTO config_history (config_id, key, value, action)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, input.key, input.value, "create"],
        )?;

        Ok(Config {
            id,
            key: input.key,
            value: input.value,
            secret: input.secret,
            encrypted: None,
            group: input.group,
            description: input.description,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<Config>> {
        let conn = self.store.conn();
        let mut stmt = conn.prepare(
            "SELECT id, key, value, secret, encrypted, group_name, description, created_at, updated_at
             FROM configs WHERE id = ?1"
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            Ok(Config {
                id: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
                secret: row.get::<_, i32>(3)? != 0,
                encrypted: row.get(4)?,
                group: row.get(5)?,
                description: row.get(6)?,
                created_at: parse_datetime(row.get(7)?),
                updated_at: parse_datetime(row.get(8)?),
            })
        })?;

        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_by_key(&self, key: &str) -> Result<Option<Config>> {
        let conn = self.store.conn();
        let mut stmt = conn.prepare(
            "SELECT id, key, value, secret, encrypted, group_name, description, created_at, updated_at
             FROM configs WHERE key = ?1"
        )?;

        let mut rows = stmt.query_map(params![key], |row| {
            Ok(Config {
                id: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
                secret: row.get::<_, i32>(3)? != 0,
                encrypted: row.get(4)?,
                group: row.get(5)?,
                description: row.get(6)?,
                created_at: parse_datetime(row.get(7)?),
                updated_at: parse_datetime(row.get(8)?),
            })
        })?;

        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list(&self, group: Option<&str>) -> Result<Vec<Config>> {
        let conn = self.store.conn();
        let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(g) = group {
            (
                "SELECT id, key, value, secret, encrypted, group_name, description, created_at, updated_at
                 FROM configs WHERE group_name = ?1 ORDER BY key",
                vec![Box::new(g.to_string())],
            )
        } else {
            (
                "SELECT id, key, value, secret, encrypted, group_name, description, created_at, updated_at
                 FROM configs ORDER BY key",
                vec![],
            )
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(Config {
                id: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
                secret: row.get::<_, i32>(3)? != 0,
                encrypted: row.get(4)?,
                group: row.get(5)?,
                description: row.get(6)?,
                created_at: parse_datetime(row.get(7)?),
                updated_at: parse_datetime(row.get(8)?),
            })
        })?;

        let mut configs = Vec::new();
        for row in rows {
            configs.push(row?);
        }
        Ok(configs)
    }

    pub fn update(&self, id: &str, input: ConfigUpdate) -> Result<Config> {
        let now = Utc::now().to_rfc3339();
        {
            let conn = self.store.conn();
            conn.execute(
                "UPDATE configs SET
                    value = COALESCE(?2, value),
                    secret = COALESCE(?3, secret),
                    group_name = COALESCE(?4, group_name),
                    description = COALESCE(?5, description),
                    updated_at = ?6
                 WHERE id = ?1",
                params![
                    id,
                    input.value,
                    input.secret.map(|s| s as i32),
                    input.group,
                    input.description,
                    now,
                ],
            )?;

            // Record history
            if let Some(value) = &input.value {
                let key: String = conn.query_row(
                    "SELECT key FROM configs WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )?;

                conn.execute(
                    "INSERT INTO config_history (config_id, key, value, action)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, key, value, "update"],
                )?;
            }
        }

        self.get(id)?.ok_or_else(|| anyhow::anyhow!("Config not found"))
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let conn = self.store.conn();

        // Record history before delete
        let key: String = conn.query_row(
            "SELECT key FROM configs WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;

        conn.execute(
            "INSERT INTO config_history (config_id, key, action)
             VALUES (?1, ?2, ?3)",
            params![id, key, "delete"],
        )?;

        conn.execute("DELETE FROM configs WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn history(&self, config_id: &str, limit: i64) -> Result<Vec<ConfigHistory>> {
        crate::history_repo::HistoryRepo::new(self.store).list(config_id, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;

    #[test]
    fn test_config_crud() {
        let store = Store::open_memory().unwrap();
        let repo = ConfigRepo::new(&store);

        // Create
        let config = repo.create(ConfigCreate {
            key: "database.host".into(),
            value: Some("localhost".into()),
            secret: false,
            group: Some("database".into()),
            description: None,
        }).unwrap();
        assert_eq!(config.key, "database.host");

        // Get
        let fetched = repo.get(&config.id).unwrap().unwrap();
        assert_eq!(fetched.value, Some("localhost".into()));

        // Update
        let updated = repo.update(&config.id, ConfigUpdate {
            value: Some("127.0.0.1".into()),
            secret: None,
            group: None,
            description: None,
        }).unwrap();
        assert_eq!(updated.value, Some("127.0.0.1".into()));

        // List
        let list = repo.list(None).unwrap();
        assert_eq!(list.len(), 1);

        // History
        let history = repo.history(&config.id, 10).unwrap();
        assert_eq!(history.len(), 2); // create + update

        // Delete
        repo.delete(&config.id).unwrap();
        assert!(repo.get(&config.id).unwrap().is_none());
    }
}
