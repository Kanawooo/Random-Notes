pub mod fts;
pub mod migrations;
pub mod models;

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct DbService {
    conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl DbService {
    pub fn new(db_path: PathBuf) -> Result<Self, String> {
        // 1. Read-only preflight check if file exists
        if db_path.exists() {
            let ro_conn = Connection::open_with_flags(
                &db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|e| {
                format!(
                    "DatabaseService preflight check failed: cannot open read-only: {}",
                    e
                )
            })?;

            // 1a. Integrity quick check
            let quick_check: String = ro_conn
                .query_row("PRAGMA quick_check(1);", [], |r| r.get(0))
                .map_err(|e| format!("DatabaseService preflight check failed: {}", e))?;

            if quick_check != "ok" {
                return Err(format!(
                    "DatabaseService preflight check failed: SQLite quick_check: {}",
                    quick_check
                ));
            }

            // 1b. Check if user objects exist
            let mut stmt = ro_conn
                .prepare("SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'")
                .map_err(|e| e.to_string())?;

            let user_objects: Vec<(String, String)> = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();

            if !user_objects.is_empty() {
                let has_migrations = user_objects
                    .iter()
                    .any(|(t, n)| t == "table" && n == "schema_migrations");
                if !has_migrations {
                    return Err("未知或不兼容的 SQLite 数据库：检测到已有用户数据对象，但缺少随笺迁移记录表。".to_string());
                }

                let mut m_stmt = ro_conn
                    .prepare("SELECT version FROM schema_migrations ORDER BY version ASC")
                    .map_err(|e| e.to_string())?;
                let applied: Vec<i64> = m_stmt
                    .query_map([], |r| r.get::<_, i64>(0))
                    .map_err(|e| e.to_string())?
                    .filter_map(|r| r.ok())
                    .collect();

                let other_user_objs: Vec<_> = user_objects
                    .iter()
                    .filter(|(_, n)| n != "schema_migrations")
                    .collect();
                if !other_user_objs.is_empty() && applied.is_empty() {
                    return Err("未知或不兼容的 SQLite 数据库：检测到已有外部数据对象，但随笺迁移记录为空。".to_string());
                }

                migrations::check_migrations_integrity(
                    &applied,
                    migrations::LATEST_MIGRATION_VERSION,
                )?;

                if applied.contains(&1) {
                    migrations::verify_v1_schema_contract(&ro_conn)?;
                }
            }
        }

        // Ensure parent dir
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        // 2. Open writable connection
        let mut conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(|e| e.to_string())?;

        migrations::run_migrations(&mut conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
        })
    }

    pub fn from_connection(conn: Connection, db_path: PathBuf) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
        }
    }

    pub fn get_conn(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }

    pub fn get_path(&self) -> &Path {
        &self.db_path
    }

    pub fn reopen(&self) -> Result<(), String> {
        let mut lock = self
            .conn
            .lock()
            .map_err(|_| "Failed to lock database".to_string())?;
        let new_conn = Connection::open(&self.db_path).map_err(|e| e.to_string())?;
        new_conn
            .execute_batch(
                "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
            )
            .map_err(|e| e.to_string())?;
        *lock = new_conn;
        Ok(())
    }
}
