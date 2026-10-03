pub mod models;

use rusqlite::{Connection, OptionalExtension};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::AppError;

pub struct Database {
    pub conn: Mutex<Connection>,
}

impl Database {
    pub fn open(app_data_dir: PathBuf) -> Result<Self, AppError> {
        std::fs::create_dir_all(&app_data_dir).map_err(|e| {
            AppError::DatabaseError(format!("Cannot create data dir: {}", e))
        })?;

        let db_path = app_data_dir.join("phosphor.db");
        let conn = Connection::open(&db_path)?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS clone_log (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                source_type TEXT NOT NULL,
                source_uid  TEXT NOT NULL,
                target_type TEXT NOT NULL,
                target_uid  TEXT NOT NULL,
                port        TEXT NOT NULL,
                success     INTEGER NOT NULL DEFAULT 0,
                timestamp   TEXT NOT NULL,
                notes       TEXT
            );

            CREATE TABLE IF NOT EXISTS saved_cards (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                name              TEXT NOT NULL,
                card_type         TEXT NOT NULL,
                frequency         TEXT NOT NULL,
                uid               TEXT NOT NULL,
                raw               TEXT NOT NULL DEFAULT '',
                decoded           TEXT NOT NULL DEFAULT '{}',
                cloneable         INTEGER NOT NULL DEFAULT 1,
                recommended_blank TEXT NOT NULL DEFAULT 'T5577',
                created_at        TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS app_settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;

        Ok(Database {
            conn: Mutex::new(conn),
        })
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, AppError> {
        let conn = self.conn.lock().map_err(|e| {
            AppError::DatabaseError(format!("Database lock poisoned: {}", e))
        })?;
        let value = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), AppError> {
        let conn = self.conn.lock().map_err(|e| {
            AppError::DatabaseError(format!("Database lock poisoned: {}", e))
        })?;
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }
}
