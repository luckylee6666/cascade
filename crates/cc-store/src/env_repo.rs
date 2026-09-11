use anyhow::Result;
use rusqlite::params;
use uuid::Uuid;
use chrono::{Utc, DateTime};

use crate::db::Store;
use cc_core::config::{Environment, EnvironmentCreate};

fn parse_datetime(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

pub struct EnvRepo<'a> {
    store: &'a Store,
}

impl<'a> EnvRepo<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub fn create(&self, input: EnvironmentCreate) -> Result<Environment> {
        let conn = self.store.conn();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO environments (id, name, parent_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, input.name, input.parent_id, now],
        )?;
        drop(conn);

        // Reject cycles up front: a cyclic graph would hang get_chain().
        if let Err(e) = cc_core::config::inherit::validate_inheritance_chain(&self.list()?) {
            let _ = self.delete(&id);
            return Err(e);
        }

        Ok(Environment {
            id,
            name: input.name,
            parent_id: input.parent_id,
            created_at: Utc::now(),
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<Environment>> {
        let conn = self.store.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, created_at FROM environments WHERE id = ?1"
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            Ok(Environment {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                created_at: parse_datetime(row.get(3)?),
            })
        })?;

        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list(&self) -> Result<Vec<Environment>> {
        let conn = self.store.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, created_at FROM environments ORDER BY name"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Environment {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                created_at: parse_datetime(row.get(3)?),
            })
        })?;

        let mut envs = Vec::new();
        for row in rows {
            envs.push(row?);
        }
        Ok(envs)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let conn = self.store.conn();
        conn.execute("DELETE FROM environments WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_chain(&self, env_id: &str) -> Result<Vec<Environment>> {
        let mut chain = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut current_id = Some(env_id.to_string());

        while let Some(id) = current_id {
            // Belt-and-braces: create() rejects cycles, but hand-edited DBs
            // could still contain one. Never hang — bail instead.
            if !visited.insert(id.clone()) {
                anyhow::bail!("Circular environment inheritance detected at '{}'", id);
            }
            if let Some(env) = self.get(&id)? {
                current_id = env.parent_id.clone();
                chain.push(env);
            } else {
                break;
            }
        }

        chain.reverse();
        Ok(chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;

    #[test]
    fn test_env_crud() {
        let store = Store::open_memory().unwrap();
        let repo = EnvRepo::new(&store);

        let env = repo.create(EnvironmentCreate {
            name: "dev".into(),
            parent_id: None,
        }).unwrap();
        assert_eq!(env.name, "dev");

        let fetched = repo.get(&env.id).unwrap().unwrap();
        assert_eq!(fetched.name, "dev");

        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);

        repo.delete(&env.id).unwrap();
        assert!(repo.get(&env.id).unwrap().is_none());
    }

    #[test]
    fn test_env_chain() {
        let store = Store::open_memory().unwrap();
        let repo = EnvRepo::new(&store);

        let base = repo.create(EnvironmentCreate {
            name: "base".into(),
            parent_id: None,
        }).unwrap();

        let prod = repo.create(EnvironmentCreate {
            name: "prod".into(),
            parent_id: Some(base.id.clone()),
        }).unwrap();

        let chain = repo.get_chain(&prod.id).unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].name, "base");
        assert_eq!(chain[1].name, "prod");
    }

    #[test]
    fn test_chain_cycle_bails_instead_of_hanging() {
        use rusqlite::params;
        let store = Store::open_memory().unwrap();
        let repo = EnvRepo::new(&store);

        // Hand-edit a cycle via raw SQL (create() would reject it).
        // Insert parentless first (FK), then wire the cycle with UPDATE.
        {
            let conn = store.conn();
            conn.execute(
                "INSERT INTO environments (id, name) VALUES ('a', 'a'), ('b', 'b')",
                params![],
            )
            .unwrap();
            conn.execute("UPDATE environments SET parent_id='b' WHERE id='a'", params![])
                .unwrap();
            conn.execute("UPDATE environments SET parent_id='a' WHERE id='b'", params![])
                .unwrap();
        }

        let err = repo.get_chain("a").unwrap_err();
        assert!(err.to_string().contains("Circular"));
    }
}
