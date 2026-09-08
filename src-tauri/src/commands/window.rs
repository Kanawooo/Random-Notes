use crate::db::models::WindowBounds;
use crate::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub fn window_hide(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.emit("event:request-hide", ());
    }
    Ok(())
}

#[tauri::command]
pub fn window_confirm_hide(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
        let _ = win.set_skip_taskbar(true);
    }
    Ok(())
}

#[tauri::command]
pub fn window_show(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_skip_taskbar(false);
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
        let _ = win.emit("event:focus-search", ());
    }
    Ok(())
}

#[tauri::command]
pub fn set_dialog_open(state: State<AppState>, open: bool) -> Result<(), String> {
    state.dialog_open.store(open, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn window_get_state(state: State<AppState>) -> Result<Option<WindowBounds>, String> {
    Ok(state.settings_service.get_all().window_bounds)
}

#[tauri::command]
pub fn window_update_state(state: State<AppState>, bounds: WindowBounds) -> Result<(), String> {
    state
        .settings_service
        .update(
            "windowBounds",
            serde_json::to_value(bounds).map_err(|e| e.to_string())?,
        )
        .map(|_| ())
}
