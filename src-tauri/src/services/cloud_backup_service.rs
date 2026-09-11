use crate::db::models::{
    CloudBackupConfig, CloudBackupConfigInput, CloudBackupFile, CloudBackupRunResult,
    CloudBackupTestResult,
};
use crate::services::backup_service::BackupService;
use crate::services::settings_service::SettingsService;
use crate::utils::paths::get_user_data_dir;
use base64::Engine;
use quick_xml::events::Event as XmlEvent;
use quick_xml::Reader;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// settings 表键：云端备份配置（含账号与应用密码，明文仅存本机）
const CONFIG_KEY: &str = "cloud_backup";
/// settings 表键：最近一次尝试结果与内容指纹
const STATE_KEY: &str = "cloud_backup_state";
/// 云端固定目录（ASCII，避免跨端编码差异）
const REMOTE_DIR: &str = "suijian-backups/";
const BACKUP_FILE_PREFIX: &str = "suijian-backup-";
const BACKUP_FILE_SUFFIX: &str = ".zip";
const BACKUP_TMP_DIR_NAME: &str = ".cloud-backup-tmp";
const RESTORE_TMP_DIR_NAME: &str = ".cloud-restore-tmp";
const DEFAULT_DAV_URL: &str = "https://dav.jianguoyun.com/dav/";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// 上传/下载大包放宽到 10 分钟级（与设计一致，不做断点续传）
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// 坚果云单文件上传上限：超限在导出后直接报错，避免上传半途失败只留难懂的网络错误
const MAX_CLOUD_FILE_BYTES: u64 = 500 * 1024 * 1024;
/// 自动备份检查节奏：每 30 分钟 tick 一次
pub const AUTO_CHECK_INTERVAL: Duration = Duration::from_secs(30 * 60);
const OUTCOME_SUCCESS: &str = "success";
const OUTCOME_NOCHANGE: &str = "nochange";
const OUTCOME_FAILED: &str = "failed";

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:getcontentlength/>
    <D:getlastmodified/>
  </D:prop>
</D:propfind>"#;

/// settings 表 `cloud_backup` 键的持久化形状（与 DTO 的差别仅在含密码原文）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct StoredConfig {
    enabled: bool,
    interval: String,
    keep_count: i64,
    account: String,
    password: String,
    dav_url: String,
}

impl Default for StoredConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: "daily".to_string(),
            keep_count: 5,
            account: String::new(),
            password: String::new(),
            dav_url: DEFAULT_DAV_URL.to_string(),
        }
    }
}

/// settings 表 `cloud_backup_state` 键的持久化形状
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct BackupState {
    last_attempt_at: Option<String>,
    last_success_at: Option<String>,
    last_fingerprint: Option<String>,
    /// "success" / "nochange" / "failed"
    last_outcome: Option<String>,
    last_error: Option<String>,
}

/// PROPFIND 目录响应中的一条资源（仅保留自家备份文件）
struct DavFile {
    file_name: String,
    size_bytes: i64,
    modified_at: String,
}

pub struct CloudBackupService {
    conn: Arc<Mutex<Connection>>,
    settings_service: Arc<SettingsService>,
    backup_service: Arc<BackupService>,
    /// 手动/自动互斥：进行中时手动报错、自动跳过
    in_progress: AtomicBool,
    /// 恢复准备（下载）取消标志：前端「取消下载」置位，下载循环检查后中止
    cancel_download: AtomicBool,
}

impl CloudBackupService {
    pub fn new(
        conn: Arc<Mutex<Connection>>,
        settings_service: Arc<SettingsService>,
        backup_service: Arc<BackupService>,
    ) -> Self {
        Self {
            conn,
            settings_service,
            backup_service,
            in_progress: AtomicBool::new(false),
            cancel_download: AtomicBool::new(false),
        }
    }

