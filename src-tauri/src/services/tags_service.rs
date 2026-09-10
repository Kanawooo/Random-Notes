use crate::db::fts::sync_notes_for_tag;
use crate::db::models::{AssignTagsInput, CreateTagInput, RenameTagInput, Tag};
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

pub struct TagsService {
    conn: Arc<Mutex<Connection>>,
}

impl TagsService {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn normalize_name(name: &str) -> String {
        name.trim().to_lowercase()
    }

    pub fn list(&self) -> Result<Vec<Tag>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, normalized_name, color, created_at FROM tags ORDER BY name ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    normalized_name: row.get(2)?,
                    color: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut tags = Vec::new();
        for r in rows {
            tags.push(r.map_err(|e| e.to_string())?);
        }
        Ok(tags)
    }

    pub fn create(&self, input: CreateTagInput) -> Result<Tag, String> {
        let trimmed_name = input.name.trim();
        if trimmed_name.is_empty() {
            return Err("标签名称不能为空".to_string());
        }

        let normalized = Self::normalize_name(trimmed_name);
        let color = input.color.unwrap_or_else(|| "#6366f1".to_string());
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        conn.execute(
            "INSERT INTO tags (id, name, normalized_name, color, created_at) VALUES (?, ?, ?, ?, ?)",
            params![id, trimmed_name, normalized, color, now],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                "已存在同名标签".to_string()
            } else {
                e.to_string()
            }
        })?;

        Ok(Tag {
            id,
            name: trimmed_name.to_string(),
            normalized_name: normalized,
            color,
            created_at: now,
        })
    }

    pub fn rename(&self, input: RenameTagInput) -> Result<Tag, String> {
        let trimmed_name = input.name.trim();
        if trimmed_name.is_empty() {
            return Err("标签名称不能为空".to_string());
        }

        let normalized = Self::normalize_name(trimmed_name);
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;

        conn.execute(
            "UPDATE tags SET name = ?, normalized_name = ? WHERE id = ?",
            params![trimmed_name, normalized, input.id],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                "已存在同名标签".to_string()
            } else {
                e.to_string()
            }
        })?;

        let mut stmt = conn
            .prepare("SELECT id, name, normalized_name, color, created_at FROM tags WHERE id = ?")
            .map_err(|e| e.to_string())?;

        let tag = stmt
            .query_row(params![input.id], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    normalized_name: row.get(2)?,
                    color: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;

        sync_notes_for_tag(&conn, &input.id)?;

        Ok(tag)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;

        // 1. Collect affected note IDs before deleting tag
        let affected_note_ids: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT DISTINCT note_id FROM note_tags WHERE tag_id = ?")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![id], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };

        // 2. Delete tag in transaction (cascades to note_tags)
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM tags WHERE id = ?", params![id])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;

        // 3. Sync FTS for all affected notes now that tag association is gone
        for note_id in affected_note_ids {
            crate::db::fts::sync_note_by_id(&conn, &note_id)?;
        }

        Ok(())
    }

    pub fn assign(&self, input: AssignTagsInput) -> Result<(), String> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        tx.execute(
            "DELETE FROM note_tags WHERE note_id = ?",
            params![input.note_id],
        )
        .map_err(|e| e.to_string())?;

        for tag_id in &input.tag_ids {
            // SELECT 形式：陈旧 tag_id（标签已删）静默跳过，防英文外键错误回滚本次赋标签
            tx.execute(
                "INSERT OR IGNORE INTO note_tags (note_id, tag_id) SELECT ?, id FROM tags WHERE id = ?",
                params![input.note_id, tag_id],
            )
            .map_err(|e| e.to_string())?;
        }

        tx.commit().map_err(|e| e.to_string())?;

        crate::db::fts::sync_note_by_id(&conn, &input.note_id)?;
        Ok(())
    }
}
