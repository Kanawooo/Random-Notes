use crate::db::models::Attachment;
use crate::utils::paths::validate_uuid;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const MAX_ATTACHMENT_SIZE_BYTES: usize = 30 * 1024 * 1024; // 30MB

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
            return Err("图片大小超过 30MB 上限".to_string());
        }

        let (mime_type, ext) = Self::validate_magic_bytes(data)
            .ok_or_else(|| "不支持的图片类型。仅支持 PNG、JPEG、GIF、WebP 格式。".to_string())?;

        let attach_id = uuid::Uuid::new_v4().to_string();
        let filename = format!("{}.{}", attach_id, ext);
        let target_path = self.attachments_dir.join(&filename);
        // 与目标同目录同卷的临时名（uuid 前缀保证唯一），锁内 rename 才是原子操作
        let temp_path = self.attachments_dir.join(format!("{}.tmp", filename));

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

        // 2. 锁外写盘：30MB 写文件不持库锁，列表/搜索等 DB 操作不被写盘阻塞；
        //    写失败（磁盘写满等）立即清理半成品 .tmp（清理失败则由启动时清理兑底）
        fs::write(&temp_path, data).map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            format!("保存图片失败: {}", e)
        })?;

        // 3. 锁内 rename + INSERT：与 cleanup_orphans/恢复目录交换等持锁观察者串行。
        //    同段事务内，持锁观察者看到的引用（DB 行 ↔ 物理文件）要么都不存在、要么都已就位，
        //    消除“行已提交但文件被并发删除”的不自愈撕裂（图片永久 404）
        let conn = self.conn.lock().map_err(|_| {
            let _ = fs::remove_file(&temp_path);
            "Database lock failed".to_string()
        })?;

        if let Err(e) = fs::rename(&temp_path, &target_path) {
            let _ = fs::remove_file(&temp_path);
            return Err(format!("保存图片失败: {}", e));
        }

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
        let img = clipboard.get_image().map_err(|e| match e {
            arboard::Error::ContentNotAvailable => "剪贴板中没有图片".to_string(),
            other => format!("读取剪贴板图片失败: {}", other),
        })?;

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

    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Attachment> {
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
    }

    /// 导出用：单条 SQL 取全量附件行，按 note_id、rowid 排序，保证每个便签组内顺序为 rowid 升序
    pub fn list_all(&self) -> Result<Vec<Attachment>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, note_id, relative_path, mime_type, byte_size, width, height, sha256, created_at
                 FROM attachments ORDER BY note_id, rowid",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], Self::map_row)
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| e.to_string())?);
        }
        Ok(list)
    }

    /// 导出预扫描用：单条 SQL 取全量附件摘要（relative_path, byte_size），不做 N+1
    pub fn list_all_summary(&self) -> Result<Vec<(String, i64)>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let mut stmt = conn
            .prepare("SELECT relative_path, byte_size FROM attachments")
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
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

        let db_files: std::collections::HashSet<String> = rows
            .collect::<Result<std::collections::HashSet<String>, _>>()
            .map_err(|e| e.to_string())?;

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
                // `.tmp` 是 save_image_bytes 锁外写盘的暂存文件（可能正被写入），
                // 不参与孤儿判定；过期残余由启动时的 cleanup_stale_temp_files 清理
                if !db_files.contains(&name) && !name.ends_with(".tmp") {
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

    /// 启动兜底：清理写入崩溃/断电残留的 `.tmp` 碎片（写入路径见 save_image_bytes）。
    /// `.tmp` 从不被数据库引用（rename 完成后才 INSERT），因此按修改时间清理不会误删已提交附件；
    /// 保留 1 小时窗口，避免误清正在写入的临时文件
    pub fn cleanup_stale_temp_files(&self) -> Result<usize, String> {
        if !self.attachments_dir.exists() {
            return Ok(0);
        }

        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let entries = fs::read_dir(&self.attachments_dir)
            .map_err(|e| format!("读取附件目录失败: {}", e))?;

        let mut removed = 0;
        for entry in entries {
            let entry = entry.map_err(|e| format!("遍历附件目录条目失败: {}", e))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".tmp") {
                continue;
            }
            // 元数据/时间戳读取失败时保守跳过，留给下次启动
            let is_stale = entry
                .metadata()
                .and_then(|m| m.modified())
                .map(|modified| modified < cutoff)
                .unwrap_or(false);
            if is_stale {
                fs::remove_file(entry.path())
                    .map_err(|e| format!("清理过期临时附件文件失败: {}", e))?;
                removed += 1;
            }
        }

        Ok(removed)
    }

    /// 启动自愈：清理恢复崩溃残留的 `.restore-tmp-*` 暂存目录，并在附件目录缺失/不完整时改回旧副本。
    /// 以数据库引用为事实源判断 `attachments.old-*` 是「过期副本（可删）」还是「数据库仍依赖的救援副本（必须改回）」
    pub fn reconcile_restore_artifacts(&self) -> Result<usize, String> {
        let user_dir = match self.attachments_dir.parent() {
            Some(dir) => dir,
            None => return Ok(0),
        };

        let mut cleaned = 0usize;
        let mut old_dirs: Vec<PathBuf> = Vec::new();

        if user_dir.exists() {
            let entries =
                fs::read_dir(user_dir).map_err(|e| format!("读取用户数据目录失败: {}", e))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("遍历用户数据目录条目失败: {}", e))?;
                let file_type = entry
                    .file_type()
                    .map_err(|e| format!("获取文件类型失败: {}", e))?;
                if !file_type.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(".restore-tmp-") {
                    // 纯暂存目录：任何崩溃窗口下删除都无数据风险
                    fs::remove_dir_all(entry.path())
                        .map_err(|e| format!("清理恢复暂存目录失败: {}", e))?;
                    cleaned += 1;
                } else if name.starts_with("attachments.old-") {
                    old_dirs.push(entry.path());
                }
            }
        }

        if old_dirs.is_empty() {
            return Ok(cleaned);
        }

        let db_refs: std::collections::HashSet<String> = {
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
            rows.collect::<Result<std::collections::HashSet<String>, _>>()
                .map_err(|e| e.to_string())?
        };

        let refs_available_in =
            |dir: &Path| -> bool { db_refs.iter().all(|p| dir.join(p).exists()) };

        if refs_available_in(&self.attachments_dir) {
            // 数据库引用的附件都已在当前目录就位：old 目录只是崩溃残留的过期副本，可安全删除
            for dir in &old_dirs {
                fs::remove_dir_all(dir).map_err(|e| format!("清理过期附件旧目录失败: {}", e))?;
                cleaned += 1;
            }
            return Ok(cleaned);
        }

        // 当前目录不满足全部引用：崩溃可能停在「旧目录已改名、新目录未装入」窗口。
        // 以数据库引用为事实源，找到数据库仍依赖的救援副本并改回原位
        if let Some(index) = old_dirs.iter().position(|dir| refs_available_in(dir)) {
            let rescue_dir = old_dirs.remove(index);
            if self.attachments_dir.exists() {
                fs::remove_dir_all(&self.attachments_dir)
                    .map_err(|e| format!("清理候选附件目录失败: {}", e))?;
            }
            fs::rename(&rescue_dir, &self.attachments_dir)
                .map_err(|e| format!("恢复附件目录失败: {}", e))?;
            for dir in &old_dirs {
                fs::remove_dir_all(dir)
                    .map_err(|e| format!("清理多余附件旧目录失败: {}", e))?;
                cleaned += 1;
            }
        }
        // 找不到与数据库引用完全匹配的旧目录：数据不足以判定，保持原状交由人工检查

        Ok(cleaned)
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
