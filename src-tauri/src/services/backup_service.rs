use crate::db::fts::reindex_all;
use crate::db::models::{
    BackupAttachmentItem, BackupExportResult, BackupFileEntry, BackupInspectResult, BackupManifest,
    BackupNoteData, BackupRestoreResult, BackupTagItem, Note, Tag,
};
use crate::db::DbService;
use crate::services::attachment_service::{AttachmentService, MAX_ATTACHMENT_SIZE_BYTES};
use crate::services::notes_service::NotesService;
use crate::services::tags_service::TagsService;
use crate::utils::paths::{get_user_data_dir, validate_uuid};
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

pub const BACKUP_FORMAT_VERSION: i64 = 1;
pub const MAX_ARCHIVE_FILE_SIZE: u64 = 8 * 1024 * 1024 * 1024; // 8GiB
pub const MAX_TOTAL_DECOMPRESSED_SIZE: u64 = 8 * 1024 * 1024 * 1024; // 8GiB
// 单条目护栏 = 附件上限×2：自家导出附件永远够不着，同时保住读取路径整条目进内存的内存上限
pub const MAX_SINGLE_ENTRY_SIZE: usize = MAX_ATTACHMENT_SIZE_BYTES * 2;
pub const MAX_ENTRY_COUNT: usize = 100_000;

pub struct TempDirGuard(PathBuf);

impl TempDirGuard {
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

pub struct BackupService {
    db_service: Arc<DbService>,
    notes_service: Arc<NotesService>,
    tags_service: Arc<TagsService>,
    attachment_service: Arc<AttachmentService>,
}

impl BackupService {
    pub fn new(
        db_service: Arc<DbService>,
        notes_service: Arc<NotesService>,
        tags_service: Arc<TagsService>,
        attachment_service: Arc<AttachmentService>,
    ) -> Self {
        Self {
            db_service,
            notes_service,
            tags_service,
            attachment_service,
        }
    }

    fn escape_html(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }

    fn sanitize_hex_color(c: &str) -> String {
        let trimmed = c.trim();
        if trimmed.len() == 7
            && trimmed.starts_with('#')
            && trimmed[1..].chars().all(|ch| ch.is_ascii_hexdigit())
        {
            trimmed.to_string()
        } else {
            "#4D6F9F".to_string()
        }
    }

    fn validate_zip_entry_path(name: &str) -> Result<(), String> {
        if name.starts_with('/') || name.starts_with('\\') {
            return Err(format!("路径以斜杠开头 (非法路径): {}", name));
        }
        if name.contains("..") {
            return Err(format!(
                "路径包含父目录指示符 '..' (非法路径穿越): {}",
                name
            ));
        }
        if name.contains(':') {
            return Err(format!("路径包含驱动器或流分隔符 ':' (非法路径): {}", name));
        }
        let p = Path::new(name);
        for component in p.components() {
            match component {
                std::path::Component::Prefix(_) => {
                    return Err(format!("检测到非法路径前缀: {}", name))
                }
                std::path::Component::RootDir => {
                    return Err(format!("检测到非法根路径组件: {}", name))
                }
                std::path::Component::ParentDir => {
                    return Err(format!("检测到非法父目录组件: {}", name))
                }
                std::path::Component::CurDir | std::path::Component::Normal(_) => {}
            }
        }
        Ok(())
    }

    pub fn validate_attachment_relative_path(rel: &str) -> bool {
        if rel.contains('/') || rel.contains('\\') || rel.contains("..") || rel.contains(':') {
            return false;
        }
        let parts: Vec<&str> = rel.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return false;
        }
        let (ext, uuid_part) = (parts[0].to_lowercase(), parts[1]);
        if !["png", "jpg", "jpeg", "gif", "webp"].contains(&ext.as_str()) {
            return false;
        }
        validate_uuid(uuid_part)
    }

    /// 短持锁阶段：VACUUM INTO 生成一致性 DB 快照（自带事务视图一致性，无需 wal_checkpoint；
    /// bundled SQLite 3.48 支持，目标文件必须不存在，故用 uuid 临时名）
    fn create_db_snapshot_locked(&self, conn: &rusqlite::Connection) -> Result<PathBuf, String> {
        let user_data_dir = self
            .db_service
            .get_path()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(get_user_data_dir);
        let backups_dir = user_data_dir.join("backups");
        fs::create_dir_all(&backups_dir).map_err(|e| format!("创建备份目录失败: {}", e))?;
        let snapshot_path = backups_dir.join(format!(".safety-db-{}.tmp", uuid::Uuid::new_v4()));
        // 绑参传文件名：SQLite 字符串字面量不支持反斜杠转义（唯一转义是 ''），
        // 路径含单引号（如用户名 O'Brien）时拼串形式直接语法错、恢复被整体阻断
        if let Err(e) = conn.execute("VACUUM INTO ?1", params![snapshot_path.to_string_lossy()]) {
            let _ = fs::remove_file(&snapshot_path);
            return Err(format!("VACUUM INTO 生成数据库快照失败: {}", e));
        }
        Ok(snapshot_path)
    }

