use crate::db::fts::{sanitize_query, sync_note_by_id};
use crate::db::models::{BatchResult, CreateNoteInput, Note, NoteScope, Tag, UpdateNoteInput};
use crate::utils::paths::validate_uuid;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::services::attachment_service::AttachmentService;

pub const DEFAULT_EMPTY_CONTENT_JSON: &str =
    "{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}";
pub const DEFAULT_NOTE_TITLE: &str = "未命名便签";

pub struct NotesService {
    conn: Arc<Mutex<Connection>>,
    tracked_empty_draft_id: Mutex<Option<String>>,
    create_lock: Mutex<()>,
    attachment_service: Arc<AttachmentService>,
}

impl NotesService {
    pub fn new(conn: Arc<Mutex<Connection>>, attachment_service: Arc<AttachmentService>) -> Self {
        Self {
            conn,
            tracked_empty_draft_id: Mutex::new(None),
            create_lock: Mutex::new(()),
            attachment_service,
        }
    }

    pub fn clear_tracked_empty_draft(&self) {
        let mut lock = self.tracked_empty_draft_id.lock().unwrap_or_else(|e| e.into_inner());
        *lock = None;
    }

    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
        Ok(Note {
            id: row.get(0)?,
            title: row.get(1)?,
            content_json: row.get(2)?,
            plain_text: row.get(3)?,
            title_manually_edited: row.get::<_, i64>(4)? != 0,
            is_pinned: row.get::<_, i64>(5)? != 0,
            archived_at: row.get(6)?,
            deleted_at: row.get(7)?,
            revision: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
            tags: None,
        })
    }

    fn attach_tags(conn: &Connection, notes: &mut [Note]) -> Result<(), String> {
        if notes.is_empty() {
            return Ok(());
        }

        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.name, t.normalized_name, t.color, t.created_at, nt.note_id
                 FROM tags t
                 JOIN note_tags nt ON t.id = nt.tag_id
                 ORDER BY t.name ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    Tag {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        normalized_name: row.get(2)?,
                        color: row.get(3)?,
                        created_at: row.get(4)?,
                    },
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut tag_map: std::collections::HashMap<String, Vec<Tag>> =
            std::collections::HashMap::new();
        for row in rows {
            let (tag, note_id) = row.map_err(|e| e.to_string())?;
            tag_map.entry(note_id).or_default().push(tag);
        }

        for note in notes.iter_mut() {
            note.tags = Some(tag_map.remove(&note.id).unwrap_or_default());
        }

        Ok(())
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<Note>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, content_json, plain_text, title_manually_edited,
                        is_pinned, archived_at, deleted_at, revision, created_at, updated_at
                 FROM notes WHERE id = ?",
            )
            .map_err(|e| e.to_string())?;

        let mut note = stmt
            .query_row(params![id], Self::map_row)
            .optional()
            .map_err(|e| e.to_string())?;

        if let Some(ref mut n) = note {
            let mut list = [n.clone()];
            Self::attach_tags(&conn, &mut list)?;
            *n = list[0].clone();
        }

        Ok(note)
    }

    pub fn create(&self, input: CreateNoteInput) -> Result<Note, String> {
        let _create_guard = self.create_lock.lock().unwrap_or_else(|e| e.into_inner());

        // Check if empty draft can be reused without holding tracked_empty_draft_id while querying DB
        if input.reuse_empty_draft == Some(true) {
            let draft_id_opt = {
                let lock = self.tracked_empty_draft_id.lock().unwrap_or_else(|e| e.into_inner());
                lock.clone()
            };

            if let Some(draft_id) = draft_id_opt {
                if let Ok(Some(existing)) = self.get_by_id(&draft_id) {
                    if existing.archived_at.is_none()
                        && existing.deleted_at.is_none()
                        && existing.plain_text.trim().is_empty()
                        && !existing.title_manually_edited
                    {
                        // Check attachments count
                        let conn = self
                            .conn
                            .lock()
                            .map_err(|_| "Database lock failed".to_string())?;
                        let attach_count: i64 = conn
                            .query_row(
                                "SELECT COUNT(*) FROM attachments WHERE note_id = ?",
                                params![draft_id],
                                |r| r.get(0),
                            )
                            .unwrap_or(0);

                        if attach_count == 0 {
                            return Ok(existing);
                        }
                    }
                }
                *self.tracked_empty_draft_id.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let title = input
            .title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_NOTE_TITLE.to_string());
        let content_json = input
            .content_json
            .unwrap_or_else(|| DEFAULT_EMPTY_CONTENT_JSON.to_string());
        let plain_text = input.plain_text.unwrap_or_default();
        let title_manually_edited = input.title_manually_edited.unwrap_or(false);
        let is_pinned = input.is_pinned.unwrap_or(false);

        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        tx.execute(
            "INSERT INTO notes (
                id, title, content_json, plain_text,
                title_manually_edited, is_pinned,
                archived_at, deleted_at, revision,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, 0, ?, ?)",
            params![
                id,
                title,
                content_json,
                plain_text,
                if title_manually_edited { 1 } else { 0 },
                if is_pinned { 1 } else { 0 },
                now,
                now
            ],
        )
        .map_err(|e| e.to_string())?;

        if let Some(tag_ids) = &input.tag_ids {
            for tag_id in tag_ids {
                // SELECT 形式：陈旧 tag_id（标签已删）静默跳过，防英文外键错误回滚本次写入
                tx.execute(
                    "INSERT OR IGNORE INTO note_tags (note_id, tag_id) SELECT ?, id FROM tags WHERE id = ?",
                    params![id, tag_id],
                )
                .map_err(|e| e.to_string())?;
            }
        }

        // FTS 索引与正文本体同一事务：索引写入失败随事务整体回滚，
        // 不再出现“正文已提交、索引缺失”的静默缺口（Transaction deref 到 Connection）
        sync_note_by_id(&tx, &id)?;

        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);

        if input.reuse_empty_draft == Some(true) {
            *self.tracked_empty_draft_id.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.clone());
        }

        self.get_by_id(&id)?
            .ok_or_else(|| "Failed to retrieve created note".to_string())
    }

    pub fn update(&self, input: UpdateNoteInput) -> Result<Note, String> {
        let existing = self
            .get_by_id(&input.id)?
            .ok_or_else(|| format!("便签不存在: {}", input.id))?;

        if existing.revision != input.expected_revision {
            return Err(format!(
                "版本冲突: 当前版本为 {}，提交版本为 {}",
                existing.revision, input.expected_revision
            ));
        }

        let title = input.title.unwrap_or(existing.title);
        let content_json = input.content_json.unwrap_or(existing.content_json);
        let plain_text = input.plain_text.unwrap_or(existing.plain_text);
        let title_manually_edited = input
            .title_manually_edited
            .unwrap_or(existing.title_manually_edited);
        let is_pinned = input.is_pinned.unwrap_or(existing.is_pinned);
        let now = chrono::Utc::now().to_rfc3339();
        let new_revision = existing.revision + 1;

        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        let rows_affected = tx
            .execute(
                "UPDATE notes
                 SET title = ?, content_json = ?, plain_text = ?,
                     title_manually_edited = ?, is_pinned = ?,
                     revision = ?, updated_at = ?
                 WHERE id = ? AND revision = ?",
                params![
                    title,
                    content_json,
                    plain_text,
                    if title_manually_edited { 1 } else { 0 },
                    if is_pinned { 1 } else { 0 },
                    new_revision,
                    now,
                    input.id,
                    existing.revision
                ],
            )
            .map_err(|e| e.to_string())?;

        if rows_affected == 0 {
            return Err(format!("版本冲突: 便签已被修改或不存在 (id: {})", input.id));
        }

        if let Some(tag_ids) = &input.tag_ids {
            tx.execute("DELETE FROM note_tags WHERE note_id = ?", params![input.id])
                .map_err(|e| e.to_string())?;
            for tag_id in tag_ids {
                // SELECT 形式：陈旧 tag_id（标签已删）静默跳过，防英文外键错误回滚本次写入
                tx.execute(
                    "INSERT OR IGNORE INTO note_tags (note_id, tag_id) SELECT ?, id FROM tags WHERE id = ?",
                    params![input.id, tag_id],
                )
                .map_err(|e| e.to_string())?;
            }
        }

        // FTS 索引与正文同一事务：索引失败触发整体回滚，不产生索引与正文不一致的中间态
        sync_note_by_id(&tx, &input.id)?;

        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);

        // If tracked draft changed, invalidate
        let mut tracked_lock = self.tracked_empty_draft_id.lock().unwrap_or_else(|e| e.into_inner());
        if tracked_lock.as_deref() == Some(&input.id)
            && (!plain_text.trim().is_empty() || title_manually_edited)
        {
            *tracked_lock = None;
        }

        drop(tracked_lock);
        self.get_by_id(&input.id)?
            .ok_or_else(|| "Failed to retrieve updated note".to_string())
    }

    fn scope_clause(scope: NoteScope) -> &'static str {
        match scope {
            NoteScope::Active => "n.deleted_at IS NULL AND n.archived_at IS NULL",
            NoteScope::Archived => "n.deleted_at IS NULL AND n.archived_at IS NOT NULL",
            NoteScope::Trash => "n.deleted_at IS NOT NULL",
            NoteScope::All => "1=1",
        }
    }

    pub fn list(&self, scope: NoteScope, limit: usize) -> Result<Vec<Note>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let clause = Self::scope_clause(scope);
        let sql = format!(
            "SELECT n.id, n.title, n.content_json, n.plain_text, n.title_manually_edited,
                    n.is_pinned, n.archived_at, n.deleted_at, n.revision, n.created_at, n.updated_at
             FROM notes n
             WHERE {}
             ORDER BY n.is_pinned DESC, n.updated_at DESC
             LIMIT ?",
            clause
        );

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit as i64], Self::map_row)
            .map_err(|e| e.to_string())?;

        let mut notes = Vec::new();
        for r in rows {
            notes.push(r.map_err(|e| e.to_string())?);
        }

        Self::attach_tags(&conn, &mut notes)?;
        Ok(notes)
    }

    /// 导出快照用：只取 id（与 list 相同的筛选与排序），导出期间并发编辑不会改变已取快照
    pub fn list_ids(&self, scope: NoteScope) -> Result<Vec<String>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let clause = Self::scope_clause(scope);
        let sql = format!(
            "SELECT n.id FROM notes n
             WHERE {}
             ORDER BY n.is_pinned DESC, n.updated_at DESC",
            clause
        );

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;

        let mut ids = Vec::new();
        for r in rows {
            ids.push(r.map_err(|e| e.to_string())?);
        }

        Ok(ids)
    }

    /// 导出快照用：按 id 分批取回完整便签，SELECT 列、map_row、attach_tags 与 list 保持一致
    pub fn list_by_ids(&self, ids: &[String]) -> Result<Vec<Note>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "SELECT n.id, n.title, n.content_json, n.plain_text, n.title_manually_edited,
                    n.is_pinned, n.archived_at, n.deleted_at, n.revision, n.created_at, n.updated_at
             FROM notes n
             WHERE n.id IN ({})",
            placeholders
        );

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(ids.iter()), Self::map_row)
            .map_err(|e| e.to_string())?;

        let mut notes = Vec::new();
        for r in rows {
            notes.push(r.map_err(|e| e.to_string())?);
        }

        Self::attach_tags(&conn, &mut notes)?;
        Ok(notes)
    }

    /// 导出预扫描用：单条聚合 SQL 返回（便签总数, 正文体量下界估计）。
    /// SQLite 的 length() 对 TEXT 返回字符数而非字节数，且未计 JSON 结构与 HTML 渲染开销；
    /// 该值仅作预扫描下界估计，打包完成后另有按 manifest 的精确复核兜底。
    /// SUM 在空表上为 NULL，用 COALESCE 归零
    pub fn count_and_size(&self) -> Result<(usize, u64), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let (count, total): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(length(content_json) + length(plain_text)), 0) FROM notes",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .map_err(|e| e.to_string())?;
        Ok((count as usize, total as u64))
    }

    pub fn search(&self, query: &str, scope: NoteScope, limit: usize) -> Result<Vec<Note>, String> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list(scope, limit);
        }

        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let clause = Self::scope_clause(scope);
        let char_count = trimmed.chars().count();

        let mut notes = Vec::new();
        if char_count >= 3 {
            // FTS5 trigram search
            let sanitized = sanitize_query(trimmed);
            let sql = format!(
                "SELECT n.id, n.title, n.content_json, n.plain_text, n.title_manually_edited,
                        n.is_pinned, n.archived_at, n.deleted_at, n.revision, n.created_at, n.updated_at
                 FROM notes_fts fts
                 JOIN notes n ON fts.note_id = n.id
                 WHERE notes_fts MATCH ? AND {}
                 ORDER BY n.is_pinned DESC, n.updated_at DESC
                 LIMIT ?",
                clause
            );
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![sanitized, limit as i64], Self::map_row)
                .map_err(|e| e.to_string())?;
            for r in rows {
                notes.push(r.map_err(|e| e.to_string())?);
            }
        } else {
            // LIKE search for short queries (1-2 chars)
            let escaped = trimmed
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            let pattern = format!("%{}%", escaped);
            let sql = format!(
                "SELECT DISTINCT n.id, n.title, n.content_json, n.plain_text, n.title_manually_edited,
                        n.is_pinned, n.archived_at, n.deleted_at, n.revision, n.created_at, n.updated_at
                 FROM notes n
                 LEFT JOIN note_tags nt ON n.id = nt.note_id
                 LEFT JOIN tags t ON nt.tag_id = t.id
                 WHERE {}
                   AND (n.title LIKE ? ESCAPE '\\' OR n.plain_text LIKE ? ESCAPE '\\' OR t.name LIKE ? ESCAPE '\\')
                 ORDER BY n.is_pinned DESC, n.updated_at DESC
                 LIMIT ?",
                clause
            );
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(
                    params![pattern, pattern, pattern, limit as i64],
                    Self::map_row,
                )
                .map_err(|e| e.to_string())?;
            for r in rows {
                notes.push(r.map_err(|e| e.to_string())?);
            }
        }

        Self::attach_tags(&conn, &mut notes)?;
        Ok(notes)
    }

    pub fn set_pin(&self, id: &str, is_pinned: bool) -> Result<Note, String> {
        {
            let conn = self
                .conn
                .lock()
                .map_err(|_| "Database lock failed".to_string())?;
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE notes SET is_pinned = ?, updated_at = ? WHERE id = ?",
                params![if is_pinned { 1 } else { 0 }, now, id],
            )
            .map_err(|e| e.to_string())?;
        }

        self.get_by_id(id)?
            .ok_or_else(|| "Note not found".to_string())
    }

    pub fn archive(&self, id: &str) -> Result<Note, String> {
        {
            let conn = self
                .conn
                .lock()
                .map_err(|_| "Database lock failed".to_string())?;
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE notes SET archived_at = ?, updated_at = ? WHERE id = ?",
                params![now, now, id],
            )
            .map_err(|e| e.to_string())?;
        }

        self.get_by_id(id)?
            .ok_or_else(|| "Note not found".to_string())
    }

    pub fn unarchive(&self, id: &str) -> Result<Note, String> {
        {
            let conn = self
                .conn
                .lock()
                .map_err(|_| "Database lock failed".to_string())?;
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE notes SET archived_at = NULL, updated_at = ? WHERE id = ?",
                params![now, id],
            )
            .map_err(|e| e.to_string())?;
        }

        self.get_by_id(id)?
            .ok_or_else(|| "Note not found".to_string())
    }

    pub fn trash(&self, id: &str) -> Result<Note, String> {
        {
            let conn = self
                .conn
                .lock()
                .map_err(|_| "Database lock failed".to_string())?;
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE notes SET deleted_at = ?, updated_at = ? WHERE id = ?",
                params![now, now, id],
            )
            .map_err(|e| e.to_string())?;
        }

        let mut tracked = self.tracked_empty_draft_id.lock().unwrap_or_else(|e| e.into_inner());
        if tracked.as_deref() == Some(id) {
            *tracked = None;
        }
        drop(tracked);

        self.get_by_id(id)?
            .ok_or_else(|| "Note not found".to_string())
    }

    pub fn restore(&self, id: &str) -> Result<Note, String> {
        {
            let conn = self
                .conn
                .lock()
                .map_err(|_| "Database lock failed".to_string())?;
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE notes SET deleted_at = NULL, updated_at = ? WHERE id = ?",
                params![now, id],
            )
            .map_err(|e| e.to_string())?;
        }

        self.get_by_id(id)?
            .ok_or_else(|| "Note not found".to_string())
    }

    pub fn trash_many(&self, ids: &[String]) -> Result<BatchResult, String> {
        if ids.is_empty() {
            return Ok(BatchResult { affected_count: 0 });
        }
        if ids.len() > 1000 {
            return Err("批量操作不能超过 1000 个便签".to_string());
        }

        let mut unique_ids: HashSet<String> = HashSet::new();
        for id in ids {
            if !validate_uuid(id) {
                return Err(format!("无效的 UUID: {}", id));
            }
            unique_ids.insert(id.clone());
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().to_rfc3339();
        let mut affected = 0;

        for id in &unique_ids {
            let count = tx
                .execute(
                    "UPDATE notes SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
                    params![now, now, id],
                )
                .map_err(|e| e.to_string())?;
            affected += count;
        }

        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);

        let mut tracked = self.tracked_empty_draft_id.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref tid) = *tracked {
            if unique_ids.contains(tid) {
                *tracked = None;
            }
        }

        Ok(BatchResult {
            affected_count: affected,
        })
    }

    pub fn delete_permanently_many(&self, ids: &[String]) -> Result<BatchResult, String> {
        if ids.is_empty() {
            return Ok(BatchResult { affected_count: 0 });
        }
        if ids.len() > 1000 {
            return Err("批量操作不能超过 1000 个便签".to_string());
        }

        let mut unique_ids: HashSet<String> = HashSet::new();
        for id in ids {
            if !validate_uuid(id) {
                return Err(format!("无效的 UUID: {}", id));
            }
            unique_ids.insert(id.clone());
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        let mut affected = 0;
        for id in &unique_ids {
            // Only permanently delete notes that are already in trash (deleted_at IS NOT NULL)
            let count = tx
                .execute(
                    "DELETE FROM notes WHERE id = ? AND deleted_at IS NOT NULL",
                    params![id],
                )
                .map_err(|e| e.to_string())?;
            if count > 0 {
                affected += count;
                tx.execute("DELETE FROM notes_fts WHERE note_id = ?", params![id])
                    .map_err(|e| e.to_string())?;
            }
        }

        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);

        let mut tracked = self.tracked_empty_draft_id.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref tid) = *tracked {
            if unique_ids.contains(tid) {
                *tracked = None;
            }
        }
        drop(tracked);

        // Physically clean up orphaned attachments after successful deletion
        self.attachment_service
            .cleanup_orphans()
            .map_err(|e| format!("便签记录已彻底删除，但清理孤儿附件失败: {}", e))?;

        Ok(BatchResult {
            affected_count: affected,
        })
    }

    pub fn empty_trash(&self) -> Result<BatchResult, String> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        let trash_ids: Vec<String> = {
            let mut stmt = tx
                .prepare("SELECT id FROM notes WHERE deleted_at IS NOT NULL")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
        };

        let affected = trash_ids.len();

        for id in &trash_ids {
            tx.execute("DELETE FROM notes WHERE id = ?", params![id])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM notes_fts WHERE note_id = ?", params![id])
                .map_err(|e| e.to_string())?;
        }

        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);

        let mut tracked = self.tracked_empty_draft_id.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref tid) = *tracked {
            if trash_ids.contains(tid) {
                *tracked = None;
            }
        }
        drop(tracked);

        // Physically clean up orphaned attachments after emptying trash
        self.attachment_service
            .cleanup_orphans()
            .map_err(|e| format!("回收站已清空，但清理孤儿附件失败: {}", e))?;

        Ok(BatchResult {
            affected_count: affected,
        })
    }
}