    /// 云端备份文件名契约：suijian-backup-YYYYMMDD-HHMMSS.zip
    /// 命令层校验 restore fileName 用（防注入任意路径/文件名）
    pub fn is_valid_backup_file_name(name: &str) -> bool {
        let Some(rest) = name.strip_prefix(BACKUP_FILE_PREFIX) else {
            return false;
        };
        let Some(stamp) = rest.strip_suffix(BACKUP_FILE_SUFFIX) else {
            return false;
        };
        let bytes = stamp.as_bytes();
        if bytes.len() != 15 || bytes[8] != b'-' {
            return false;
        }
        bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 8 || b.is_ascii_digit())
    }

    pub fn get_config(&self) -> Result<CloudBackupConfig, String> {
        let config = self.load_config()?;
        let state = self.load_state();
        Ok(CloudBackupConfig {
            enabled: config.enabled,
            interval: config.interval,
            keep_count: config.keep_count,
            account: config.account,
            dav_url: config.dav_url,
            has_password: !config.password.is_empty(),
            last_success_at: state.last_success_at,
            last_error: state.last_error,
        })
    }

    /// 保存配置（密码留空表示保持不变）；枚举与地址校验在命令层完成后进入
    pub fn update_config(&self, input: CloudBackupConfigInput) -> Result<CloudBackupConfig, String> {
        let mut config = self.load_config()?;
        config.enabled = input.enabled;
        config.interval = input.interval;
        config.keep_count = input.keep_count;
        config.account = input.account.trim().to_string();
        config.dav_url = input.dav_url.trim().to_string();
        if let Some(password) = input.password {
            if !password.is_empty() {
                config.password = password;
            }
        }
        self.save_config(&config)?;
        self.get_config()
    }

    /// 测试连接：PROPFIND Depth 0；预期内的连接/认证错误以 ok=false 消息返回
    pub fn test_connection(
        &self,
        account: &str,
        password: Option<&str>,
        dav_url: &str,
    ) -> Result<CloudBackupTestResult, String> {
        let account = account.trim();
        if account.is_empty() {
            return Err("请先填写坚果云账号".to_string());
        }
        let dav_url = dav_url.trim();
        if dav_url.is_empty() {
            return Err("请先填写 WebDAV 服务器地址".to_string());
        }
        let password = match password {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => self.load_config()?.password,
        };
        if password.is_empty() {
            return Err("请先填写应用密码".to_string());
        }

        let client = WebDavClient::new(dav_url, account, &password)?;
        match client.propfind(&client.base_url, "0") {
            Ok(_) => Ok(CloudBackupTestResult {
                ok: true,
                message: "连接成功，账号与服务器可用".to_string(),
            }),
            Err(message) => Ok(CloudBackupTestResult {
                ok: false,
                message,
            }),
        }
    }

    /// 列出云端备份（仅自家命名，按文件名倒序；格式含时间戳，字典序即时间序）
    pub fn list_backups(&self) -> Result<Vec<CloudBackupFile>, String> {
        let config = self.load_config()?;
        let client = self.build_client(&config)?;
        let files = client.list_dir()?;
        Ok(files
            .into_iter()
            .map(|file| CloudBackupFile {
                file_name: file.file_name,
                size_bytes: file.size_bytes,
                modified_at: file.modified_at,
            })
            .collect())
    }

    /// 下载指定云端备份到 `.cloud-restore-tmp/` 并返回本地路径（fileName 已由命令层校验）
    pub fn download_backup(&self, file_name: &str) -> Result<PathBuf, String> {
        if !Self::is_valid_backup_file_name(file_name) {
            return Err("无效的云端备份文件名".to_string());
        }
        let config = self.load_config()?;
        let client = self.build_client(&config)?;

        let dir = restore_tmp_dir();
        if dir.exists() {
            // 清理失败容忍：旧下载可能仍持有临时文件句柄（Windows 不可删），
            // 残留由下次准备或启动清理兜底，本次直接用新文件名继续
            if let Err(e) = fs::remove_dir_all(&dir) {
                eprintln!("[warn] 清理云端恢复临时目录失败（继续使用）: {}", e);
            }
        }
        fs::create_dir_all(&dir).map_err(|e| format!("创建云端恢复临时目录失败: {}", e))?;

        let dest = dir.join(file_name);
        // 每次下载开始复位取消标志（上次取消可能残留在置位状态）
        self.cancel_download.store(false, Ordering::SeqCst);
        if let Err(message) = client.download(file_name, &dest, &self.cancel_download) {
            let _ = fs::remove_file(&dest);
            return Err(message);
        }
        Ok(dest)
    }

    /// 置位下载取消标志：下载循环检查后中止；下一次下载开始时会自动复位
    pub fn cancel_download(&self) {
        self.cancel_download.store(true, Ordering::SeqCst);
    }

    /// 读取下载取消标志：恢复准备在检查完成后据此决定是否签发 token
    pub fn is_download_cancelled(&self) -> bool {
        self.cancel_download.load(Ordering::SeqCst)
    }

    /// 自动备份检查：未启用/进行中/未到间隔时直接跳过（不联网）
    pub fn auto_check(&self) {
        let config = match self.load_config() {
            Ok(config) => config,
            Err(e) => {
                eprintln!("[warn] 读取云端备份配置失败，跳过本次自动检查: {}", e);
                return;
            }
        };
        if !config.enabled || self.in_progress.load(Ordering::SeqCst) {
            return;
        }
        if !is_due(&config, &self.load_state()) {
            return;
        }
        match self.run_backup() {
            Ok(_) => {}
            // 手动备份恰好抢先时静默跳过，不算失败
            Err(e) if e.contains("正在进行中") => {}
            Err(e) => eprintln!("[warn] 云端自动备份失败: {}", e),
        }
    }

    /// 手动/自动共用入口：AtomicBool 互斥，失败也写入 lastAttemptAt/lastError。
    /// 锁经 guard 释放：execute_backup panic（unwind 构建，如 dev/test）时不会把
    /// 「正在进行中」锁死；release 为 panic=abort，直接终止进程，无残留锁
    pub fn run_backup(&self) -> Result<CloudBackupRunResult, String> {
        struct InProgressGuard<'a>(&'a AtomicBool);
        impl Drop for InProgressGuard<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }

        if self
            .in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("云端备份正在进行中，请稍候".to_string());
        }
        let _guard = InProgressGuard(&self.in_progress);

        let attempt_at = chrono::Utc::now().to_rfc3339();
        let result = self.execute_backup(&attempt_at);

        if let Err(message) = &result {
            let mut state = self.load_state();
            state.last_attempt_at = Some(attempt_at);
            state.last_outcome = Some(OUTCOME_FAILED.to_string());
            state.last_error = Some(message.clone());
            if let Err(e) = self.save_state(&state) {
                eprintln!("[warn] 保存云端备份失败状态失败: {}", e);
            }
        }
        result
    }

    fn execute_backup(&self, attempt_at: &str) -> Result<CloudBackupRunResult, String> {
        let config = self.load_config()?;
        let client = self.build_client(&config)?;

        // 内容指纹与上次成功上传一致 → 不生成包、不联网上传
        let fingerprint = self.compute_fingerprint(&config.account, &config.dav_url)?;
        let mut state = self.load_state();
        if state.last_fingerprint.as_deref() == Some(fingerprint.as_str()) {
            state.last_attempt_at = Some(attempt_at.to_string());
            state.last_outcome = Some(OUTCOME_NOCHANGE.to_string());
            state.last_error = None;
            self.save_state(&state)?;
            return Ok(CloudBackupRunResult {
                status: OUTCOME_NOCHANGE.to_string(),
                file_name: None,
                size_bytes: None,
                message: "内容无变化，已跳过上传".to_string(),
            });
        }

        let tmp_dir = backup_tmp_dir();
        fs::create_dir_all(&tmp_dir).map_err(|e| format!("创建云端备份临时目录失败: {}", e))?;
        let file_name = format!(
            "{}{}{}",
            BACKUP_FILE_PREFIX,
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            BACKUP_FILE_SUFFIX
        );
        let tmp_path = tmp_dir.join(&file_name);

        // 临时包无论成败都清理（导出失败/上传中断不留下残包）
        let upload_result = (|| -> Result<u64, String> {
            self.backup_service.export_backup(&tmp_path)?;
            let size_bytes = fs::metadata(&tmp_path)
                .map_err(|e| format!("读取待上传备份文件大小失败: {}", e))?
                .len();
            if size_bytes > MAX_CLOUD_FILE_BYTES {
                return Err(format!(
                    "备份包 {:.0} MB 超过坚果云单文件 500MB 上限，请先精简图片附件后再试",
                    size_bytes as f64 / 1024.0 / 1024.0
                ));
            }
            client.mkcol_dir()?;
            client.put_file(&file_name, &tmp_path)?;
            Ok(size_bytes)
        })();
        let _ = fs::remove_file(&tmp_path);
        let size_bytes = upload_result?;

        state.last_attempt_at = Some(attempt_at.to_string());
        state.last_success_at = Some(attempt_at.to_string());
        state.last_fingerprint = Some(fingerprint);
        state.last_outcome = Some(OUTCOME_SUCCESS.to_string());
        state.last_error = None;
        self.save_state(&state)?;

        // 保留策略尽力而为：上传已成功，清理失败不改变本次备份结果
        if let Err(e) = self.apply_retention(&client, config.keep_count) {
            eprintln!("[warn] 云端旧备份清理失败: {}", e);
        }

        Ok(CloudBackupRunResult {
            // 对外契约（design）：上传成功为 "uploaded"；内部状态仍记 "success"
            status: "uploaded".to_string(),
            file_name: Some(file_name),
            size_bytes: Some(size_bytes),
            message: "云端备份完成".to_string(),
        })
    }

    fn apply_retention(&self, client: &WebDavClient, keep_count: i64) -> Result<(), String> {
        let files = client.list_dir()?;
        let keep = keep_count.max(0) as usize;
        if files.len() <= keep {
            return Ok(());
        }
        // 列表为倒序（最新在前），末尾是最旧的；从旧到新删除多余项
        for file in files.iter().rev().take(files.len() - keep) {
            client.delete_file(&file.file_name)?;
        }
        Ok(())
    }

    /// 内容指纹：目标账号/服务器 + 便签/附件/标签全量按 id 排序后 SHA-256，不含导出时间戳等易变字段。
    /// 目标并入指纹：换账号或换服务器后必须重新上传，否则新目标会被误判为「无变化」
    fn compute_fingerprint(&self, account: &str, dav_url: &str) -> Result<String, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let mut hasher = Sha256::new();

        hasher.update(b"target\0");
        hash_field(&mut hasher, account.as_bytes());
        hash_field(&mut hasher, dav_url.as_bytes());

        hasher.update(b"notes\0");
        {
            let mut stmt = conn
                .prepare("SELECT id, updated_at, is_pinned, content_json FROM notes ORDER BY id")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            for row in rows {
                let (id, updated_at, is_pinned, content_json) = row.map_err(|e| e.to_string())?;
                hash_field(&mut hasher, id.as_bytes());
                hash_field(&mut hasher, updated_at.as_bytes());
                hash_field(&mut hasher, if is_pinned != 0 { b"1" } else { b"0" });
                hash_field(&mut hasher, content_json.as_bytes());
            }
        }

        hasher.update(b"attachments\0");
        {
            let mut stmt = conn
                .prepare("SELECT id, relative_path, byte_size FROM attachments ORDER BY id")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            for row in rows {
                let (id, relative_path, byte_size) = row.map_err(|e| e.to_string())?;
                hash_field(&mut hasher, id.as_bytes());
                hash_field(&mut hasher, relative_path.as_bytes());
                hash_field(&mut hasher, byte_size.to_string().as_bytes());
            }
        }

        hasher.update(b"tags\0");
        {
            let mut stmt = conn
                .prepare("SELECT id, name, color FROM tags ORDER BY id")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            for row in rows {
                let (id, name, color) = row.map_err(|e| e.to_string())?;
                hash_field(&mut hasher, id.as_bytes());
                hash_field(&mut hasher, name.as_bytes());
                hash_field(&mut hasher, color.as_bytes());
            }
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    fn load_config(&self) -> Result<StoredConfig, String> {
        match self.settings_service.get_raw(CONFIG_KEY)? {
            Some(json) => {
                serde_json::from_str(&json).map_err(|e| format!("云端备份配置解析失败: {}", e))
            }
            None => Ok(StoredConfig::default()),
        }
    }

    fn save_config(&self, config: &StoredConfig) -> Result<(), String> {
        let json = serde_json::to_string(config).map_err(|e| e.to_string())?;
        self.settings_service.set_raw(CONFIG_KEY, &json)
    }

    fn load_state(&self) -> BackupState {
        self.settings_service
            .get_raw(STATE_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    fn save_state(&self, state: &BackupState) -> Result<(), String> {
        let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
        self.settings_service.set_raw(STATE_KEY, &json)
    }

    fn build_client(&self, config: &StoredConfig) -> Result<WebDavClient, String> {
        if config.account.trim().is_empty() {
            return Err("请先在设置中填写坚果云账号".to_string());
        }
        if config.password.is_empty() {
            return Err("请先在设置中填写应用密码".to_string());
        }
        if config.dav_url.trim().is_empty() {
            return Err("请先在设置中填写 WebDAV 服务器地址".to_string());
        }
        WebDavClient::new(&config.dav_url, &config.account, &config.password)
    }
}

/// 清理应用数据目录下的云端临时目录：上传包可重建、下载包可重下，启动时无条件清理
pub fn cleanup_temp_dirs() {
    for dir in [backup_tmp_dir(), restore_tmp_dir()] {
        if dir.exists() {
            if let Err(e) = fs::remove_dir_all(&dir) {
                eprintln!("[warn] 清理云端备份临时目录失败 {:?}: {}", dir, e);
            }
        }
    }
}

fn backup_tmp_dir() -> PathBuf {
    get_user_data_dir().join(BACKUP_TMP_DIR_NAME)
}

fn restore_tmp_dir() -> PathBuf {
    get_user_data_dir().join(RESTORE_TMP_DIR_NAME)
}

fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(bytes);
    hasher.update([0u8]);
}

/// 触发规则：从未尝试→立即；failed→1 小时后重试；其他→按设定间隔
fn is_due(config: &StoredConfig, state: &BackupState) -> bool {
    let Some(attempt_at) = state.last_attempt_at.as_deref() else {
        return true;
    };
    let Ok(last) = chrono::DateTime::parse_from_rfc3339(attempt_at) else {
        return true;
    };
    let elapsed =
        chrono::Utc::now().signed_duration_since(last.with_timezone(&chrono::Utc));
    if elapsed < chrono::Duration::zero() {
        // 系统时钟回拨：按已到期处理
        return true;
    }
    if state.last_outcome.as_deref() == Some(OUTCOME_FAILED) {
        elapsed >= chrono::Duration::hours(1)
    } else {
        elapsed >= interval_duration(&config.interval)
    }
}

fn interval_duration(interval: &str) -> chrono::Duration {
    match interval {
        "every3days" => chrono::Duration::days(3),
        "weekly" => chrono::Duration::days(7),
        _ => chrono::Duration::days(1),
    }
}

/// 坚果云 WebDAV 客户端：Basic 认证；PROPFIND/MKCOL 属非标准方法，需显式放行
struct WebDavClient {
    agent: ureq::Agent,
    base_url: String,
    auth_header: String,
}

impl WebDavClient {
    fn new(dav_url: &str, account: &str, password: &str) -> Result<Self, String> {
        let mut base_url = dav_url.trim().to_string();
        if !base_url.ends_with('/') {
            base_url.push('/');
        }
        // 提前校验地址可解析：后续所有请求 URL 都由此拼接
        parse_uri(&base_url)?;

        let tls_config = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls_config)
            .allow_non_standard_methods(true)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_response(Some(CONNECT_TIMEOUT))
            .timeout_send_body(Some(TRANSFER_TIMEOUT))
            .timeout_recv_body(Some(TRANSFER_TIMEOUT))
            .user_agent(concat!("suijian/", env!("CARGO_PKG_VERSION")))
            .build();
        let agent = ureq::Agent::new_with_config(config);

        let credentials = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", account, password));
        Ok(Self {
            agent,
            base_url,
            auth_header: format!("Basic {}", credentials),
        })
    }

    fn dir_url(&self) -> String {
        format!("{}{}", self.base_url, REMOTE_DIR)
    }

    fn file_url(&self, file_name: &str) -> String {
        format!("{}{}", self.dir_url(), file_name)
    }

    fn propfind(&self, url: &str, depth: &str) -> Result<String, String> {
        let request = ureq::http::Request::builder()
            .method(
                ureq::http::Method::from_bytes(b"PROPFIND")
                    .map_err(|e| format!("构造云端请求失败: {}", e))?,
            )
            .uri(parse_uri(url)?)
            .header("Authorization", &self.auth_header)
            .header("Depth", depth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(PROPFIND_BODY.to_string())
            .map_err(|e| format!("构造云端请求失败: {}", e))?;
        let mut response = self.agent.run(request).map_err(map_ureq_error)?;
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("读取云端响应失败: {}", e))
    }

    /// MKCOL 云端备份目录；405/409 表示已存在，视为成功
    fn mkcol_dir(&self) -> Result<(), String> {
        let request = ureq::http::Request::builder()
            .method(
                ureq::http::Method::from_bytes(b"MKCOL")
                    .map_err(|e| format!("构造云端请求失败: {}", e))?,
            )
            .uri(parse_uri(&self.dir_url())?)
            .header("Authorization", &self.auth_header)
            .body(())
            .map_err(|e| format!("构造云端请求失败: {}", e))?;
        match self.agent.run(request) {
            Ok(_) => Ok(()),
            Err(ureq::Error::StatusCode(405)) | Err(ureq::Error::StatusCode(409)) => Ok(()),
            Err(e) => Err(map_ureq_error(e)),
        }
    }

    /// PUT 流式上传（File body 不进内存）
    fn put_file(&self, file_name: &str, path: &Path) -> Result<(), String> {
        let file = File::open(path).map_err(|e| format!("打开待上传的备份文件失败: {}", e))?;
        self.agent
            .put(parse_uri(&self.file_url(file_name))?)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/zip")
            .send(file)
            .map_err(map_ureq_error)?;
        Ok(())
    }

    /// GET 流式下载到文件；每块前检查取消标志，置位后中止（已写内容由调用方清理）
    fn download(&self, file_name: &str, dest: &Path, cancel: &AtomicBool) -> Result<(), String> {
        let response = self
            .agent
            .get(parse_uri(&self.file_url(file_name))?)
            .header("Authorization", &self.auth_header)
            .call()
            .map_err(map_ureq_error)?;
        let mut reader = response.into_body().into_reader();
        let mut file = File::create(dest).map_err(|e| format!("创建下载临时文件失败: {}", e))?;
        let mut buf = [0u8; 64 * 1024];
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("下载已取消".to_string());
            }
            let read = reader
                .read(&mut buf)
                .map_err(|e| format!("下载云端备份失败（可能网络中断）: {}", e))?;
            if read == 0 {
                break;
            }
            file.write_all(&buf[..read])
                .map_err(|e| format!("写入下载临时文件失败: {}", e))?;
        }
        Ok(())
    }

    fn delete_file(&self, file_name: &str) -> Result<(), String> {
        self.agent
            .delete(parse_uri(&self.file_url(file_name))?)
            .header("Authorization", &self.auth_header)
            .call()
            .map_err(map_ureq_error)?;
        Ok(())
    }

    fn list_dir(&self) -> Result<Vec<DavFile>, String> {
        let xml = self.propfind(&self.dir_url(), "1")?;
        parse_propfind_files(&xml)
    }
}

