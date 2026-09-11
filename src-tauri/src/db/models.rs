use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content_json: String,
    pub plain_text: String,
    pub title_manually_edited: bool,
    pub is_pinned: bool,
    pub archived_at: Option<String>,
    pub deleted_at: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
    pub tags: Option<Vec<Tag>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NoteScope {
    #[default]
    Active,
    Archived,
    Trash,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNoteInput {
    pub title: Option<String>,
    pub content_json: Option<String>,
    pub plain_text: Option<String>,
    pub title_manually_edited: Option<bool>,
    pub is_pinned: Option<bool>,
    pub tag_ids: Option<Vec<String>>,
    pub reuse_empty_draft: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNoteInput {
    pub id: String,
    #[serde(rename = "expectedRevision")]
    pub expected_revision: i64,
    pub title: Option<String>,
    pub content_json: Option<String>,
    pub plain_text: Option<String>,
    pub title_manually_edited: Option<bool>,
    pub is_pinned: Option<bool>,
    pub tag_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub normalized_name: String,
    pub color: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTagInput {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameTagInput {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignTagsInput {
    #[serde(rename = "noteId")]
    pub note_id: String,
    #[serde(rename = "tagIds")]
    pub tag_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub note_id: String,
    pub relative_path: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    #[serde(rename = "affectedCount")]
    pub affected_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub hotkey: String,
    #[serde(rename = "shortcutNewNote")]
    pub shortcut_new_note: String,
    #[serde(rename = "shortcutBackToSearch")]
    pub shortcut_back_to_search: String,
    #[serde(rename = "shortcutDismiss")]
    pub shortcut_dismiss: String,
    #[serde(rename = "launchAtLogin")]
    pub launch_at_login: bool,
    #[serde(rename = "autoHideOnBlur")]
    pub auto_hide_on_blur: bool,
    #[serde(rename = "windowBounds")]
    pub window_bounds: Option<WindowBounds>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Space".to_string(),
            shortcut_new_note: "Ctrl+N".to_string(),
            shortcut_back_to_search: "Ctrl+E".to_string(),
            shortcut_dismiss: "Escape".to_string(),
            launch_at_login: false,
            auto_hide_on_blur: false,
            window_bounds: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyStatus {
    pub registered: bool,
    #[serde(rename = "currentHotkey")]
    pub current_hotkey: String,
    #[serde(rename = "requestedHotkey")]
    pub requested_hotkey: Option<String>,
    #[serde(rename = "requestSucceeded")]
    pub request_succeeded: Option<bool>,
    pub error: Option<String>,
    #[serde(rename = "recommendedHotkey")]
    pub recommended_hotkey: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    #[serde(rename = "userDataPath")]
    pub user_data_path: String,
    #[serde(rename = "dbPath")]
    pub db_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupExportResult {
    pub canceled: bool,
    #[serde(rename = "filePath")]
    pub file_path: Option<String>,
    #[serde(rename = "noteCount")]
    pub note_count: Option<usize>,
    #[serde(rename = "attachmentCount")]
    pub attachment_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInspectResult {
    pub canceled: bool,
    pub token: Option<String>,
    #[serde(rename = "noteCount")]
    pub note_count: Option<usize>,
    #[serde(rename = "tagCount")]
    pub tag_count: Option<usize>,
    #[serde(rename = "attachmentCount")]
    pub attachment_count: Option<usize>,
    #[serde(rename = "totalByteSize")]
    pub total_byte_size: Option<u64>,
    #[serde(rename = "fileName")]
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStatus {
    #[serde(rename = "isRecovery")]
    pub is_recovery: bool,
    pub error: Option<String>,
    #[serde(rename = "dbPath")]
    pub db_path: String,
    #[serde(rename = "userDataPath")]
    pub user_data_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRestoreResult {
    pub canceled: bool,
    #[serde(rename = "restoredNoteCount")]
    pub restored_note_count: Option<usize>,
    #[serde(rename = "restoredTagCount")]
    pub restored_tag_count: Option<usize>,
    #[serde(rename = "restoredAttachmentCount")]
    pub restored_attachment_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFileEntry {
    pub path: String,
    #[serde(rename = "byteSize")]
    pub byte_size: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTagItem {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupAttachmentItem {
    pub id: String,
    pub relative_path: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupNoteData {
    pub id: String,
    pub title: String,
    pub content_json: String,
    pub plain_text: String,
    pub title_manually_edited: bool,
    pub is_pinned: bool,
    pub archived_at: Option<String>,
    pub deleted_at: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
    pub tags: Vec<BackupTagItem>,
    pub attachments: Option<Vec<BackupAttachmentItem>>,
}

// ---------- 云端备份（坚果云 WebDAV）DTO：wire 侧显式 camelCase ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudBackupConfig {
    pub enabled: bool,
    pub interval: String,
    #[serde(rename = "keepCount")]
    pub keep_count: i64,
    pub account: String,
    #[serde(rename = "davUrl")]
    pub dav_url: String,
    #[serde(rename = "hasPassword")]
    pub has_password: bool,
    #[serde(rename = "lastSuccessAt")]
    pub last_success_at: Option<String>,
    #[serde(rename = "lastError")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudBackupConfigInput {
    pub enabled: bool,
    pub interval: String,
    #[serde(rename = "keepCount")]
    pub keep_count: i64,
    pub account: String,
    #[serde(rename = "davUrl")]
    pub dav_url: String,
    /// 留空（None 或空串）表示沿用已存密码，不回传也不清空
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudBackupFile {
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: i64,
    #[serde(rename = "modifiedAt")]
    pub modified_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudBackupRunResult {
    /// "uploaded" 或 "nochange"
    pub status: String,
    #[serde(rename = "fileName")]
    pub file_name: Option<String>,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudBackupTestResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub version: i64,
    #[serde(rename = "appVersion")]
    pub app_version: String,
    #[serde(rename = "exportedAt")]
    pub exported_at: String,
    pub tags: Option<Vec<BackupTagItem>>,
    pub files: Vec<BackupFileEntry>,
}
