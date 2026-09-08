use crate::db::models::WindowBounds;
use crate::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

// 与 notes/tags/attachments 命令既有模式一致：恢复模式禁止写（内存库跑迁移后写入会假成功、重启即丢）
fn check_write_permission(state: &AppState) -> Result<(), String> {
    if let Some(err) = &state.read_only_recovery_error {
        return Err(format!("系统处于只读保护模式，禁止写入或修改数据：{}", err));
    }
    Ok(())
}

/// 隐藏/退出前直接同步落盘当前窗口几何：防抖线程有 500ms drain，app.exit 与 drain 存在竞态，
/// 不走 channel 而是当场读几何写库（Windows 上 inner_size/outer_position 为直接 Win32 查询，任意线程安全）
pub fn flush_window_bounds(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.inner_size()) else {
        return;
    };
    if size.width == 0 || size.height == 0 {
        return; // 隐藏/最小化期间的零尺寸不覆盖上一次有效值
    }
    let state = app.state::<AppState>();
    let _ = state.settings_service.update(
        "windowBounds",
        serde_json::to_value(WindowBounds {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
        })
        .unwrap_or_default(),
    );
}

/// 隐藏主窗口的唯一入口：hide + skip_taskbar(true)，供 window_confirm_hide 与失焦直接隐藏复用
pub fn hide_main_window(app: &AppHandle) {
    flush_window_bounds(app);
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
        let _ = win.set_skip_taskbar(true);
    }
}

#[tauri::command]
pub fn window_hide(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.emit("event:request-hide", ());
    }
    Ok(())
}

#[tauri::command]
pub fn window_confirm_hide(app: AppHandle) -> Result<(), String> {
    hide_main_window(&app);
    Ok(())
}

#[tauri::command]
pub fn window_set_unsaved_error(state: State<AppState>, has_error: bool) -> Result<(), String> {
    state.has_unsaved_error.store(has_error, Ordering::SeqCst);
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
    check_write_permission(&state)?;
    state
        .settings_service
        .update(
            "windowBounds",
            serde_json::to_value(bounds).map_err(|e| e.to_string())?,
        )
        .map(|_| ())
}
