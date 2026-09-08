use crate::db::models::{BackupExportResult, BackupInspectResult, BackupRestoreResult};
use crate::AppState;
use std::sync::atomic::Ordering;
use tauri::State;

#[tauri::command]
pub fn backup_export(
    state: State<AppState>,
    default_filename: Option<String>,
) -> Result<BackupExportResult, String> {
    if let Some(err) = &state.read_only_recovery_error {
        return Err(format!("系统处于只读恢复模式，禁止导出备份：{}", err));
    }

    state.dialog_open.store(true, Ordering::SeqCst);
    let now = chrono::Utc::now();
    let file_name =
        default_filename.unwrap_or_else(|| format!("suijian-backup-{}.zip", now.format("%Y%m%d")));

    let chosen_path = rfd::FileDialog::new()
        .add_filter("Zip Archive (*.zip)", &["zip"])
        .set_file_name(&file_name)
        .save_file();

    state.dialog_open.store(false, Ordering::SeqCst);

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

    state.backup_service.export_backup(&path)
}

#[tauri::command]
pub fn backup_inspect_select(state: State<AppState>) -> Result<BackupInspectResult, String> {
    if let Some(err) = &state.read_only_recovery_error {
        return Err(format!("系统处于只读恢复模式，禁止恢复备份：{}", err));
    }

    state.dialog_open.store(true, Ordering::SeqCst);
    let chosen_path = rfd::FileDialog::new()
        .add_filter("Zip Archive (*.zip)", &["zip"])
        .pick_file();
    state.dialog_open.store(false, Ordering::SeqCst);

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

    let mut inspect_res = state.backup_service.inspect_backup(&path)?;
    let token = uuid::Uuid::new_v4().to_string();
    inspect_res.token = Some(token.clone());

    let mut lock = state.pending_restore.lock().unwrap();
    lock.insert(token, path);

    Ok(inspect_res)
}

#[tauri::command]
pub fn backup_restore_confirm(
    state: State<AppState>,
    token: String,
) -> Result<BackupRestoreResult, String> {
    if let Some(err) = &state.read_only_recovery_error {
        return Err(format!("系统处于只读恢复模式，禁止恢复备份：{}", err));
    }

    let path = {
        let mut lock = state.pending_restore.lock().unwrap();
        lock.remove(&token)
            .ok_or_else(|| "恢复会话无效或已过期，请重新选择备份文件".to_string())?
    };

    state.dialog_open.store(true, Ordering::SeqCst);
    let res = state.backup_service.restore_backup(&path);
    state.dialog_open.store(false, Ordering::SeqCst);

    res
}
