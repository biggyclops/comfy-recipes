use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub recipe: String,
    pub created_at: String,
    pub source_image_path: Option<String>,
    pub target_image_path: Option<String>,
    pub result_image_path: Option<String>,
    pub settings_json: String,
    pub prompt_id: Option<String>,
    pub swap_mode: Option<String>,
}

pub struct Database {
    conn: Mutex<Connection>,
}

fn db_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "comfyrecipes", "Comfy Recipes").map(|dirs| {
        let data_dir = dirs.data_dir();
        fs::create_dir_all(data_dir).ok();
        data_dir.join("history.db")
    })
}

fn images_dir() -> Option<PathBuf> {
    ProjectDirs::from("com", "comfyrecipes", "Comfy Recipes").map(|dirs| {
        let data_dir = dirs.data_dir();
        let images = data_dir.join("images");
        fs::create_dir_all(&images).ok();
        images
    })
}

pub fn get_images_dir() -> Result<PathBuf, String> {
    images_dir().ok_or_else(|| "Could not determine images directory".to_string())
}

impl Database {
    pub fn new() -> Result<Self, String> {
        let path = db_path().ok_or("Could not determine database path")?;
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                recipe TEXT NOT NULL,
                created_at TEXT NOT NULL,
                source_image_path TEXT,
                target_image_path TEXT,
                result_image_path TEXT,
                settings_json TEXT NOT NULL,
                prompt_id TEXT,
                swap_mode TEXT
            )",
            [],
        )
        .map_err(|e| e.to_string())?;

        Self::run_migrations(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn run_migrations(conn: &Connection) -> Result<(), String> {
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(history)")
            .map_err(|e| e.to_string())?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        if !columns.contains(&"swap_mode".to_string()) {
            conn.execute(
                "ALTER TABLE history ADD COLUMN swap_mode TEXT DEFAULT 'face_only'",
                [],
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    pub fn add_entry(
        &self,
        recipe: &str,
        source_image_path: Option<&str>,
        target_image_path: Option<&str>,
        result_image_path: Option<&str>,
        settings: &serde_json::Value,
        prompt_id: Option<&str>,
        swap_mode: Option<&str>,
    ) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now: DateTime<Utc> = Utc::now();
        let settings_json = serde_json::to_string(settings).map_err(|e| e.to_string())?;

        conn.execute(
            "INSERT INTO history (recipe, created_at, source_image_path, target_image_path, result_image_path, settings_json, prompt_id, swap_mode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                recipe,
                now.to_rfc3339(),
                source_image_path,
                target_image_path,
                result_image_path,
                settings_json,
                prompt_id,
                swap_mode,
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(conn.last_insert_rowid())
    }

    #[allow(dead_code)]
    pub fn update_result(&self, id: i64, result_image_path: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE history SET result_image_path = ?1 WHERE id = ?2",
            params![result_image_path, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_recent(&self, limit: usize) -> Result<Vec<HistoryEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, recipe, created_at, source_image_path, target_image_path, result_image_path, settings_json, prompt_id, swap_mode
                 FROM history ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;

        let entries = stmt
            .query_map([limit as i64], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    recipe: row.get(1)?,
                    created_at: row.get(2)?,
                    source_image_path: row.get(3)?,
                    target_image_path: row.get(4)?,
                    result_image_path: row.get(5)?,
                    settings_json: row.get(6)?,
                    prompt_id: row.get(7)?,
                    swap_mode: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    pub fn get_entry(&self, id: i64) -> Result<Option<HistoryEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, recipe, created_at, source_image_path, target_image_path, result_image_path, settings_json, prompt_id, swap_mode
                 FROM history WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;

        let entry = stmt
            .query_row([id], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    recipe: row.get(1)?,
                    created_at: row.get(2)?,
                    source_image_path: row.get(3)?,
                    target_image_path: row.get(4)?,
                    result_image_path: row.get(5)?,
                    settings_json: row.get(6)?,
                    prompt_id: row.get(7)?,
                    swap_mode: row.get(8)?,
                })
            })
            .ok();

        Ok(entry)
    }

    pub fn cleanup_old(&self, keep_count: usize) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;

        if count <= keep_count as i64 {
            return Ok(0);
        }

        let to_delete = count - keep_count as i64;
        
        conn.execute(
            "DELETE FROM history WHERE id IN (SELECT id FROM history ORDER BY created_at ASC LIMIT ?1)",
            [to_delete],
        )
        .map_err(|e| e.to_string())?;

        Ok(to_delete as usize)
    }
}