fn parse_uri(url: &str) -> Result<ureq::http::Uri, String> {
    url.parse::<ureq::http::Uri>()
        .map_err(|e| format!("服务器地址无效: {}", e))
}

/// ureq 错误 → 中文可行动提示
fn map_ureq_error(err: ureq::Error) -> String {
    match err {
        ureq::Error::StatusCode(code) => map_status_code(code),
        ureq::Error::Timeout(_)
        | ureq::Error::Io(_)
        | ureq::Error::ConnectionFailed
        | ureq::Error::HostNotFound
        | ureq::Error::Tls(_)
        | ureq::Error::NativeTls(_) => "无法连接坚果云，请检查网络后重试".to_string(),
        other => format!("云端请求失败: {}", other),
    }
}

fn map_status_code(code: u16) -> String {
    match code {
        401 => "账号或应用密码错误，请重新生成应用密码".to_string(),
        403 => "坚果云拒绝访问，请确认应用密码权限".to_string(),
        404 => "云端备份目录不存在，请先执行一次「立即备份」".to_string(),
        429 => "请求过于频繁，稍后会自动重试".to_string(),
        507 => "坚果云空间不足，请清理云端备份；清理后会自动重试".to_string(),
        _ => format!("云端备份请求失败（HTTP {}）", code),
    }
}

