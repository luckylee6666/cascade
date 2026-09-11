use anyhow::Result;
use rusqlite::Connection;
use std::sync::Mutex;
use std::path::Path;

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Default vault location: `~/.cascade/vault.db`.
    /// `$CASCADE_VAULT` (or legacy `$CC_VAULT`) overrides it.
    /// Falls back to a legacy `~/.cc/vault.db` when the new path is absent.
    pub fn vault_path() -> Result<std::path::PathBuf> {
        if let Ok(p) = std::env::var("CASCADE_VAULT").or_else(|_| std::env::var("CC_VAULT")) {
            return Ok(std::path::PathBuf::from(p));
        }
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home dir"))?;
        let new_path = home.join(".cascade").join("vault.db");
        let legacy = home.join(".cc").join("vault.db");
        if !new_path.exists() && legacy.exists() {
            return Ok(legacy);
        }
        Ok(new_path)
    }

    pub fn open_default() -> Result<Self> {
        let path = Self::vault_path()?;
        if !path.exists() {
            anyhow::bail!("Vault not initialized at {}. Run 'cascade init' first.", path.display());
        }
        Self::open(&path)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store lock poisoned");
        conn.execute_batch(MIGRATION_SQL)?;
        Ok(())
    }

    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("store lock poisoned")
    }
}

const MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS environments (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    parent_id   TEXT REFERENCES environments(id),
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS configs (
    id          TEXT PRIMARY KEY,
    key         TEXT NOT NULL UNIQUE,
    value       TEXT,
    secret      INTEGER DEFAULT 0,
    encrypted   TEXT,
    group_name  TEXT,
    description TEXT,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- config × env value layer (a config may have different values per environment)
CREATE TABLE IF NOT EXISTS config_env_values (
    config_id   TEXT REFERENCES configs(id) ON DELETE CASCADE,
    env_id      TEXT REFERENCES environments(id) ON DELETE CASCADE,
    value       TEXT,
    PRIMARY KEY (config_id, env_id)
);

CREATE TABLE IF NOT EXISTS projects (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS project_configs (
    project_id      TEXT REFERENCES projects(id) ON DELETE CASCADE,
    config_id       TEXT REFERENCES configs(id) ON DELETE CASCADE,
    env_id          TEXT REFERENCES environments(id) ON DELETE CASCADE,
    override_value  TEXT,
    override_encrypted TEXT,
    PRIMARY KEY (project_id, config_id, env_id)
);

CREATE TABLE IF NOT EXISTS config_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    config_id   TEXT REFERENCES configs(id) ON DELETE SET NULL,
    key         TEXT NOT NULL,
    value       TEXT,
    encrypted   TEXT,
    action      TEXT NOT NULL,
    operator    TEXT,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS tokens (
    id          TEXT PRIMARY KEY,
    project_id  TEXT REFERENCES projects(id) ON DELETE CASCADE,
    token       TEXT NOT NULL UNIQUE,
    permissions TEXT DEFAULT 'read',
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    expires_at  DATETIME
);
"#;
