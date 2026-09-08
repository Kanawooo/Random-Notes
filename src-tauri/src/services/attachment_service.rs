use crate::db::models::Attachment;
use crate::utils::paths::validate_uuid;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const MAX_ATTACHMENT_SIZE_BYTES: usize = 15 * 1024 * 1024; // 15MB

pub struct AttachmentService {
    conn: Arc<Mutex<Connection>>,
    attachments_dir: PathBuf,
}

impl AttachmentService {
    pub fn new(conn: Arc<Mutex<Connection>>, attachments_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&attachments_dir);
        Self {
            conn,
            attachments_dir,
        }
    }

    pub fn get_absolute_path(&self, relative_path: &str) -> PathBuf {
        self.attachments_dir.join(relative_path)
    }

    pub fn get_attachments_dir(&self) -> &Path {
        &self.attachments_dir
    }

    pub fn validate_magic_bytes(data: &[u8]) -> Option<(&'static str, &'static str)> {
        if data.len() < 4 {
            return None;
        }

        // PNG: 89 50 4E 47 0D 0A 1A 0A
        if data.len() >= 8 && data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Some(("image/png", "png"));
        }

        // JPEG: FF D8 FF
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(("image/jpeg", "jpg"));
        }

        // GIF: GIF87a or GIF89a
        if data.len() >= 6 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
            return Some(("image/gif", "gif"));
        }

        // WebP: RIFF .... WEBP
        if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
            return Some(("image/webp", "webp"));
        }

        None
    }

    pub fn save_image_bytes(&self, note_id: &str, data: &[u8]) -> Result<Attachment, String> {
        if !validate_uuid(note_id) {
            return Err("无效的 note_id".to_string());
        }

        if data.len() > MAX_ATTACHMENT_SIZE_BYTES {
            return Err("图片大小超过 15MB 上限".to_string());
        }

        let (mime_type, ext) = Self::validate_magic_bytes(data)
            .ok_or_else(|| "不支持的图片类型。仅支持 PNG、JPEG、GIF、WebP 格式。".to_string())?;

        let attach_id = uuid::Uuid::new_v4().to_string();
        let filename = format!("{}.{}", attach_id, ext);
        let target_path = self.attachments_dir.join(&filename);

        // 1. Calculate hash, dimensions, and metadata in memory BEFORE acquiring DB lock
        let mut hasher = Sha256::new();
        hasher.update(data);
        let sha256 = format!("{:x}", hasher.finalize());

        let (width, height) = match image::load_from_memory(data) {
            Ok(img) => (Some(img.width() as i64), Some(img.height() as i64)),
            Err(_) => (None, None),
        };

        let now = chrono::Utc::now().to_rfc3339();
        let byte_size = data.len() as i64;

        if !self.attachments_dir.exists() {
            fs::create_dir_all(&self.attachments_dir)
                .map_err(|e| format!("创建附件目录失败: {}", e))?;
        }

        // 2. Acquire DB lock, write file and INSERT while holding the lock
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;

        fs::write(&target_path, data).map_err(|e| format!("保存图片失败: {}", e))?;

        if let Err(e) = conn.execute(
            "INSERT INTO attachments (id, note_id, relative_path, mime_type, byte_size, width, height, sha256, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![attach_id, note_id, filename, mime_type, byte_size, width, height, sha256, now],
        ) {
            let _ = fs::remove_file(&target_path);
            return Err(format!("插入附件数据失败: {}", e));
        }

        Ok(Attachment {
            id: attach_id,
            note_id: note_id.to_string(),
            relative_path: filename,
            mime_type: mime_type.to_string(),
            byte_size,
            width,
            height,
            sha256,
            created_at: now,
        })
    }

    pub fn save_from_clipboard(&self, note_id: &str) -> Result<Attachment, String> {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| format!("剪贴板访问失败: {}", e))?;
        let img = clipboard
            .get_image()
            .map_err(|_| "剪贴板中没有图片".to_string())?;

        // Convert raw RGBA bitmap to PNG
        let width = img.width as u32;
        let height = img.height as u32;
        let mut png_bytes: Vec<u8> = Vec::new();
        {
            let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
            use image::ImageEncoder;
            encoder
                .write_image(&img.bytes, width, height, image::ExtendedColorType::Rgba8)
                .map_err(|e| format!("PNG 编码失败: {}", e))?;
        }

        self.save_image_bytes(note_id, &png_bytes)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;

        let rel_path: String = conn
            .query_row(
                "SELECT relative_path FROM attachments WHERE id = ?",
                params![id],
                |r| r.get(0),
            )
            .map_err(|_| "附件不存在或已被删除".to_string())?;

        let file_path = self.attachments_dir.join(&rel_path);
        let temp_delete_path =
            self.attachments_dir
                .join(format!("{}.del-{}.tmp", rel_path, uuid::Uuid::new_v4()));

        let file_existed = file_path.exists();
        if file_existed {
            fs::rename(&file_path, &temp_delete_path)
                .map_err(|e| format!("准备删除附件物理文件失败: {}", e))?;
        }

        // DB transaction for deletion
        let db_result = (|| -> Result<(), rusqlite::Error> {
            let tx = conn.transaction()?;
            let rows = tx.execute("DELETE FROM attachments WHERE id = ?", params![id])?;
            if rows == 0 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            tx.commit()?;
            Ok(())
        })();

        if let Err(e) = db_result {
            // Roll back file rename if it existed
            if file_existed {
                let _ = fs::rename(&temp_delete_path, &file_path);
            }
            return Err(format!("删除附件数据库记录失败: {}", e));
        }

        // Now remove the temporary deleted file
        if file_existed {
            if let Err(e) = fs::remove_file(&temp_delete_path) {
                return Err(format!("附件记录已删除，但清理临时物理文件失败: {}", e));
            }
        }

        Ok(())
    }

    pub fn list_for_note(&self, note_id: &str) -> Result<Vec<Attachment>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, note_id, relative_path, mime_type, byte_size, width, height, sha256, created_at
                 FROM attachments WHERE note_id = ?",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![note_id], |row| {
                Ok(Attachment {
                    id: row.get(0)?,
                    note_id: row.get(1)?,
                    relative_path: row.get(2)?,
                    mime_type: row.get(3)?,
                    byte_size: row.get(4)?,
                    width: row.get(5)?,
                    height: row.get(6)?,
                    sha256: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| e.to_string())?);
        }
        Ok(list)
    }

    pub fn cleanup_orphans(&self) -> Result<usize, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let mut stmt = conn
            .prepare("SELECT relative_path FROM attachments")
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;

        let db_files: std::collections::HashSet<String> = rows.filter_map(|r| r.ok()).collect();

        if !self.attachments_dir.exists() {
            return Ok(0);
        }

        let entries = fs::read_dir(&self.attachments_dir)
            .map_err(|e| format!("读取附件目录失败: {}", e))?;

        let mut removed = 0;
        let mut failed_deletions = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| format!("遍历附件目录条目失败: {}", e))?;
            let file_type = entry
                .file_type()
                .map_err(|e| format!("获取文件类型失败: {}", e))?;
            if file_type.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !db_files.contains(&name) {
                    match fs::remove_file(entry.path()) {
                        Ok(()) => removed += 1,
                        Err(e) => failed_deletions.push(format!("{}: {}", name, e)),
                    }
                }
            }
        }

        if !failed_deletions.is_empty() {
            return Err(format!(
                "清理孤立附件物理文件失败 ({} 个文件无法删除): {}",
                failed_deletions.len(),
                failed_deletions.join(", ")
            ));
        }

        Ok(removed)
    }

    pub fn resolve_attachment_file(&self, id_or_uuid: &str) -> Result<PathBuf, String> {
        if !validate_uuid(id_or_uuid) {
            return Err("无效的附件 UUID".to_string());
        }

        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let rel_path: String = conn
            .query_row(
                "SELECT relative_path FROM attachments WHERE id = ?",
                params![id_or_uuid],
                |r| r.get(0),
            )
            .map_err(|_| "附件未找到".to_string())?;

        // Guard against path traversal
        let clean_path = Path::new(&rel_path);
        if clean_path.is_absolute()
            || clean_path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
        {
            return Err("非法附件相对路径".to_string());
        }

        let full_path = self.attachments_dir.join(clean_path);
        if !full_path.exists() {
            return Err("附件物理文件不存在".to_string());
        }

        Ok(full_path)
    }
}