/// 解析 PROPFIND 207 multistatus：按 local-name 匹配（兼容 d:/D:/无前缀），
/// href URL 解码后取文件名，仅保留自家备份命名
fn parse_propfind_files(xml: &str) -> Result<Vec<DavFile>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut files: Vec<DavFile> = Vec::new();
    let mut builder: Option<DavFileBuilder> = None;
    let mut current_tag: Vec<u8> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(XmlEvent::Start(e)) => {
                let local = e.local_name().as_ref().to_vec();
                if local == b"response" {
                    builder = Some(DavFileBuilder::default());
                }
                current_tag = local;
            }
            Ok(XmlEvent::Text(text)) => {
                if let Some(entry) = builder.as_mut() {
                    let decoded = text
                        .decode()
                        .map_err(|e| format!("解析云端目录响应失败: {}", e))?;
                    let value = quick_xml::escape::unescape(&decoded)
                        .map(|v| v.into_owned())
                        .unwrap_or_else(|_| decoded.into_owned());
                    match current_tag.as_slice() {
                        b"href" => entry.href = value,
                        b"getcontentlength" => entry.size_bytes = value.trim().parse::<i64>().ok(),
                        b"getlastmodified" => entry.modified_at = value,
                        _ => {}
                    }
                }
            }
            Ok(XmlEvent::End(e)) => {
                if e.local_name().as_ref() == b"response" {
                    if let Some(entry) = builder.take() {
                        if let Some(file) = entry.into_dav_file() {
                            files.push(file);
                        }
                    }
                }
                current_tag.clear();
            }
            Ok(XmlEvent::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(format!("解析云端目录响应失败: {}", e)),
        }
    }

    files.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    Ok(files)
}

#[derive(Default)]
struct DavFileBuilder {
    href: String,
    size_bytes: Option<i64>,
    modified_at: String,
}

impl DavFileBuilder {
    fn into_dav_file(self) -> Option<DavFile> {
        let decoded = percent_decode(&self.href);
        let path = decoded
            .split(['?', '#'])
            .next()
            .unwrap_or_default();
        if path.ends_with('/') {
            // 目录自身（Depth 1 会包含所请求目录）
            return None;
        }
        let file_name = path.rsplit('/').next().unwrap_or_default().to_string();
        if !CloudBackupService::is_valid_backup_file_name(&file_name) {
            // 只认自家备份文件：保留策略的 DELETE 绝不能碰目录里别的文件
            return None;
        }
        Some(DavFile {
            file_name,
            size_bytes: self.size_bytes.unwrap_or(0),
            modified_at: to_iso_time(&self.modified_at),
        })
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// getlastmodified（RFC 1123）→ ISO 8601；解析失败保留原文
fn to_iso_time(raw: &str) -> String {
    chrono::DateTime::parse_from_rfc2822(raw.trim())
        .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339())
        .unwrap_or_else(|_| raw.trim().to_string())
}
