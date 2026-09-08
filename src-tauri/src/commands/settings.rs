use crate::db::models::{AppSettings, HotkeyStatus};
use crate::AppState;
use tauri::{AppHandle, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

const ALLOWED_KEYS: &[&str] = &[
    "hotkey",
    "shortcutNewNote",
    "shortcutBackToSearch",
    "shortcutDismiss",
    "launchAtLogin",
    "autoHideOnBlur",
    "windowBounds",
];

// 恢复模式内存库跑迁移后 settings 表存在，无守卫则写入“成功”但重启即丢（数据欺骗），
// 与 notes/tags/attachments 命令的既有局部守卫模式一致
fn check_write_permission(state: &AppState) -> Result<(), String> {
    if let Some(err) = &state.read_only_recovery_error {
        return Err(format!("系统处于只读保护模式，禁止写入或修改数据：{}", err));
    }
    Ok(())
}

#[tauri::command]
pub fn settings_get_all(state: State<AppState>) -> Result<AppSettings, String> {
    Ok(state.settings_service.get_all())
}

#[tauri::command]
pub fn settings_get(state: State<AppState>, key: String) -> Result<Option<String>, String> {
    if !ALLOWED_KEYS.contains(&key.as_str()) {
        return Err(format!("不支持的设置项: {}", key));
    }
    state.settings_service.get_raw(&key)
}

#[tauri::command]
pub fn settings_update(
    app: AppHandle,
    state: State<AppState>,
    key: String,
    value: serde_json::Value,
) -> Result<AppSettings, String> {
    check_write_permission(&state)?;
    if !ALLOWED_KEYS.contains(&key.as_str()) {
        return Err(format!("不支持的设置项: {}", key));
    }

    if key == "launchAtLogin" {
        let target = value
            .as_bool()
            .ok_or_else(|| "launchAtLogin 必须是布尔值".to_string())?;
        let old_val = state.settings_service.get_all().launch_at_login;

        use tauri_plugin_autostart::ManagerExt;
        let os_res = if target {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        };

        if let Err(e) = os_res {
            return Err(format!("系统开机启动项配置失败: {}", e));
        }

        match state.settings_service.update(&key, value) {
            Ok(updated) => Ok(updated),
            Err(e) => {
                // Revert OS configuration
                if old_val {
                    let _ = app.autolaunch().enable();
                } else {
                    let _ = app.autolaunch().disable();
                }
                Err(format!("保存自启动配置失败: {}", e))
            }
        }
    } else {
        state.settings_service.update(&key, value)
    }
}

#[tauri::command]
pub fn settings_update_action_shortcuts(
    state: State<AppState>,
    shortcut_new_note: String,
    shortcut_back_to_search: String,
    shortcut_dismiss: String,
) -> Result<AppSettings, String> {
    check_write_permission(&state)?;
    state.settings_service.update_action_shortcuts(
        &shortcut_new_note,
        &shortcut_back_to_search,
        &shortcut_dismiss,
    )
}

#[tauri::command]
pub fn settings_get_hotkey_status(state: State<AppState>) -> Result<HotkeyStatus, String> {
    let lock = state.startup_hotkey_status.lock().unwrap_or_else(|e| e.into_inner());
    Ok(lock.clone())
}

#[tauri::command]
pub fn settings_register_hotkey(
    app: AppHandle,
    state: State<AppState>,
    hotkey: String,
) -> Result<HotkeyStatus, String> {
    let current_settings = state.settings_service.get_all();
    let old_hotkey = current_settings.hotkey.clone();

    let normalized =
        match crate::services::settings_service::SettingsService::validate_and_normalize_shortcut(
            &hotkey, true,
        ) {
            Ok(n) => n,
            Err(e) => {
                return Ok(HotkeyStatus {
                    registered: false,
                    current_hotkey: old_hotkey,
                    requested_hotkey: Some(hotkey),
                    request_succeeded: Some(false),
                    error: Some(format!("快捷键格式无效: {}", e)),
                    recommended_hotkey: Some("Ctrl+Shift+Space".to_string()),
                });
            }
        };

    // If same as current, check if already registered and active
    if normalized.eq_ignore_ascii_case(&old_hotkey) {
        let is_registered = {
            let lock = state.startup_hotkey_status.lock().unwrap_or_else(|e| e.into_inner());
            lock.registered
        };
        if is_registered {
            let status = HotkeyStatus {
                registered: true,
                current_hotkey: old_hotkey,
                requested_hotkey: Some(normalized),
                request_succeeded: Some(true),
                error: None,
                recommended_hotkey: None,
            };
            let mut lock = state.startup_hotkey_status.lock().unwrap_or_else(|e| e.into_inner());
            *lock = status.clone();
            return Ok(status);
        }
    }

    // Validate shortcut string parse
    let parsed_shortcut: Result<Shortcut, _> = normalized.parse();
    if parsed_shortcut.is_err() {
        return Ok(HotkeyStatus {
            registered: false,
            current_hotkey: old_hotkey,
            requested_hotkey: Some(normalized),
            request_succeeded: Some(false),
            error: Some("快捷键格式不正确".to_string()),
            recommended_hotkey: Some("Ctrl+Shift+Space".to_string()),
        });
    }

    let shortcut = parsed_shortcut.unwrap();
    let gs = app.global_shortcut();

    // 1. Register new hotkey on global shortcut manager
    if let Err(e) = gs.register(shortcut) {
        return Ok(HotkeyStatus {
            registered: false,
            current_hotkey: old_hotkey,
            requested_hotkey: Some(normalized),
            request_succeeded: Some(false),
            error: Some(format!("注册全局快捷键失败: {}", e)),
            recommended_hotkey: Some("Ctrl+Shift+Space".to_string()),
        });
    }

    // 2. Persist new hotkey in DB. If persistence fails: ROLL BACK new registration!
    if let Err(e) = state
        .settings_service
        .update("hotkey", serde_json::Value::String(normalized.clone()))
    {
        let _ = gs.unregister(shortcut);
        return Ok(HotkeyStatus {
            registered: false,
            current_hotkey: old_hotkey,
            requested_hotkey: Some(normalized),
            request_succeeded: Some(false),
            error: Some(format!("保存快捷键到数据库失败: {}", e)),
            recommended_hotkey: Some("Ctrl+Shift+Space".to_string()),
        });
    }

    // 3. Unregister old shortcut after new one is safely registered and persisted
    if let Ok(old_sc) = old_hotkey.parse::<Shortcut>() {
        let _ = gs.unregister(old_sc);
    }

    let status = HotkeyStatus {
        registered: true,
        current_hotkey: normalized.clone(),
        requested_hotkey: Some(normalized),
        request_succeeded: Some(true),
        error: None,
        recommended_hotkey: None,
    };

    let mut lock = state.startup_hotkey_status.lock().unwrap_or_else(|e| e.into_inner());
    *lock = status.clone();

    Ok(status)
}
