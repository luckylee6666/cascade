use anyhow::Result;
use rusqlite::params;

use crate::db::Store;
use cc_core::config::ConfigEnvValue;

pub struct EnvValueRepo<'a> {
    store: &'a Store,
}

impl<'a> EnvValueRepo<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    /// Upsert the value of one config in one environment.
    pub fn set(&self, config_id: &str, env_id: &str, value: &str) -> Result<()> {
        let conn = self.store.conn();
        conn.execute(
            "INSERT INTO config_env_values (config_id, env_id, value)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(config_id, env_id) DO UPDATE SET value=excluded.value",
            params![config_id, env_id, value],
        )?;
        Ok(())
    }

    /// Remove the env-level value; resolution falls back to base/parent envs.
    pub fn remove(&self, config_id: &str, env_id: &str) -> Result<()> {
        let conn = self.store.conn();
        conn.execute(
            "DELETE FROM config_env_values WHERE config_id = ?1 AND env_id = ?2",
            params![config_id, env_id],
        )?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<ConfigEnvValue>> {
        let conn = self.store.conn();
        let mut stmt = conn.prepare(
            "SELECT config_id, env_id, value FROM config_env_values",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ConfigEnvValue {
                config_id: row.get(0)?,
                env_id: row.get(1)?,
                value: row.get(2)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;
    use crate::config_repo::ConfigRepo;
    use crate::env_repo::EnvRepo;
    use cc_core::config::{ConfigCreate, EnvironmentCreate};

    #[test]
    fn test_env_value_crud() {
        let store = Store::open_memory().unwrap();
        let repo = EnvValueRepo::new(&store);
        let configs = ConfigRepo::new(&store);
        let envs = EnvRepo::new(&store);

        let config = configs
            .create(ConfigCreate {
                key: "db.host".into(),
                value: Some("localhost".into()),
                secret: false,
                group: None,
                description: None,
            })
            .unwrap();
        let env = envs
            .create(EnvironmentCreate {
                name: "prod".into(),
                parent_id: None,
            })
            .unwrap();

        repo.set(&config.id, &env.id, "prod.db").unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].value.as_deref(), Some("prod.db"));

        // Upsert overwrites.
        repo.set(&config.id, &env.id, "prod2.db").unwrap();
        assert_eq!(repo.list().unwrap()[0].value.as_deref(), Some("prod2.db"));

        repo.remove(&config.id, &env.id).unwrap();
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn test_env_value_fk_requires_real_config_and_env() {
        let store = Store::open_memory().unwrap();
        let repo = EnvValueRepo::new(&store);
        assert!(repo.set("nope", "nope", "v").is_err());
    }
}