    /// 锁外慢阶段：读快照 DB 文件 + 附件目录 → 打包 safety zip（tmp+rename 原子落盘）→ 删临时快照。
    /// zip 条目名保持 suijian.db + attachments/<filename> 与原实现一致（人工兑底恢复路径依赖此结构）
    fn package_safety_snapshot(&self, db_snapshot: &Path) -> Result<PathBuf, String> {
        let user_data_dir = self
            .db_service
            .get_path()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(get_user_data_dir);
        let backups_dir = user_data_dir.join("backups");
        fs::create_dir_all(&backups_dir).map_err(|e| format!("创建备份目录失败: {}", e))?;
        let safety_file_name = format!(
            "safety-backup-{}.zip",
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        );
        let safety_tmp_path = backups_dir.join(format!(".{}.tmp", safety_file_name));
        let safety_final_path = backups_dir.join(&safety_file_name);

        let result = (|| -> Result<(), String> {
            let file = File::create(&safety_tmp_path)
                .map_err(|e| format!("创建安全快照临时文件失败: {}", e))?;
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            let db_bytes = fs::read(db_snapshot)
                .map_err(|e| format!("读取数据库快照失败: {}", e))?;
            zip.start_file("suijian.db", options)
                .map_err(|e| format!("写入安全快照数据库条目失败: {}", e))?;
            zip.write_all(&db_bytes)
                .map_err(|e| format!("写入安全快照数据库内容失败: {}", e))?;

            let current_attachments_dir = self.attachment_service.get_attachments_dir();
            if current_attachments_dir.exists() {
                let entries = fs::read_dir(&current_attachments_dir)
                    .map_err(|e| format!("读取附件目录失败: {}", e))?;
                for entry in entries {
                    let entry = entry.map_err(|e| format!("读取附件条目失败: {}", e))?;
                    let ft = entry
                        .file_type()
                        .map_err(|e| format!("获取附件类型失败: {}", e))?;
                    if ft.is_file() {
                        let file_name = entry.file_name().to_string_lossy().to_string();
                        let file_data = fs::read(entry.path())
                            .map_err(|e| format!("读取物理附件文件 {} 失败: {}", file_name, e))?;
                        let zip_entry_name = format!("attachments/{}", file_name);
                        zip.start_file(&zip_entry_name, options)
                            .map_err(|e| format!("写入安全快照附件条目失败: {}", e))?;
                        zip.write_all(&file_data)
                            .map_err(|e| format!("写入安全快照附件内容失败: {}", e))?;
                    }
                }
            }

            zip.finish()
                .map_err(|e| format!("安全快照压缩打包完成失败: {}", e))?;
            fs::rename(&safety_tmp_path, &safety_final_path)
                .map_err(|e| format!("安全快照文件重命名失败: {}", e))?;
            Ok(())
        })();

        let _ = fs::remove_file(db_snapshot);
        if let Err(e) = result {
            // 与 export_backup 失败分支删 .export-*.tmp 同构：资源创建者自行清理，非新增机制
            let _ = fs::remove_file(&safety_tmp_path);
            return Err(e);
        }

        // 轮转：文件名含时间戳，字典序即时间序，保留最新 5 个，失败仅记日志不影响恢复流程
        if let Ok(entries) = fs::read_dir(&backups_dir) {
            let mut safeties: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.file_name()
                        .map(|n| {
                            let n = n.to_string_lossy();
                            n.starts_with("safety-backup-") && n.ends_with(".zip")
                        })
                        .unwrap_or(false)
                })
                .collect();
            safeties.sort();
            if safeties.len() > 5 {
                for old in &safeties[..safeties.len() - 5] {
                    if let Err(e) = fs::remove_file(old) {
                        eprintln!("[warn] 清理陈旧 safety 快照失败 {:?}: {}", old, e);
                    }
                }
            }
        }

        Ok(safety_final_path)
    }

    fn render_html_export(note: &Note, tags: &[Tag]) -> String {
        let tag_spans = tags
            .iter()
            .map(|t| {
                let safe_color = Self::sanitize_hex_color(&t.color);
                format!(
                    "<span class=\"tag\" style=\"background-color:{};\">{}</span>",
                    safe_color,
                    Self::escape_html(&t.name)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");

        format!(
            "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n<style>\nbody {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; max-width: 800px; margin: 40px auto; padding: 0 20px; color: #292A27; background: #FFFDF7; }}\nh1 {{ border-bottom: 2px solid #ECE9E0; padding-bottom: 10px; margin-bottom: 10px; }}\n.meta {{ color: #73756E; font-size: 0.9em; margin-bottom: 24px; }}\n.tag {{ display: inline-block; padding: 2px 8px; border-radius: 4px; color: #fff; font-size: 0.85em; margin-right: 6px; }}\n.content {{ white-space: pre-wrap; word-break: break-word; }}\n</style>\n</head>\n<body>\n<h1>{}</h1>\n<div class=\"meta\">\n<div>创建时间: {} | 更新时间: {}</div>\n<div style=\"margin-top: 8px;\">{}</div>\n</div>\n<div class=\"content\">{}</div>\n</body>\n</html>",
            Self::escape_html(&note.title),
            Self::escape_html(&note.title),
            note.created_at,
            note.updated_at,
            tag_spans,
            Self::escape_html(&note.plain_text)
        )
    }

    pub fn export_backup(&self, destination_path: &Path) -> Result<BackupExportResult, String> {
        let parent_dir = destination_path
            .parent()
            .ok_or_else(|| "目标文件路径无效".to_string())?;
        fs::create_dir_all(parent_dir).map_err(|e| format!("创建备份目标目录失败: {}", e))?;

        // 导出预扫描：读取侧护栏不放松导出侧，任何“能导出、不能恢复”的包在此拦截。
        // 条目数按 zip 实际条目计（每条便签写 JSON+HTML 两条），与读取侧 archive.len() 同一口径
        let (note_count, note_content_bytes) = self.notes_service.count_and_size()?;
        let attachment_summaries = self.attachment_service.list_all_summary()?;
        let attachment_bytes: u64 = attachment_summaries.iter().map(|(_, size)| *size as u64).sum();

        let zip_entry_count = note_count
            .saturating_mul(2)
            .saturating_add(attachment_summaries.len())
            .saturating_add(1);
        if zip_entry_count > MAX_ENTRY_COUNT {
            return Err(format!(
                "备份条目数 {}（便签 {} ×2 + 附件 {} + 清单 1）超过上限 {}，请分批备份或清理数据",
                zip_entry_count,
                note_count,
                attachment_summaries.len(),
                MAX_ENTRY_COUNT
            ));
        }

        for (relative_path, byte_size) in &attachment_summaries {
            if *byte_size > MAX_SINGLE_ENTRY_SIZE as i64 {
                return Err(format!(
                    "附件 {}（{:.1}MB）超过单条目上限 {:.1}MB，请先压缩该图片",
                    relative_path,
                    *byte_size as f64 / (1024.0 * 1024.0),
                    MAX_SINGLE_ENTRY_SIZE as f64 / (1024.0 * 1024.0)
                ));
            }
        }

        if note_content_bytes + attachment_bytes > MAX_TOTAL_DECOMPRESSED_SIZE {
            return Err(format!(
                "备份解压总量约 {:.1}GB 超过上限 {:.0}GB，请分批备份",
                (note_content_bytes + attachment_bytes) as f64 / (1024.0 * 1024.0 * 1024.0),
                MAX_TOTAL_DECOMPRESSED_SIZE as f64 / (1024.0 * 1024.0 * 1024.0)
            ));
        }

        let temp_export_path = parent_dir.join(format!(".export-{}.tmp", uuid::Uuid::new_v4()));

        let tags = self.tags_service.list()?;
        // 分页拉取导出：避免把全部便签（含 content_json）一次性读进内存
        const EXPORT_PAGE_SIZE: usize = 500;
        let mut exported_note_count = 0usize;

        // 附件一次查询后按便签分组，避免逐便签查询的 N+1
        let mut attachments_by_note: HashMap<String, Vec<_>> = HashMap::new();
        for att in self.attachment_service.list_all()? {
            attachments_by_note
                .entry(att.note_id.clone())
                .or_default()
                .push(att);
        }

        let mut manifest_files: Vec<BackupFileEntry> = Vec::new();
        let mut total_attachments = 0;
        let attach_dir = self.attachment_service.get_attachments_dir();

        let export_result: Result<(), String> = (|| {
            let file = File::create(&temp_export_path)
                .map_err(|e| format!("创建临时备份文件失败: {}", e))?;
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

            // 先固定 ID 快照再分批取正文：OFFSET 分页在导出期间并发置顶/编辑/删除时会重排，
            // 导致条目重复（zip 重名，导出失败）或静默漏读；ID 快照后每篇至多读一次、集合固定
            let note_ids = self.notes_service.list_ids(crate::db::models::NoteScope::All)?;
            for chunk in note_ids.chunks(EXPORT_PAGE_SIZE) {
                let notes = self.notes_service.list_by_ids(chunk)?;

                for note in &notes {
                    let note_tags = note.tags.clone().unwrap_or_default();
                    let note_attachments = attachments_by_note.remove(&note.id).unwrap_or_default();
                    total_attachments += note_attachments.len();

                    let backup_tags: Vec<BackupTagItem> = note_tags
                        .iter()
                        .map(|t| BackupTagItem {
                            id: t.id.clone(),
                            name: t.name.clone(),
                            color: Self::sanitize_hex_color(&t.color),
                            created_at: Some(t.created_at.clone()),
                        })
                        .collect();

                    let backup_attachments: Vec<BackupAttachmentItem> = note_attachments
                        .iter()
                        .map(|a| BackupAttachmentItem {
                            id: a.id.clone(),
                            relative_path: a.relative_path.clone(),
                            mime_type: a.mime_type.clone(),
                            byte_size: a.byte_size,
                            width: a.width,
                            height: a.height,
                            sha256: a.sha256.clone(),
                            created_at: a.created_at.clone(),
                        })
                        .collect();

                    let backup_note = BackupNoteData {
                        id: note.id.clone(),
                        title: note.title.clone(),
                        content_json: note.content_json.clone(),
                        plain_text: note.plain_text.clone(),
                        title_manually_edited: note.title_manually_edited,
                        is_pinned: note.is_pinned,
                        archived_at: note.archived_at.clone(),
                        deleted_at: note.deleted_at.clone(),
                        revision: note.revision,
                        created_at: note.created_at.clone(),
                        updated_at: note.updated_at.clone(),
                        tags: backup_tags,
                        attachments: Some(backup_attachments.clone()),
                    };

                    // Write JSON
                    let json_bytes =
                        serde_json::to_vec_pretty(&backup_note).map_err(|e| e.to_string())?;
                    let json_path = format!("notes/{}.json", note.id);
                    zip.start_file(&json_path, options)
                        .map_err(|e| e.to_string())?;
                    zip.write_all(&json_bytes).map_err(|e| e.to_string())?;

                    let mut h_json = Sha256::new();
                    h_json.update(&json_bytes);
                    manifest_files.push(BackupFileEntry {
                        path: json_path,
                        byte_size: json_bytes.len(),
                        sha256: format!("{:x}", h_json.finalize()),
                    });

                    // Write HTML
                    let html_str = Self::render_html_export(note, &note_tags);
                    let html_bytes = html_str.as_bytes();
                    let html_path = format!("notes/{}.html", note.id);
                    zip.start_file(&html_path, options)
                        .map_err(|e| e.to_string())?;
                    zip.write_all(html_bytes).map_err(|e| e.to_string())?;

                    let mut h_html = Sha256::new();
                    h_html.update(html_bytes);
                    manifest_files.push(BackupFileEntry {
                        path: html_path,
                        byte_size: html_bytes.len(),
                        sha256: format!("{:x}", h_html.finalize()),
                    });

                    // Write attachment files into assets/ with strict validation
                    for att in backup_attachments {
                        if !Self::validate_attachment_relative_path(&att.relative_path) {
                            return Err(format!(
                                "导出失败: 附件 {} 的登记路径不符合规范（附件数据可能被外部修改过），请检查数据目录后重试",
                                att.relative_path
                            ));
                        }

                        let physical_path = attach_dir.join(&att.relative_path);
                        if !physical_path.exists() {
                            return Err(format!(
                                "导出失败: 数据库登记的附件物理文件缺失: {}",
                                att.relative_path
                            ));
                        }

                        let data = fs::read(&physical_path).map_err(|e| {
                            format!("导出失败: 读取附件 {} 失败: {}", att.relative_path, e)
                        })?;

                        if data.len() as i64 != att.byte_size {
                            return Err(format!(
                                "导出失败: 附件 {} 大小与数据库记录不符 (实际: {}, 记录: {})",
                                att.relative_path,
                                data.len(),
                                att.byte_size
                            ));
                        }

                        let mut h_asset = Sha256::new();
                        h_asset.update(&data);
                        let actual_sha = format!("{:x}", h_asset.finalize());
                        if !actual_sha.eq_ignore_ascii_case(&att.sha256) {
                            return Err(format!(
                                "导出失败: 附件 {} 哈希校验不匹配",
                                att.relative_path
                            ));
                        }

                        if AttachmentService::validate_magic_bytes(&data).map(|(m, _)| m)
                            != Some(&att.mime_type)
                        {
                            return Err(format!(
                                "导出失败: 附件 {} MIME 类型校验失败",
                                att.relative_path
                            ));
                        }

                        let asset_path = format!("assets/{}", att.relative_path);
                        zip.start_file(&asset_path, options)
                            .map_err(|e| e.to_string())?;
                        zip.write_all(&data).map_err(|e| e.to_string())?;

                        manifest_files.push(BackupFileEntry {
                            path: asset_path,
                            byte_size: data.len(),
                            sha256: actual_sha,
                        });
                    }
                }
                exported_note_count += notes.len();
            }

            // 收尾校验：快照固定后，快照内 id 应恰好各写入一次；数量不符说明导出期间数据发生了变化
            if exported_note_count != note_ids.len() {
                return Err(format!(
                    "导出期间数据发生变化（快照 {} 篇，实际写入 {} 篇），请重试导出",
                    note_ids.len(),
                    exported_note_count
                ));
            }

            // 单条目精确复核：与读取侧逐条目解压上限同口径（同一常量、同一 > 判界），
            // 防止单条便签序列化后超限产出“导出成功、恢复被拒”的包
            for f in &manifest_files {
                if f.byte_size > MAX_SINGLE_ENTRY_SIZE {
                    return Err(format!(
                        "条目 {}（{:.1}MB）超过单条目上限 {:.1}MB，请精简该便签内容后重新导出",
                        f.path,
                        f.byte_size as f64 / (1024.0 * 1024.0),
                        MAX_SINGLE_ENTRY_SIZE as f64 / (1024.0 * 1024.0)
                    ));
                }
            }

            // 精确总量复核：预扫描是下界估计（未计 HTML 渲染与 JSON 序列化开销），此处按 manifest
            // 实际 byte_size 汇总，与读取侧 total_decompressed 同口径；超限走既有错误路径清理临时文件
            let manifest_total_bytes: u64 = manifest_files.iter().map(|f| f.byte_size as u64).sum();
            if manifest_total_bytes > MAX_TOTAL_DECOMPRESSED_SIZE {
                return Err(format!(
                    "备份解压总量 {:.1}GB 超过上限 {:.0}GB，请分批备份",
                    manifest_total_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                    MAX_TOTAL_DECOMPRESSED_SIZE as f64 / (1024.0 * 1024.0 * 1024.0)
                ));
            }

            // Write manifest.json
            let manifest = BackupManifest {
                version: BACKUP_FORMAT_VERSION,
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                exported_at: chrono::Utc::now().to_rfc3339(),
                tags: Some(
                    tags.into_iter()
                        .map(|t| BackupTagItem {
                            id: t.id,
                            name: t.name,
                            color: Self::sanitize_hex_color(&t.color),
                            created_at: Some(t.created_at),
                        })
                        .collect(),
                ),
                files: manifest_files,
            };

            let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
            zip.start_file("manifest.json", options)
                .map_err(|e| e.to_string())?;
            zip.write_all(&manifest_bytes).map_err(|e| e.to_string())?;

            zip.finish()
                .map_err(|e| format!("完成备份压缩失败: {}", e))?;

            // 预扫描按原始体量估算，压缩后仍以实际文件体积兜底复核
            let zip_size = fs::metadata(&temp_export_path)
                .map_err(|e| format!("读取备份文件大小失败: {}", e))?
                .len();
            if zip_size > MAX_ARCHIVE_FILE_SIZE {
                return Err(format!(
                    "备份文件大小 {:.2}GB 超过上限 {:.0}GB，请分批备份",
                    zip_size as f64 / (1024.0 * 1024.0 * 1024.0),
                    MAX_ARCHIVE_FILE_SIZE as f64 / (1024.0 * 1024.0 * 1024.0)
                ));
            }
            Ok(())
        })();

        if let Err(e) = export_result {
            let _ = fs::remove_file(&temp_export_path);
            return Err(e);
        }

        if destination_path.exists() {
            // 安全交换：先把旧备份改名为同目录 .bak 临时名，rename 失败时不丢旧文件
            let old_backup_path = parent_dir.join(format!(".bak-{}.tmp", uuid::Uuid::new_v4()));
            fs::rename(destination_path, &old_backup_path).map_err(|e| {
                let _ = fs::remove_file(&temp_export_path);
                format!("保存最终备份文件失败: {}", e)
            })?;

            match fs::rename(&temp_export_path, destination_path) {
                Ok(()) => {
                    let _ = fs::remove_file(&old_backup_path);
                }
                Err(e) => {
                    let _ = fs::rename(&old_backup_path, destination_path);
                    let _ = fs::remove_file(&temp_export_path);
                    return Err(format!("保存最终备份文件失败: {}", e));
                }
            }
        } else {
            fs::rename(&temp_export_path, destination_path).map_err(|e| {
                let _ = fs::remove_file(&temp_export_path);
                format!("保存最终备份文件失败: {}", e)
            })?;
        }

        Ok(BackupExportResult {
            canceled: false,
            file_path: Some(destination_path.to_string_lossy().to_string()),
            note_count: Some(exported_note_count),
            attachment_count: Some(total_attachments),
        })
    }

    /// Inspect and strictly validate a backup archive without touching current data
    pub fn inspect_backup(&self, backup_zip_path: &Path) -> Result<BackupInspectResult, String> {
        let zip_file =
            File::open(backup_zip_path).map_err(|e| format!("无法打开备份文件: {}", e))?;
        let metadata = zip_file.metadata().map_err(|e| e.to_string())?;
        if metadata.len() > MAX_ARCHIVE_FILE_SIZE {
            return Err(format!("备份文件大小超过 {}GB 上限", MAX_ARCHIVE_FILE_SIZE / (1024 * 1024 * 1024)));
        }

        let mut archive =
            ZipArchive::new(zip_file).map_err(|e| format!("无效的 ZIP 压缩包: {}", e))?;
        if archive.len() > MAX_ENTRY_COUNT {
            return Err(format!("备份包内文件数超过 {} 上限", MAX_ENTRY_COUNT));
        }

        // 1. First pass: extract manifest
        let mut manifest_data: Option<BackupManifest> = None;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name().to_string();

            Self::validate_zip_entry_path(&name)?;

            if name == "manifest.json" {
                if file.size() > MAX_SINGLE_ENTRY_SIZE as u64 {
                    return Err(format!("manifest.json 大小超过 {}MB 上限", MAX_SINGLE_ENTRY_SIZE / (1024 * 1024)));
                }
                let mut content = Vec::new();
                let mut buf = [0u8; 65536];
                let mut total_read = 0usize;
                loop {
                    let n = file.read(&mut buf).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    total_read += n;
                    if total_read > MAX_SINGLE_ENTRY_SIZE {
                        return Err(format!("manifest.json 解压大小超过 {}MB 上限", MAX_SINGLE_ENTRY_SIZE / (1024 * 1024)));
                    }
                    content.extend_from_slice(&buf[..n]);
                }
                let manifest: BackupManifest = serde_json::from_slice(&content)
                    .map_err(|e| format!("manifest.json 解析失败: {}", e))?;
                if manifest.version != BACKUP_FORMAT_VERSION {
                    return Err(format!("不支持的备份格式版本: {}", manifest.version));
                }
                manifest_data = Some(manifest);
                break;
            }
        }

        let manifest =
            manifest_data.ok_or_else(|| "备份包缺少 manifest.json 清单文件".to_string())?;

        let mut manifest_map: HashMap<String, BackupFileEntry> = HashMap::new();
        for f in &manifest.files {
            if manifest_map.insert(f.path.clone(), f.clone()).is_some() {
                return Err(format!(
                    "manifest.json 中包含重复的文件条目路径: {}",
                    f.path
                ));
            }
        }

        let mut seen_zip_files = HashSet::new();
        let mut seen_note_ids = HashSet::new();
        let mut seen_att_ids = HashSet::new();
        let mut seen_att_rel_paths = HashSet::new();

        // Referenced assets mapped to (byte_size, sha256, mime_type)
        let mut referenced_assets: HashMap<String, (i64, String, String)> = HashMap::new();
        // 实际 assets 条目只保留元数据（长度、sha256、魔数 mime），避免把全部解压字节留在内存
        let mut seen_asset_meta: HashMap<String, (usize, String, Option<String>)> = HashMap::new();

        let mut total_decompressed: u64 = 0;
        let mut note_count = 0;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name().to_string();

            if file.is_dir() {
                continue;
            }

            Self::validate_zip_entry_path(&name)?;

            if !seen_zip_files.insert(name.clone()) {
                return Err(format!("备份包中包含重复文件条目: {}", name));
            }

            if name == "manifest.json" {
                continue;
            }

            let expected_entry = manifest_map
                .get(&name)
                .ok_or_else(|| format!("备份包包含清单中未列出的多余文件: {}", name))?;

            // Read entry with bounded buffer
            let mut content = Vec::new();
            let mut buf = [0u8; 65536];
            let mut entry_read: usize = 0;

            loop {
                let n = file.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                entry_read += n;
                total_decompressed += n as u64;

                if entry_read > MAX_SINGLE_ENTRY_SIZE {
                    return Err(format!("条目 {} 解压大小超过 {}MB 上限", name, MAX_SINGLE_ENTRY_SIZE / (1024 * 1024)));
                }
                if total_decompressed > MAX_TOTAL_DECOMPRESSED_SIZE {
                    return Err(format!("解压总量超过 {}GB 上限 (防 Zip 炸弹)", MAX_TOTAL_DECOMPRESSED_SIZE / (1024 * 1024 * 1024)));
                }

                content.extend_from_slice(&buf[..n]);
            }

            // Verify size & hash
            if content.len() != expected_entry.byte_size {
                return Err(format!(
                    "文件 {} 大小与清单不符 (实际: {}, 清单: {})",
                    name,
                    content.len(),
                    expected_entry.byte_size
                ));
            }

            let mut hasher = Sha256::new();
            hasher.update(&content);
            let actual_hash = format!("{:x}", hasher.finalize());
            if !actual_hash.eq_ignore_ascii_case(&expected_entry.sha256) {
                return Err(format!("文件 {} 哈希校验不匹配", name));
            }

            if name.starts_with("notes/") && name.ends_with(".json") {
                let note_data: BackupNoteData = serde_json::from_slice(&content)
                    .map_err(|e| format!("解析便签数据 {} 失败: {}", name, e))?;

                if !validate_uuid(&note_data.id) {
                    return Err(format!("便签 ID 格式不是有效的 UUID: {}", note_data.id));
                }
                if !seen_note_ids.insert(note_data.id.clone()) {
                    return Err(format!("备份包中包含重复的便签 ID: {}", note_data.id));
                }

                for tag in &note_data.tags {
                    if !validate_uuid(&tag.id) {
                        return Err(format!("标签 ID 格式不是有效的 UUID: {}", tag.id));
                    }
                }

                if let Some(attachments) = &note_data.attachments {
                    for att in attachments {
                        if !validate_uuid(&att.id) {
                            return Err(format!("附件 ID 格式不是有效的 UUID: {}", att.id));
                        }
                        if !seen_att_ids.insert(att.id.clone()) {
                            return Err(format!("重复的附件 ID: {}", att.id));
                        }
                        if !seen_att_rel_paths.insert(att.relative_path.clone()) {
                            return Err(format!("重复的附件相对路径: {}", att.relative_path));
                        }
                        if !Self::validate_attachment_relative_path(&att.relative_path) {
                            return Err(format!("非法的附件相对路径格式: {}", att.relative_path));
                        }

                        let asset_key = format!("assets/{}", att.relative_path);
                        referenced_assets.insert(
                            asset_key,
                            (att.byte_size, att.sha256.clone(), att.mime_type.clone()),
                        );
                    }
                }

                note_count += 1;
            } else if name.starts_with("assets/") {
                seen_asset_meta.insert(
                    name.clone(),
                    (
                        content.len(),
                        actual_hash.clone(),
                        AttachmentService::validate_magic_bytes(&content)
                            .map(|(m, _)| m.to_string()),
                    ),
                );
            }
        }

        // Check for missing manifest files
        for manifest_path in manifest_map.keys() {
            if !seen_zip_files.contains(manifest_path) {
                return Err(format!("清单中列出的文件在备份包中缺失: {}", manifest_path));
            }
        }

        // Cross-validation: 1-to-1 match between referenced assets and actual assets
        for (ref_path, (exp_size, exp_sha, exp_mime)) in &referenced_assets {
            let (actual_size, actual_sha, actual_mime) = seen_asset_meta
                .get(ref_path)
                .ok_or_else(|| format!("便签引用的附件在备份包 assets/ 中缺失: {}", ref_path))?;

            if (*actual_size as i64) != *exp_size {
                return Err(format!(
                    "附件 {} 大小与便签记录不符 (实际: {}, 记录: {})",
                    ref_path, actual_size, exp_size
                ));
            }

            if !actual_sha.eq_ignore_ascii_case(exp_sha) {
                return Err(format!("附件 {} 哈希值与便签记录不匹配", ref_path));
            }

            if actual_mime.as_deref() != Some(exp_mime.as_str()) {
                return Err(format!("附件 {} 类型与魔数校验不匹配", ref_path));
            }
        }

        for asset_path in seen_asset_meta.keys() {
            if !referenced_assets.contains_key(asset_path) {
                return Err(format!("备份包包含未被任何便签引用的附件: {}", asset_path));
            }
        }

        let tag_count = manifest.tags.map(|t| t.len()).unwrap_or(0);
        let attachment_count = referenced_assets.len();
        let file_name = backup_zip_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string());

        Ok(BackupInspectResult {
            canceled: false,
            token: None,
            note_count: Some(note_count),
            tag_count: Some(tag_count),
            attachment_count: Some(attachment_count),
            total_byte_size: Some(metadata.len()),
            file_name,
        })
    }

    /// Restore backup using staged atomic swap with automatic rollback
    pub fn restore_backup(&self, backup_zip_path: &Path) -> Result<BackupRestoreResult, String> {
        let zip_file =
            File::open(backup_zip_path).map_err(|e| format!("无法打开备份文件: {}", e))?;
        let metadata = zip_file.metadata().map_err(|e| e.to_string())?;
        if metadata.len() > MAX_ARCHIVE_FILE_SIZE {
            return Err(format!("备份文件大小超过 {}GB 上限", MAX_ARCHIVE_FILE_SIZE / (1024 * 1024 * 1024)));
        }

        let mut archive =
            ZipArchive::new(zip_file).map_err(|e| format!("无效的 ZIP 压缩包: {}", e))?;
        if archive.len() > MAX_ENTRY_COUNT {
            return Err(format!("备份包内文件数超过 {} 上限", MAX_ENTRY_COUNT));
        }

        // 1. First pass: extract manifest
        let mut manifest_data: Option<BackupManifest> = None;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name().to_string();

            Self::validate_zip_entry_path(&name)?;

            if name == "manifest.json" {
                if file.size() > MAX_SINGLE_ENTRY_SIZE as u64 {
                    return Err(format!("manifest.json 大小超过 {}MB 上限", MAX_SINGLE_ENTRY_SIZE / (1024 * 1024)));
                }
                let mut content = Vec::new();
                let mut buf = [0u8; 65536];
                let mut total_read = 0usize;
                loop {
                    let n = file.read(&mut buf).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    total_read += n;
                    if total_read > MAX_SINGLE_ENTRY_SIZE {
                        return Err(format!("manifest.json 解压大小超过 {}MB 上限", MAX_SINGLE_ENTRY_SIZE / (1024 * 1024)));
                    }
                    content.extend_from_slice(&buf[..n]);
                }
                let manifest: BackupManifest = serde_json::from_slice(&content)
                    .map_err(|e| format!("manifest.json 解析失败: {}", e))?;
                if manifest.version != BACKUP_FORMAT_VERSION {
                    return Err(format!("不支持的备份格式版本: {}", manifest.version));
                }
                manifest_data = Some(manifest);
                break;
            }
        }

        let manifest =
            manifest_data.ok_or_else(|| "备份包缺少 manifest.json 清单文件".to_string())?;

        let mut manifest_map: HashMap<String, BackupFileEntry> = HashMap::new();
        for f in &manifest.files {
            if manifest_map.insert(f.path.clone(), f.clone()).is_some() {
                return Err(format!(
                    "manifest.json 中包含重复的文件条目路径: {}",
                    f.path
                ));
            }
        }

        // 2. Setup staged candidate directories protected by RAII TempDirGuard in user data volume
        let user_data_dir = self
            .db_service
            .get_path()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(get_user_data_dir);
        let temp_dir = TempDirGuard::new(
            user_data_dir.join(format!(".restore-tmp-{}", uuid::Uuid::new_v4())),
        );
        fs::create_dir_all(temp_dir.path()).map_err(|e| e.to_string())?;
        let candidate_attachments_dir = temp_dir.path().join("candidate_attachments");
        fs::create_dir_all(&candidate_attachments_dir).map_err(|e| e.to_string())?;

        let mut seen_zip_files = HashSet::new();
        let mut seen_note_ids = HashSet::new();
        let mut seen_att_ids = HashSet::new();
        let mut seen_att_rel_paths = HashSet::new();
        let mut referenced_assets: HashMap<String, (i64, String, String)> = HashMap::new();
        let mut seen_asset_files = HashSet::new();

        // 校验通过的便签以 NDJSON 落盘到临时目录，避免全部 BackupNoteData 在事务期常驻内存
        let notes_ndjson_path = temp_dir.path().join("notes.ndjson");
        let mut notes_writer = BufWriter::new(
            File::create(&notes_ndjson_path)
                .map_err(|e| format!("创建恢复便签暂存文件失败: {}", e))?,
        );
        let mut verified_note_count: usize = 0;
        let mut total_decompressed: u64 = 0;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name().to_string();

            if file.is_dir() {
                continue;
            }

            Self::validate_zip_entry_path(&name)?;

            if !seen_zip_files.insert(name.clone()) {
                return Err(format!("备份包中包含重复文件条目: {}", name));
            }

            if name == "manifest.json" {
                continue;
            }

            let expected_entry = manifest_map
                .get(&name)
                .ok_or_else(|| format!("备份包包含清单中未列出的多余文件: {}", name))?;

            // Read entry with bounded buffer
            let mut content = Vec::new();
            let mut buf = [0u8; 65536];
            let mut entry_read: usize = 0;

            loop {
                let n = file.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                entry_read += n;
                total_decompressed += n as u64;

                if entry_read > MAX_SINGLE_ENTRY_SIZE {
                    return Err(format!("条目 {} 解压大小超过 {}MB 上限", name, MAX_SINGLE_ENTRY_SIZE / (1024 * 1024)));
                }
                if total_decompressed > MAX_TOTAL_DECOMPRESSED_SIZE {
                    return Err(format!("解压总量超过 {}GB 上限 (防 Zip 炸弹)", MAX_TOTAL_DECOMPRESSED_SIZE / (1024 * 1024 * 1024)));
                }

                content.extend_from_slice(&buf[..n]);
            }

            // Verify size & hash
            if content.len() != expected_entry.byte_size {
                return Err(format!(
                    "文件 {} 大小与清单不符 (实际: {}, 清单: {})",
                    name,
                    content.len(),
                    expected_entry.byte_size
                ));
            }

            let mut hasher = Sha256::new();
            hasher.update(&content);
            let actual_hash = format!("{:x}", hasher.finalize());
            if !actual_hash.eq_ignore_ascii_case(&expected_entry.sha256) {
                return Err(format!("文件 {} 哈希校验不匹配", name));
            }

            if name.starts_with("notes/") && name.ends_with(".json") {
                let note_data: BackupNoteData = serde_json::from_slice(&content)
                    .map_err(|e| format!("解析便签数据 {} 失败: {}", name, e))?;

                if !validate_uuid(&note_data.id) {
                    return Err(format!("便签 ID 格式不是有效的 UUID: {}", note_data.id));
                }
                if !seen_note_ids.insert(note_data.id.clone()) {
                    return Err(format!("备份包中包含重复的便签 ID: {}", note_data.id));
                }

                for tag in &note_data.tags {
                    if !validate_uuid(&tag.id) {
                        return Err(format!("标签 ID 格式不是有效的 UUID: {}", tag.id));
                    }
                }

                if let Some(attachments) = &note_data.attachments {
                    for att in attachments {
                        if !validate_uuid(&att.id) {
                            return Err(format!("附件 ID 格式不是有效的 UUID: {}", att.id));
                        }
                        if !seen_att_ids.insert(att.id.clone()) {
                            return Err(format!("重复的附件 ID: {}", att.id));
                        }
                        if !seen_att_rel_paths.insert(att.relative_path.clone()) {
                            return Err(format!("重复的附件相对路径: {}", att.relative_path));
                        }
                        if !Self::validate_attachment_relative_path(&att.relative_path) {
                            return Err(format!("非法的附件相对路径格式: {}", att.relative_path));
                        }

                        let asset_key = format!("assets/{}", att.relative_path);
                        referenced_assets.insert(
                            asset_key,
                            (att.byte_size, att.sha256.clone(), att.mime_type.clone()),
                        );
                    }
                }

                let line = serde_json::to_vec(&note_data)
                    .map_err(|e| format!("写入恢复便签暂存文件失败: {}", e))?;
                notes_writer
                    .write_all(&line)
                    .map_err(|e| format!("写入恢复便签暂存文件失败: {}", e))?;
                notes_writer
                    .write_all(b"\n")
                    .map_err(|e| format!("写入恢复便签暂存文件失败: {}", e))?;
                verified_note_count += 1;
            } else if name.starts_with("assets/") {
                let filename = name.trim_start_matches("assets/");
                if !Self::validate_attachment_relative_path(filename) {
                    return Err(format!("非法的附件文件名: {}", filename));
                }
                let target_file = candidate_attachments_dir.join(filename);
                fs::write(&target_file, &content).map_err(|e| e.to_string())?;
                seen_asset_files.insert(name);
            }
        }

        notes_writer
            .flush()
            .map_err(|e| format!("写入恢复便签暂存文件失败: {}", e))?;
        drop(notes_writer);

        // Check for missing manifest files
        for manifest_path in manifest_map.keys() {
            if !seen_zip_files.contains(manifest_path) {
                return Err(format!("清单中列出的文件在备份包中缺失: {}", manifest_path));
            }
        }

        // Cross-validation: 1-to-1 match between referenced assets and actual candidate files
        for (ref_path, (exp_size, exp_sha, exp_mime)) in &referenced_assets {
            let filename = ref_path.trim_start_matches("assets/");
            let candidate_file = candidate_attachments_dir.join(filename);
            if !candidate_file.exists() {
                return Err(format!(
                    "便签引用的附件在备份包 assets/ 中缺失: {}",
                    ref_path
                ));
            }

            let candidate_bytes = fs::read(&candidate_file).map_err(|e| e.to_string())?;
            if candidate_bytes.len() as i64 != *exp_size {
                return Err(format!(
                    "附件 {} 大小与便签记录不符 (实际: {}, 记录: {})",
                    ref_path,
                    candidate_bytes.len(),
                    exp_size
                ));
            }

            let mut h = Sha256::new();
            h.update(&candidate_bytes);
            let actual_sha = format!("{:x}", h.finalize());
            if !actual_sha.eq_ignore_ascii_case(exp_sha) {
                return Err(format!("附件 {} 哈希校验不匹配", ref_path));
            }

            let magic_mime = AttachmentService::validate_magic_bytes(&candidate_bytes)
                .map(|(m, _)| m.to_string());
            if magic_mime.as_deref() != Some(exp_mime.as_str()) {
                return Err(format!("附件 {} MIME 类型校验失败", ref_path));
            }
        }

        for asset_path in &seen_asset_files {
            if !referenced_assets.contains_key(asset_path) {
                return Err(format!("备份包包含未引用的多余附件: {}", asset_path));
            }
        }

        // 3. 短持锁：VACUUM INTO 生成一致性 DB 快照后立即放锁。
        //    快照与恢复之间存在极小写入窗口（放锁后其他命令可写库/改附件），
        //    恢复本身为全量覆盖，窗口仅使快照内容与实际库毫秒级不一致——可接受的降级快照
        let snapshot_db = {
            let conn_arc = self.db_service.get_conn();
            let conn = conn_arc
                .lock()
                .map_err(|_| "Database lock failed".to_string())?;
            self.create_db_snapshot_locked(&conn)?
        };

        // 4. 锁外慢操作：读附件+压缩打包（原持锁打包会阻塞全部命令），失败仍中止恢复
        self.package_safety_snapshot(&snapshot_db)
            .map_err(|e| format!("生成恢复前物理安全快照失败，中止恢复: {}", e))?;

        let conn_arc = self.db_service.get_conn();
        let mut conn = conn_arc
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;

        // 5. Staged swap: switch attachments directory while holding DB lock
        let current_attachments_dir = self.attachment_service.get_attachments_dir().to_path_buf();
        let old_attachments_dir = current_attachments_dir
            .parent()
            .unwrap_or(&current_attachments_dir)
            .join(format!("attachments.old-{}", uuid::Uuid::new_v4()));

        let attachments_existed = current_attachments_dir.exists();
        if attachments_existed {
            fs::rename(&current_attachments_dir, &old_attachments_dir)
                .map_err(|e| format!("切换现有附件目录失败: {}", e))?;
        }

        // Rename candidate attachments to current attachments
        if let Err(e) = fs::rename(&candidate_attachments_dir, &current_attachments_dir) {
            if attachments_existed {
                let _ = fs::rename(&old_attachments_dir, &current_attachments_dir);
            }
            return Err(format!("安装恢复候选附件失败: {}", e));
        }

        let mut restored_tags_count = 0;
        let mut restored_attach_count = 0;
        let restored_notes_count = verified_note_count;
        // 备份可能同时含旧算法下合法共存的同名标签（如「ＡＢＣ」与「abc」，新归一算法判定同名）。
        // 恢复时按归一结果合并：冲突标签复用已插入标签的 id，note_tags 关联指向复用 id，
        // 防止 UNIQUE(normalized_name) 冲突回滚整次恢复，且便签标签关联不丢。
        let mut restored_tag_ids: HashMap<String, String> = HashMap::new();

        let tx_result: Result<(), String> = (|| {
            let tx = conn.transaction().map_err(|e| e.to_string())?;

            tx.execute("DELETE FROM note_tags", [])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM attachments", [])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM notes", [])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM tags", [])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM notes_fts", [])
                .map_err(|e| e.to_string())?;

            if let Some(tags) = &manifest.tags {
                for tag in tags {
                    let norm = TagsService::normalize_name(&tag.name);
                    if restored_tag_ids.contains_key(&norm) {
                        // 与该归一标签已恢复的条目合并：不再插入，后续便签标签关联统一指向复用 id
                        continue;
                    }
                    let safe_color = Self::sanitize_hex_color(&tag.color);
                    let created_at = tag
                        .created_at
                        .clone()
                        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
                    tx.execute(
                        "INSERT INTO tags (id, name, normalized_name, color, created_at) VALUES (?, ?, ?, ?, ?)",
                        params![tag.id, tag.name, norm, safe_color, created_at],
                    )
                    .map_err(|e| e.to_string())?;
                    restored_tag_ids.insert(norm, tag.id.clone());
                    restored_tags_count += 1;
                }
            }

            let notes_file = File::open(&notes_ndjson_path)
                .map_err(|e| format!("读取恢复便签暂存文件失败: {}", e))?;
            let notes_reader = BufReader::new(notes_file);
            for line in notes_reader.lines() {
                let line = line.map_err(|e| format!("读取恢复便签暂存文件失败: {}", e))?;
                let note: BackupNoteData = serde_json::from_str(&line)
                    .map_err(|e| format!("解析恢复便签暂存数据失败: {}", e))?;
                tx.execute(
                    "INSERT INTO notes (
                        id, title, content_json, plain_text,
                        title_manually_edited, is_pinned,
                        archived_at, deleted_at, revision,
                        created_at, updated_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        note.id,
                        note.title,
                        note.content_json,
                        note.plain_text,
                        if note.title_manually_edited { 1 } else { 0 },
                        if note.is_pinned { 1 } else { 0 },
                        note.archived_at,
                        note.deleted_at,
                        note.revision,
                        note.created_at,
                        note.updated_at
                    ],
                )
                .map_err(|e| e.to_string())?;

                for tag in &note.tags {
                    let norm = TagsService::normalize_name(&tag.name);
                    // 归一后与已恢复标签同类（含 manifest 内合并）时复用其 id；否则插入并登记，
                    // 避免 INSERT 被 UNIQUE 忽略后 note_tags 仍引用旧 id 造成关联丢失
                    let effective_tag_id = match restored_tag_ids.get(&norm) {
                        Some(existing_id) => existing_id.clone(),
                        None => {
                            let safe_color = Self::sanitize_hex_color(&tag.color);
                            let created_at = tag
                                .created_at
                                .clone()
                                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
                            tx.execute(
                                "INSERT OR IGNORE INTO tags (id, name, normalized_name, color, created_at) VALUES (?, ?, ?, ?, ?)",
                                params![tag.id, tag.name, norm, safe_color, created_at],
                            )
                            .map_err(|e| e.to_string())?;
                            restored_tag_ids.insert(norm, tag.id.clone());
                            tag.id.clone()
                        }
                    };

                    tx.execute(
                        "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?, ?)",
                        params![note.id, effective_tag_id],
                    )
                    .map_err(|e| e.to_string())?;
                }

                if let Some(attachments) = &note.attachments {
                    for att in attachments {
                        tx.execute(
                            "INSERT OR IGNORE INTO attachments (
                                id, note_id, relative_path, mime_type, byte_size, width, height, sha256, created_at
                            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                            params![
                                att.id,
                                note.id,
                                att.relative_path,
                                att.mime_type,
                                att.byte_size,
                                att.width,
                                att.height,
                                att.sha256,
                                att.created_at
                            ],
                        )
                        .map_err(|e| e.to_string())?;
                        restored_attach_count += 1;
                    }
                }
            }

            // Rebuild FTS index BEFORE committing transaction!
            reindex_all(&tx)?;

            tx.commit().map_err(|e| e.to_string())?;
            Ok(())
        })();

        if let Err(db_err) = tx_result {
            // Rollback attachments: delete candidate attachments from destination and restore old attachments
            let mut rollback_errors = Vec::new();
            if current_attachments_dir.exists() {
                if let Err(e) = fs::remove_dir_all(&current_attachments_dir) {
                    rollback_errors.push(format!("清理候选附件目录失败: {}", e));
                }
            }
            if attachments_existed {
                if let Err(e) = fs::rename(&old_attachments_dir, &current_attachments_dir) {
                    rollback_errors.push(format!("恢复原附件目录失败: {}", e));
                }
            }

            if rollback_errors.is_empty() {
                return Err(format!("恢复数据库数据失败，已自动回滚全部数据: {}", db_err));
            } else {
                return Err(format!(
                    "恢复数据库数据失败，数据库事务已回滚，但附件目录回滚遇到错误 ({}): {}",
                    db_err,
                    rollback_errors.join("; ")
                ));
            }
        }

        drop(conn);

        // Success: clean up old attachments directory
        if attachments_existed && old_attachments_dir.exists() {
            let _ = fs::remove_dir_all(&old_attachments_dir);
        }

        // Clear tracked empty draft id in NotesService
        self.notes_service.clear_tracked_empty_draft();

        Ok(BackupRestoreResult {
            canceled: false,
            restored_note_count: Some(restored_notes_count),
            restored_tag_count: Some(restored_tags_count),
            restored_attachment_count: Some(restored_attach_count),
        })
    }
}
