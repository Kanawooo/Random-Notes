use crate::db::models::{
    BackupInspectResult, CloudBackupConfig, CloudBackupConfigInput, CloudBackupFile,
    CloudBackupRunResult, CloudBackupTestResult,
};
use crate::services::cloud_backup_service::CloudBackupService;
use crate::AppState;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

const ALLOWED_INTERVALS: &[&str] = &["daily", "every3days", "weekly"];
const ALLOWED_KEEP_COUNTS: &[i64] = &[3, 5, 10];

fn check_write_permission(state: &AppState) -> Result<(), String> {
    if let Some(err) = &state.read_only_recovery_error {
        return Err(format!("系统处于只读恢复模式，禁止云端备份操作：{}", err));
    }
    Ok(())
}

#[tauri::command]
pub fn cloud_backup_config_get(state: State<AppState>) -> Result<CloudBackupConfig, String> {
    check_write_permission(&state)?;
    state.cloud_backup_service.get_config()
}

#[tauri::command]
pub fn cloud_backup_config_update(
    state: State<AppState>,
    input: CloudBackupConfigInput,
) -> Result<CloudBackupConfig, String> {
    check_write_permission(&state)?;
    if !ALLOWED_INTERVALS.contains(&input.interval.as_str()) {
        return Err(format!("无效的备份间隔: {}", input.interval));
    }
    if !ALLOWED_KEEP_COUNTS.contains(&input.keep_count) {
        return Err(format!("无效的保留份数: {}", input.keep_count));
    }
    let dav_url = input.dav_url.trim();
    if !dav_url.starts_with("http://") && !dav_url.starts_with("https://") {
        return Err("服务器地址必须以 http:// 或 https:// 开头".to_string());
    }
    state.cloud_backup_service.update_config(input)
}

#[tauri::command]
pub async fn cloud_backup_test_connection(
    app: AppHandle,
    account: String,
    password: Option<String>,
    dav_url: String,
) -> Result<CloudBackupTestResult, String> {
    let state = app.state::<AppState>();
    check_write_permission(&state)?;
    let service = state.cloud_backup_service.clone();
    drop(state);
    tauri::async_runtime::spawn_blocking(move || {
        service.test_connection(&account, password.as_deref(), &dav_url)
    })
    .await
    .map_err(|e| format!("云端连接测试任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn cloud_backup_run(app: AppHandle) -> Result<CloudBackupRunResult, String> {
    let state = app.state::<AppState>();
    check_write_permission(&state)?;
    let service = state.cloud_backup_service.clone();
    drop(state);
    tauri::async_runtime::spawn_blocking(move || service.run_backup())
        .await
        .map_err(|e| format!("云端备份任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn cloud_backup_list(app: AppHandle) -> Result<Vec<CloudBackupFile>, String> {
    let state = app.state::<AppState>();
    check_write_permission(&state)?;
    let service = state.cloud_backup_service.clone();
    drop(state);
    tauri::async_runtime::spawn_blocking(move || service.list_backups())
        .await
        .map_err(|e| format!("云端备份列表任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn cloud_backup_restore_prepare(
    app: AppHandle,
    file_name: String,
) -> Result<BackupInspectResult, String> {
    let state = app.state::<AppState>();
    check_write_permission(&state)?;
    if !CloudBackupService::is_valid_backup_file_name(&file_name) {
        return Err("无效的云端备份文件名".to_string());
    }
    let cloud_service = state.cloud_backup_service.clone();
    let backup_service = state.backup_service.clone();
    let pending_restore = state.pending_restore.clone();
    drop(state);
    tauri::async_runtime::spawn_blocking(move || {
        let zip_path = cloud_service.download_backup(&file_name)?;
        let mut inspect_res = backup_service.inspect_backup(&zip_path)?;
        // 下载/检查期间收到取消：不签发 token，删掉临时包（避免留下无人使用的 token 与残留文件）
        if cloud_service.is_download_cancelled() {
            let _ = std::fs::remove_file(&zip_path);
            return Err("下载已取消".to_string());
        }
        let token = uuid::Uuid::new_v4().to_string();
        inspect_res.token = Some(token.clone());
        // 复用 backup_inspect_select 的 30 分钟 token 窗口与单次消费语义
        let mut lock = pending_restore.lock().unwrap_or_else(|e| e.into_inner());
        lock.retain(|_, (_, issued)| issued.elapsed() < Duration::from_secs(30 * 60));
        lock.insert(token, (zip_path, Instant::now()));
        Ok(inspect_res)
    })
    .await
    .map_err(|e| format!("云端恢复准备任务执行失败: {}", e))?
}

/// 取消进行中的恢复准备下载：置位中断标志，下一次下载循环检查后中止
#[tauri::command]
pub fn cloud_backup_restore_cancel(state: State<AppState>) -> Result<(), String> {
    check_write_permission(&state)?;
    state.cloud_backup_service.cancel_download();
    Ok(())
}
