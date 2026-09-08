pub mod commands;
pub mod db;
pub mod protocol;
pub mod services;
pub mod utils;

use commands::*;
use db::models::{HotkeyStatus, WindowBounds};
use db::DbService;
use protocol::handle_attachment_protocol;
use services::attachment_service::AttachmentService;
use services::backup_service::BackupService;
use services::notes_service::NotesService;
use services::purge_service::PurgeService;
use services::settings_service::SettingsService;
use services::tags_service::TagsService;
use utils::paths::{ensure_dirs, get_attachments_dir, get_db_path, set_user_data_override};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub struct AppState {
    pub db_service: Arc<DbService>,
    pub notes_service: Arc<NotesService>,
    pub tags_service: Arc<TagsService>,
    pub attachment_service: Arc<AttachmentService>,
    pub settings_service: Arc<SettingsService>,
    pub purge_service: Arc<PurgeService>,
    pub backup_service: Arc<BackupService>,
    pub dialog_open: Arc<AtomicBool>,
    pub pending_restore: Arc<Mutex<HashMap<String, PathBuf>>>,
    pub startup_hotkey_status: Arc<Mutex<HotkeyStatus>>,
    pub read_only_recovery_error: Option<String>,
    pub focus_blur_token: Arc<AtomicU64>,
    pub is_minimized_startup: bool,
}

fn toggle_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.emit("event:request-hide", ());
        } else {
            let _ = win.set_skip_taskbar(false);
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
            let _ = win.emit("event:focus-search", ());
        }
    }
}

