use crate::db::models::{BackupExportResult, BackupInspectResult, BackupRestoreResult};
use crate::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};

/// 同步命令在主线程执行会冻结 UI（rfd 对话框、全量校验/打包/恢复均为 CPU+IO 密集），
/// 统一改 async fn + spawn_blocking；rfd 官方支持任意线程（自管 COM 套间），
/// 理论失败模式 RPC_E_CHANGED_MODE（线程已被 MTL 模型初始化）表现为对话框直接返回 Err，概率极低
#[tauri::command]
pub async fn backup_export(
    app: AppHandle,
    default_filename: Option<String>,
) -> Result<BackupExportResult, String> {
    let state = app.state::<AppState>();
    if let Some(err) = &state.read_only_recovery_error {
        return Err(format!("系统处于只读恢复模式，禁止导出备份：{}", err));
    }
    let dialog_open = state.dialog_open.clone();
    let backup_service = state.backup_service.clone();
    drop(state);
    tauri::async_runtime::spawn_blocking(move || {
        dialog_open.store(true, Ordering::SeqCst);
        let now = chrono::Utc::now();
        let file_name = default_filename
            .unwrap_or_else(|| format!("suijian-backup-{}.zip", now.format("%Y%m%d")));

        let chosen_path = rfd::FileDialog::new()
            .add_filter("Zip Archive (*.zip)", &["zip"])
            .set_file_name(&file_name)
            .save_file();

        dialog_open.store(false, Ordering::SeqCst);

        let path = match chosen_path {
            Some(p) => p,
            None => {
                return Ok(BackupExportResult {
                    canceled: true,
                    file_path: None,
                    note_count: None,
                    attachment_count: None,
                })
            }
        };

        backup_service.export_backup(&path)
    })
    .await
    .map_err(|e| format!("导出任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn backup_inspect_select(app: AppHandle) -> Result<BackupInspectResult, String> {
    let state = app.state::<AppState>();
    if let Some(err) = &state.read_only_recovery_error {
        return Err(format!("系统处于只读恢复模式，禁止恢复备份：{}", err));
    }
    let dialog_open = state.dialog_open.clone();
    let backup_service = state.backup_service.clone();
    let pending_restore = state.pending_restore.clone();
    drop(state);
    tauri::async_runtime::spawn_blocking(move || {
        dialog_open.store(true, Ordering::SeqCst);
        let chosen_path = rfd::FileDialog::new()
            .add_filter("Zip Archive (*.zip)", &["zip"])
            .pick_file();
        dialog_open.store(false, Ordering::SeqCst);

        let path = match chosen_path {
            Some(p) => p,
            None => {
                return Ok(BackupInspectResult {
                    canceled: true,
                    token: None,
                    note_count: None,
                    tag_count: None,
                    attachment_count: None,
                    total_byte_size: None,
                    file_name: None,
                })
            }
        };

        let mut inspect_res = backup_service.inspect_backup(&path)?;
        let token = uuid::Uuid::new_v4().to_string();
        inspect_res.token = Some(token.clone());

        let mut lock = pending_restore.lock().unwrap_or_else(|e| e.into_inner());
        // 签发前清理超过 30 分钟的陈旧会话，防反复选择备份后放弃确认导致无界增长
        lock.retain(|_, (_, issued)| issued.elapsed() < std::time::Duration::from_secs(30 * 60));
        lock.insert(token, (path, std::time::Instant::now()));

        Ok(inspect_res)
    })
    .await
    .map_err(|e| format!("备份检查任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn backup_restore_confirm(
    app: AppHandle,
    token: String,
) -> Result<BackupRestoreResult, String> {
    let state = app.state::<AppState>();
    if let Some(err) = &state.read_only_recovery_error {
        return Err(format!("系统处于只读恢复模式，禁止恢复备份：{}", err));
    }
    // token 消费与 dialog_open 门控留在命令入口，保证单次消费语义；只把慢恢复搬进阻塞线程
    let path = {
        let mut lock = state.pending_restore.lock().unwrap_or_else(|e| e.into_inner());
        lock.remove(&token)
            .map(|(p, _)| p)
            .ok_or_else(|| "恢复会话无效或已过期，请重新选择备份文件".to_string())?
    };
    let dialog_open = state.dialog_open.clone();
    let backup_service = state.backup_service.clone();
    drop(state);
    dialog_open.store(true, Ordering::SeqCst);
    let res = tauri::async_runtime::spawn_blocking(move || backup_service.restore_backup(&path))
        .await
        .map_err(|e| format!("恢复任务执行失败: {}", e));
    dialog_open.store(false, Ordering::SeqCst);
    res?
}
