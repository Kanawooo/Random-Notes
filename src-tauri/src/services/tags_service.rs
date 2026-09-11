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

    /// 唯一性判定用归一化：全角字符折叠到半角（U+3000 → 空格、U+FF01–FF5E 减 0xFEE0），
    /// 再 trim + lowercase，使「ＡＢＣ」「abc」指向同一标签；既有数据不迁移，避免自动合并的删改风险。
    /// 纯 std 实现，无新增依赖
    pub fn normalize_name(name: &str) -> String {
        name.chars()
            .map(|c| match c {
                '\u{3000}' => ' ',
                '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
                _ => c,
            })
            .collect::<String>()
            .trim()
            .to_lowercase()
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
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;

        // 改名与受影响便签的 FTS 同步放在同一事务：
        // 否则同步失败会留下“标签已改、部分便签索引仍是旧名”的不一致
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        tx.execute(
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

        let tag = {
            let mut stmt = tx
                .prepare("SELECT id, name, normalized_name, color, created_at FROM tags WHERE id = ?")
                .map_err(|e| e.to_string())?;

            stmt.query_row(params![input.id], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    normalized_name: row.get(2)?,
                    color: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?
        };

        sync_notes_for_tag(&tx, &input.id)?;
        tx.commit().map_err(|e| e.to_string())?;

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
            rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
        };

        // 2. 事务内删除标签（级联清理 note_tags）并同步受影响便签的 FTS：
        // 索引同步失败随事务一起回滚，不再出现“标签关联已生效、索引仍含旧标签名”的不一致
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM tags WHERE id = ?", params![id])
            .map_err(|e| e.to_string())?;
        for note_id in affected_note_ids {
            crate::db::fts::sync_note_by_id(&tx, &note_id)?;
        }
        tx.commit().map_err(|e| e.to_string())?;

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

        // FTS 同步与标签关联同一事务：索引失败回滚整次赋值，避免“关联生效、索引陈旧”
        crate::db::fts::sync_note_by_id(&tx, &input.note_id)?;

        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
}
