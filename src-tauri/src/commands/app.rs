use crate::db::models::{AppInfo, RecoveryStatus};
use crate::utils::paths::{get_db_path, get_user_data_dir};
use crate::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub fn app_get_info() -> Result<AppInfo, String> {
    Ok(AppInfo {
        name: "随笺".to_string(),
        version: "0.1.0".to_string(),
        user_data_path: get_user_data_dir().to_string_lossy().to_string(),
        db_path: get_db_path().to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn app_get_recovery_status(state: State<AppState>) -> Result<RecoveryStatus, String> {
    Ok(RecoveryStatus {
        is_recovery: state.read_only_recovery_error.is_some(),
        error: state.read_only_recovery_error.clone(),
        db_path: get_db_path().to_string_lossy().to_string(),
        user_data_path: get_user_data_dir().to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn app_open_user_data_folder() -> Result<(), String> {
    open::that(get_user_data_dir()).map_err(|e| format!("无法打开用户数据文件夹: {}", e))
}

#[tauri::command]
pub fn app_quit(app: AppHandle) -> Result<(), String> {
    let _ = app.emit("event:request-quit", ());
    Ok(())
}

#[tauri::command]
pub fn app_confirm_quit(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn app_open_external(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.contains('\0') || trimmed.contains('\r') || trimmed.contains('\n') {
        return Err("链接包含非法控制字符".to_string());
    }

    let lower = trimmed.to_lowercase();
    if lower.contains("javascript:") || lower.contains("file:") || lower.contains("data:") {
        return Err("禁止访问危险协议链接".to_string());
    }

    if !(trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("mailto:"))
    {
        return Err("仅允许打开 http, https, mailto 协议的外部链接".to_string());
    }

    open::that(trimmed).map_err(|e| format!("无法打开外部链接: {}", e))
}

#[tauri::command]
pub fn renderer_ready(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    // 渲染进程（重）加载后编辑器状态从 DB 重建，复位门控标志，防滞留值永久抑制自动隐藏
    state.has_unsaved_error.store(false, Ordering::SeqCst);
    state.dialog_open.store(false, Ordering::SeqCst);
    if !state.is_minimized_startup {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.set_skip_taskbar(false);
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
            let _ = win.emit("event:focus-search", ());
        }
    }
    Ok(())
}

