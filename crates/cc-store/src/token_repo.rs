use anyhow::Result;
use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::params;
use uuid::Uuid;

use crate::db::Store;

pub struct TokenRepo<'a> {
    store: &'a Store,
}

pub struct TokenCheck {
    pub valid: bool,
    pub permissions: String,
}

impl<'a> TokenRepo<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    /// Insert or refresh a token. `expires_at` is None = never expires.
    pub fn upsert(&self, token: &str, permissions: &str) -> Result<()> {
        let conn = self.store.conn();
        conn.execute(
            "INSERT INTO tokens (id, token, permissions) VALUES (?1, ?2, ?3)
             ON CONFLICT(token) DO UPDATE SET permissions=excluded.permissions, expires_at=NULL",
            params![Uuid::new_v4().to_string(), token, permissions],
        )?;
        Ok(())
    }

    /// Insert a token if absent, keeping existing permissions.
    /// Used at server boot: never silently upgrades a scoped token to admin.
    pub fn ensure(&self, token: &str, default_permissions: &str) -> Result<()> {
        let conn = self.store.conn();
        conn.execute(
            "INSERT INTO tokens (id, token, permissions) VALUES (?1, ?2, ?3)
             ON CONFLICT(token) DO NOTHING",
            params![Uuid::new_v4().to_string(), token, default_permissions],
        )?;
        Ok(())
    }

    pub fn revoke(&self, token: &str) -> Result<()> {
        let conn = self.store.conn();
        conn.execute("DELETE FROM tokens WHERE token = ?1", params![token])?;
        Ok(())
    }

    /// Validate a token: must exist and not be expired.
    /// Unparseable expiry fails closed (reject) — a corrupt row must not grant access.
    pub fn check(&self, token: &str) -> TokenCheck {
        let invalid = TokenCheck {
            valid: false,
            permissions: String::new(),
        };
        let conn = self.store.conn();
        let row: Result<(String, Option<String>), _> = conn
            .query_row(
                "SELECT permissions, expires_at FROM tokens WHERE token = ?1",
                params![token],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| rusqlite::Error::QueryReturnedNoRows);
        let (permissions, expires_at) = match row {
            Ok(v) => v,
            Err(_) => return invalid,
        };
        match expires_at {
            None => TokenCheck {
                valid: true,
                permissions,
            },
            Some(s) => {
                let exp = DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .or_else(|_| {
                        NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                            .map(|naive| naive.and_utc())
                    });
                match exp {
                    Ok(exp) if Utc::now() < exp => TokenCheck {
                        valid: true,
                        permissions,
                    },
                    _ => invalid,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;

    #[test]
    fn test_token_upsert_check_revoke() {
        let store = Store::open_memory().unwrap();
        let repo = TokenRepo::new(&store);

        assert!(!repo.check("nope").valid);

        repo.upsert("tok-123", "admin").unwrap();
        let check = repo.check("tok-123");
        assert!(check.valid);
        assert_eq!(check.permissions, "admin");

        // Refresh is idempotent (upsert, no duplicate error).
        repo.upsert("tok-123", "read").unwrap();
        let check = repo.check("tok-123");
        assert!(check.valid);
        assert_eq!(check.permissions, "read");

        repo.revoke("tok-123").unwrap();
        assert!(!repo.check("tok-123").valid);
    }

    #[test]
    fn test_token_ensure_keeps_existing_permissions() {
        let store = Store::open_memory().unwrap();
        let repo = TokenRepo::new(&store);

        repo.ensure("share-tok", "read").unwrap();
        assert_eq!(repo.check("share-tok").permissions, "read");

        // A later boot with --token must NOT upgrade the scoped token.
        repo.ensure("share-tok", "admin").unwrap();
        assert_eq!(repo.check("share-tok").permissions, "read");

        // Fresh tokens get the default permission.
        repo.ensure("admin-tok", "admin").unwrap();
        assert_eq!(repo.check("admin-tok").permissions, "admin");
    }

    #[test]
    fn test_token_expiry() {
        let store = Store::open_memory().unwrap();
        let repo = TokenRepo::new(&store);

        // Expired token (RFC3339, past).
        {
            let conn = store.conn();
            conn.execute(
                "INSERT INTO tokens (id, token, permissions, expires_at) VALUES (?1, ?2, ?3, ?4)",
                params!["id-1", "old-tok", "read", "2000-01-01T00:00:00+00:00"],
            )
            .unwrap();
        }
        assert!(!repo.check("old-tok").valid);

        // Future expiry (legacy SQLite CURRENT_TIMESTAMP format).
        {
            let conn = store.conn();
            conn.execute(
                "INSERT INTO tokens (id, token, permissions, expires_at) VALUES (?1, ?2, ?3, ?4)",
                params!["id-2", "legacy-tok", "read", "2999-01-01 00:00:00"],
            )
            .unwrap();
        }
        assert!(repo.check("legacy-tok").valid);
    }
}