pub fn run() {
    // 1. Process CLI flags (e.g. --user-data-dir, --minimized)
    let args: Vec<String> = std::env::args().collect();
    let is_minimized_startup = args.iter().any(|arg| {
        arg == "--minimized" || arg == "--hidden" || arg == "--launch-at-login"
    });
    for (i, arg) in args.iter().enumerate() {
        if let Some(val) = arg.strip_prefix("--user-data-dir=") {
            set_user_data_override(PathBuf::from(val));
        } else if arg == "--user-data-dir" {
            if let Some(next_arg) = args.get(i + 1) {
                set_user_data_override(PathBuf::from(next_arg));
            }
        }
    }

    let _ = ensure_dirs();

    // 2. Initialize Database & Services
    let db_path = get_db_path();
    let (db_service, read_only_recovery_error) = match DbService::new(db_path.clone()) {
        Ok(service) => (Arc::new(service), None),
        Err(e) => {
            // Read-only recovery mode: create an in-memory SQLite connection for fallback
            let mem_conn = rusqlite::Connection::open_in_memory().unwrap();
            let fallback_db = DbService::from_connection(mem_conn, db_path);
            (
                Arc::new(fallback_db),
                Some(format!("数据库初始化或迁移失败: {}", e)),
            )
        }
    };

    let attachment_service = Arc::new(AttachmentService::new(
        db_service.get_conn(),
        get_attachments_dir(),
    ));
    let notes_service = Arc::new(NotesService::new(
        db_service.get_conn(),
        attachment_service.clone(),
    ));
    let tags_service = Arc::new(TagsService::new(db_service.get_conn()));
    let settings_service = Arc::new(SettingsService::new(db_service.get_conn()));
    let purge_service = Arc::new(PurgeService::new(
        db_service.get_conn(),
        attachment_service.clone(),
    ));
    let backup_service = Arc::new(BackupService::new(
        db_service.clone(),
        notes_service.clone(),
        tags_service.clone(),
        attachment_service.clone(),
    ));

    // Run startup 30-day purge only if DB is healthy
    if read_only_recovery_error.is_none() {
        let _ = purge_service.purge_deleted_notes_older_than_days(30);

        // Periodic 24h background purge
        let purge_clone = purge_service.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(24 * 3600));
            let _ = purge_clone.purge_deleted_notes_older_than_days(30);
        });
    }

    let dialog_open = Arc::new(AtomicBool::new(false));
    let focus_blur_token = Arc::new(AtomicU64::new(0));
    let pending_restore = Arc::new(Mutex::new(HashMap::new()));
    let startup_hotkey_status = Arc::new(Mutex::new(HotkeyStatus {
        registered: false,
        current_hotkey: String::new(),
        requested_hotkey: None,
        request_succeeded: None,
        error: None,
        recommended_hotkey: None,
    }));

    let attach_proto_clone = attachment_service.clone();

    // 3. Build Tauri application
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_skip_taskbar(false);
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
                let _ = win.emit("event:focus-search", ());
            }
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_window(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .register_uri_scheme_protocol("suijian-attachment", move |_ctx, req| {
            handle_attachment_protocol(&req, attach_proto_clone.clone())
        })
        .manage(AppState {
            db_service,
            notes_service,
            tags_service,
            attachment_service,
            settings_service,
            purge_service,
            backup_service,
            dialog_open,
            pending_restore,
            startup_hotkey_status,
            read_only_recovery_error,
            focus_blur_token,
            is_minimized_startup,
        })
        .setup(|app| {
            let handle = app.handle();
            let state = handle.state::<AppState>();
            let settings = state.settings_service.get_all();

            // Register global shortcut and store result in startup_hotkey_status
            let hotkey_str = settings.hotkey.clone();
            let hotkey_res = match hotkey_str.parse::<Shortcut>() {
                Ok(sc) => match handle.global_shortcut().register(sc) {
                    Ok(_) => HotkeyStatus {
                        registered: true,
                        current_hotkey: hotkey_str.clone(),
                        requested_hotkey: Some(hotkey_str),
                        request_succeeded: Some(true),
                        error: None,
                        recommended_hotkey: None,
                    },
                    Err(e) => HotkeyStatus {
                        registered: false,
                        current_hotkey: String::new(),
                        requested_hotkey: Some(hotkey_str),
                        request_succeeded: Some(false),
                        error: Some(format!("注册全局快捷键失败 (已被其他程序占用): {}", e)),
                        recommended_hotkey: Some("Ctrl+Shift+Space".to_string()),
                    },
                },
                Err(e) => HotkeyStatus {
                    registered: false,
                    current_hotkey: String::new(),
                    requested_hotkey: Some(hotkey_str),
                    request_succeeded: Some(false),
                    error: Some(format!("快捷键格式无效: {}", e)),
                    recommended_hotkey: Some("Ctrl+Shift+Space".to_string()),
                },
            };

            let mut hotkey_lock = state.startup_hotkey_status.lock().unwrap();
            *hotkey_lock = hotkey_res;
            drop(hotkey_lock);

            // Restore and validate window bounds with multi-monitor check
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_icon(tauri::include_image!("icons/128x128.png"));
                let _ = win.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize {
                    width: 680.0,
                    height: 460.0,
                })));

                let scale_factor = win
                    .current_monitor()
                    .ok()
                    .flatten()
                    .map(|m| m.scale_factor())
                    .unwrap_or(1.0);

                let (is_logical_default, target_phys_w, target_phys_h, target_x, target_y) =
                    match settings.window_bounds {
                        None => (
                            true,
                            (880.0 * scale_factor).round() as u32,
                            (640.0 * scale_factor).round() as u32,
                            None,
                            None,
                        ),
                        Some(bounds) => {
                            // If saved bounds are exactly the old erroneous physical default (880x640),
                            // upgrade to logical 880x640 DIP.
                            if bounds.width == 880 && bounds.height == 640 {
                                (
                                    true,
                                    (880.0 * scale_factor).round() as u32,
                                    (640.0 * scale_factor).round() as u32,
                                    Some(bounds.x),
                                    Some(bounds.y),
                                )
                            } else {
                                let min_phys_w = (680.0 * scale_factor).round() as u32;
                                let min_phys_h = (460.0 * scale_factor).round() as u32;
                                (
                                    false,
                                    bounds.width.max(min_phys_w),
                                    bounds.height.max(min_phys_h),
                                    Some(bounds.x),
                                    Some(bounds.y),
                                )
                            }
                        }
                    };

                let mut final_x = None;
                let mut final_y = None;
                let mut final_w = target_phys_w;
                let mut final_h = target_phys_h;

                if let (Some(x), Some(y)) = (target_x, target_y) {
                    if let Ok(monitors) = win.available_monitors() {
                        let win_l = x;
                        let win_t = y;
                        let win_r = x + final_w as i32;
                        let win_b = y + final_h as i32;

                        for m in monitors {
                            let m_pos = m.position();
                            let m_size = m.size();
                            let m_l = m_pos.x;
                            let m_t = m_pos.y;
                            let m_r = m_pos.x + m_size.width as i32;
                            let m_b = m_pos.y + m_size.height as i32;

                            let inter_w = (win_r.min(m_r) - win_l.max(m_l)).max(0);
                            let inter_h = (win_b.min(m_b) - win_t.max(m_t)).max(0);
                            if inter_w >= 100 && inter_h >= 100 {
                                final_w = final_w.min(m_size.width);
                                final_h = final_h.min(m_size.height);

                                let clamped_x = x.max(m_l).min(m_r - final_w as i32);
                                let clamped_y = y.max(m_t).min(m_b - final_h as i32);

                                final_x = Some(clamped_x);
                                final_y = Some(clamped_y);
                                break;
                            }
                        }
                    }
                }

                if is_logical_default {
                    let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize {
                        width: 880.0,
                        height: 640.0,
                    }));
                } else {
                    let _ = win.set_size(tauri::Size::Physical(tauri::PhysicalSize {
                        width: final_w,
                        height: final_h,
                    }));
                }

                if let (Some(x), Some(y)) = (final_x, final_y) {
                    let _ = win
                        .set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
                } else {
                    // Position on current/primary monitor centered horizontally, upper 15% Y
                    if let Ok(Some(monitor)) = win.current_monitor() {
                        let m_pos = monitor.position();
                        let m_size = monitor.size();
                        let new_x = m_pos.x + ((m_size.width as i32 - final_w as i32) / 2);
                        let new_y = m_pos.y + ((m_size.height as i32 * 15) / 100);
                        let _ =
                            win.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                                x: new_x,
                                y: new_y,
                            }));
                    } else {
                        let _ = win.center();
                    }
                }
            }

            // Clean up any orphan attachments on startup
            let _ = state.attachment_service.cleanup_orphans();

            // Setup System Tray
            let show_item = MenuItem::with_id(handle, "show", "显示随笺", true, None::<&str>)?;
            let new_note_item =
                MenuItem::with_id(handle, "new_note", "新建便签", true, None::<&str>)?;
            let sep1 = tauri::menu::PredefinedMenuItem::separator(handle)?;
            let launch_item = CheckMenuItem::with_id(
                handle,
                "launch_at_login",
                "登录时启动",
                true,
                settings.launch_at_login,
                None::<&str>,
            )?;
            let sep2 = tauri::menu::PredefinedMenuItem::separator(handle)?;
            let quit_item = MenuItem::with_id(handle, "quit", "退出", true, None::<&str>)?;

            let tray_menu = Menu::with_items(
                handle,
                &[
                    &show_item,
                    &new_note_item,
                    &sep1,
                    &launch_item,
                    &sep2,
                    &quit_item,
                ],
            )?;

            let _tray = TrayIconBuilder::with_id("tray")
                .icon(tauri::include_image!("icons/32x32.png"))
                .tooltip("随笺 - 桌面便签")
                .menu(&tray_menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.set_skip_taskbar(false);
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                            let _ = win.emit("event:focus-search", ());
                        }
                    }
                    "new_note" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.set_skip_taskbar(false);
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                            let _ = win.emit("event:request-new-note", ());
                        }
                    }
                    "launch_at_login" => {
                        let state = app.state::<AppState>();
                        let cur = state.settings_service.get_all().launch_at_login;
                        let target = !cur;
                        use tauri_plugin_autostart::ManagerExt;
                        let os_res = if target {
                            app.autolaunch().enable()
                        } else {
                            app.autolaunch().disable()
                        };
                        if os_res.is_ok() {
                            if let Err(e) = state
                                .settings_service
                                .update("launchAtLogin", serde_json::Value::Bool(target))
                            {
                                eprintln!("[warn] 保存开机自启配置失败: {}", e);
                                if cur {
                                    let _ = app.autolaunch().enable();
                                } else {
                                    let _ = app.autolaunch().disable();
                                }
                            }
                        }
                    }
                    "quit" => {
                        let _ = app.emit("event:request-quit", ());
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                let _ = win.emit("event:request-hide", ());
                            } else {
                                let _ = win.set_skip_taskbar(false);
                                let _ = win.show();
                                let _ = win.unminimize();
                                let _ = win.set_focus();
                                let _ = win.emit("event:focus-search", ());
                            }
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.emit("event:request-hide", ());
            }
            WindowEvent::Focused(focused) => {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let auto_hide = state.settings_service.get_all().auto_hide_on_blur;
                let is_dialog_open = state.dialog_open.load(Ordering::SeqCst);

                if *focused {
                    state.focus_blur_token.fetch_add(1, Ordering::SeqCst);
                } else if auto_hide && !is_dialog_open {
                    let token_val = state.focus_blur_token.fetch_add(1, Ordering::SeqCst) + 1;
                    let app_handle = app.clone();
                    let win = window.clone();
                    let token_arc = state.focus_blur_token.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(350));
                        if token_arc.load(Ordering::SeqCst) == token_val {
                            if !win.is_focused().unwrap_or(false) {
                                let state = app_handle.state::<AppState>();
                                let auto_hide = state.settings_service.get_all().auto_hide_on_blur;
                                let is_dialog_open = state.dialog_open.load(Ordering::SeqCst);
                                if auto_hide && !is_dialog_open {
                                    let _ = win.emit("event:request-hide", ());
                                }
                            }
                        }
                    });
                }
            }
            WindowEvent::Moved(pos) => {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                state.focus_blur_token.fetch_add(1, Ordering::SeqCst);
                if let Ok(size) = window.inner_size() {
                    if let Err(e) = state.settings_service.update(
                        "windowBounds",
                        serde_json::to_value(WindowBounds {
                            x: pos.x,
                            y: pos.y,
                            width: size.width,
                            height: size.height,
                        })
                        .unwrap_or_default(),
                    ) {
                        eprintln!("[warn] 保存窗口移动位置失败: {}", e);
                    }
                }
            }
            WindowEvent::Resized(size) => {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                state.focus_blur_token.fetch_add(1, Ordering::SeqCst);
                if let Ok(pos) = window.outer_position() {
                    if let Err(e) = state.settings_service.update(
                        "windowBounds",
                        serde_json::to_value(WindowBounds {
                            x: pos.x,
                            y: pos.y,
                            width: size.width,
                            height: size.height,
                        })
                        .unwrap_or_default(),
                    ) {
                        eprintln!("[warn] 保存窗口调整尺寸失败: {}", e);
                    }
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            notes::notes_list,
            notes::notes_search,
            notes::notes_get,
            notes::notes_create,
            notes::notes_update,
            notes::notes_pin,
            notes::notes_archive,
            notes::notes_unarchive,
            notes::notes_trash,
            notes::notes_restore,
            notes::notes_trash_many,
            notes::notes_delete_permanently_many,
            notes::notes_empty_trash,
            tags::tags_list,
            tags::tags_create,
            tags::tags_rename,
            tags::tags_delete,
            tags::tags_assign,
            attachments::attachments_add_from_clipboard,
            attachments::attachments_remove,
            attachments::attachments_get_url,
            backup::backup_export,
            backup::backup_inspect_select,
            backup::backup_restore_confirm,
            settings::settings_get_all,
            settings::settings_get,
            settings::settings_update,
            settings::settings_update_action_shortcuts,
            settings::settings_get_hotkey_status,
            settings::settings_register_hotkey,
            window::window_hide,
            window::window_confirm_hide,
            window::window_show,
            window::set_dialog_open,
            window::window_get_state,
            window::window_update_state,
            app::app_get_info,
            app::app_get_recovery_status,
            app::app_open_user_data_folder,
            app::app_quit,
            app::app_confirm_quit,
            app::app_open_external,
            app::renderer_ready,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
