use crate::services::attachment_service::AttachmentService;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

pub struct PurgeService {
    conn: Arc<Mutex<Connection>>,
    attachment_service: Arc<AttachmentService>,
}

impl PurgeService {
    pub fn new(conn: Arc<Mutex<Connection>>, attachment_service: Arc<AttachmentService>) -> Self {
        Self {
            conn,
            attachment_service,
        }
    }

    pub fn purge_deleted_notes_older_than_days(&self, days: i64) -> Result<usize, String> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
        let cutoff_str = cutoff.to_rfc3339();

        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        let note_ids: Vec<String> = {
            let mut stmt = tx
                .prepare("SELECT id FROM notes WHERE deleted_at IS NOT NULL AND deleted_at < ?")
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(params![cutoff_str], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;

            rows.filter_map(|r| r.ok()).collect()
        };

        if note_ids.is_empty() {
            return Ok(0);
        }

        let purged_count = note_ids.len();
        for id in &note_ids {
            tx.execute("DELETE FROM notes WHERE id = ?", params![id])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM notes_fts WHERE note_id = ?", params![id])
                .map_err(|e| e.to_string())?;
        }

        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);

        // Cleanup orphaned attachments
        self.attachment_service
            .cleanup_orphans()
            .map_err(|e| format!("清理过期便签成功，但清理孤儿附件失败: {}", e))?;

        Ok(purged_count)
    }
}
