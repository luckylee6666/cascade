use anyhow::Result;
use rusqlite::params;
use uuid::Uuid;
use chrono::{Utc, DateTime};

use crate::db::Store;
use cc_core::config::{Project, ProjectCreate, ProjectConfig, ProjectConfigCreate};

fn parse_datetime(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

pub struct ProjectRepo<'a> {
    store: &'a Store,
}

impl<'a> ProjectRepo<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub fn create(&self, input: ProjectCreate) -> Result<Project> {
        let conn = self.store.conn();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO projects (id, name, description, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, input.name, input.description, now],
        )?;

        Ok(Project {
            id,
            name: input.name,
            description: input.description,
            created_at: Utc::now(),
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<Project>> {
        let conn = self.store.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, created_at FROM projects WHERE id = ?1"
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: parse_datetime(row.get(3)?),
            })
        })?;

        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list(&self) -> Result<Vec<Project>> {
        let conn = self.store.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, created_at FROM projects ORDER BY name"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: parse_datetime(row.get(3)?),
            })
        })?;

        let mut projects = Vec::new();
        for row in rows {
            projects.push(row?);
        }
        Ok(projects)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let conn = self.store.conn();
        conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn add_config(&self, project_id: &str, input: ProjectConfigCreate) -> Result<ProjectConfig> {
        let conn = self.store.conn();

        conn.execute(
            "INSERT INTO project_configs (project_id, config_id, env_id, override_value)
             VALUES (?1, ?2, ?3, ?4)",
            params![project_id, input.config_id, input.env_id, input.override_value],
        )?;

        Ok(ProjectConfig {
            project_id: project_id.to_string(),
            config_id: input.config_id,
            env_id: input.env_id,
            override_value: input.override_value,
            override_encrypted: None,
        })
    }

    pub fn remove_config(&self, project_id: &str, config_id: &str, env_id: &str) -> Result<()> {
        let conn = self.store.conn();
        conn.execute(
            "DELETE FROM project_configs WHERE project_id = ?1 AND config_id = ?2 AND env_id = ?3",
            params![project_id, config_id, env_id],
        )?;
        Ok(())
    }

    pub fn list_configs(&self, project_id: &str) -> Result<Vec<ProjectConfig>> {
        let conn = self.store.conn();
        let mut stmt = conn.prepare(
            "SELECT project_id, config_id, env_id, override_value, override_encrypted
             FROM project_configs WHERE project_id = ?1"
        )?;

        let rows = stmt.query_map(params![project_id], |row| {
            Ok(ProjectConfig {
                project_id: row.get(0)?,
                config_id: row.get(1)?,
                env_id: row.get(2)?,
                override_value: row.get(3)?,
                override_encrypted: row.get(4)?,
            })
        })?;

        let mut configs = Vec::new();
        for row in rows {
            configs.push(row?);
        }
        Ok(configs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;
    use cc_core::config::{Environment, EnvironmentCreate};
    use crate::env_repo::EnvRepo;

    #[test]
    fn test_project_crud() {
        let store = Store::open_memory().unwrap();
        let repo = ProjectRepo::new(&store);

        let project = repo.create(ProjectCreate {
            name: "my-project".into(),
            description: Some("A test project".into()),
        }).unwrap();
        assert_eq!(project.name, "my-project");

        let fetched = repo.get(&project.id).unwrap().unwrap();
        assert_eq!(fetched.name, "my-project");

        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);

        repo.delete(&project.id).unwrap();
        assert!(repo.get(&project.id).unwrap().is_none());
    }

    #[test]
    fn test_project_configs() {
        let store = Store::open_memory().unwrap();
        let project_repo = ProjectRepo::new(&store);
        let env_repo = EnvRepo::new(&store);
        let config_repo = crate::config_repo::ConfigRepo::new(&store);

        // Create env
        let env = env_repo.create(EnvironmentCreate {
            name: "dev".into(),
            parent_id: None,
        }).unwrap();

        // Create project
        let project = project_repo.create(ProjectCreate {
            name: "my-project".into(),
            description: None,
        }).unwrap();

        // Create a real config (FK constraint requires it)
        let config = config_repo.create(cc_core::config::ConfigCreate {
            key: "test.key".into(),
            value: Some("base-value".into()),
            secret: false,
            group: None,
            description: None,
        }).unwrap();

        // Add config
        let pc = project_repo.add_config(&project.id, ProjectConfigCreate {
            config_id: config.id.clone(),
            env_id: env.id.clone(),
            override_value: Some("value-1".into()),
        }).unwrap();
        assert_eq!(pc.project_id, project.id);

        // List configs
        let configs = project_repo.list_configs(&project.id).unwrap();
        assert_eq!(configs.len(), 1);

        // Remove config
        project_repo.remove_config(&project.id, &config.id, &env.id).unwrap();
        let configs = project_repo.list_configs(&project.id).unwrap();
        assert_eq!(configs.len(), 0);
    }
}
