use crate::db::models::{AppSettings, WindowBounds};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::sync::{Arc, Mutex};

pub struct SettingsService {
    conn: Arc<Mutex<Connection>>,
}

impl SettingsService {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn get_raw(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        conn.query_row(
            "SELECT value_json FROM settings WHERE key = ?",
            params![key],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn set_raw(&self, key: &str, value_json: &str) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            params![key, value_json, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_all(&self) -> AppSettings {
        let mut settings = AppSettings::default();

        if let Ok(Some(val)) = self.get_raw("hotkey") {
            if let Ok(s) = serde_json::from_str::<String>(&val) {
                if !s.trim().is_empty() {
                    settings.hotkey = s;
                }
            }
        }

        if let Ok(Some(val)) = self.get_raw("shortcutNewNote") {
            if let Ok(s) = serde_json::from_str::<String>(&val) {
                if !s.trim().is_empty() {
                    settings.shortcut_new_note = s;
                }
            }
        }

        if let Ok(Some(val)) = self.get_raw("shortcutBackToSearch") {
            if let Ok(s) = serde_json::from_str::<String>(&val) {
                if !s.trim().is_empty() {
                    settings.shortcut_back_to_search = s;
                }
            }
        }

        if let Ok(Some(val)) = self.get_raw("shortcutDismiss") {
            if let Ok(s) = serde_json::from_str::<String>(&val) {
                if !s.trim().is_empty() {
                    settings.shortcut_dismiss = s;
                }
            }
        }

        if let Ok(Some(val)) = self.get_raw("launchAtLogin") {
            if let Ok(b) = serde_json::from_str::<bool>(&val) {
                settings.launch_at_login = b;
            }
        }

        if let Ok(Some(val)) = self.get_raw("autoHideOnBlur") {
            if let Ok(b) = serde_json::from_str::<bool>(&val) {
                settings.auto_hide_on_blur = b;
            }
        }

        if let Ok(Some(val)) = self.get_raw("windowBounds") {
            if let Ok(b) = serde_json::from_str::<Option<WindowBounds>>(&val) {
                settings.window_bounds = b;
            }
        }

        settings
    }

    pub fn validate_and_normalize_shortcut(input: &str, is_global: bool) -> Result<String, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("快捷键不能为空".to_string());
        }
        let parts: Vec<&str> = trimmed
            .split('+')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if parts.is_empty() {
            return Err("无效的快捷键".to_string());
        }

        let mut has_ctrl = false;
        let mut has_alt = false;
        let mut has_shift = false;
        let mut has_super = false;
        let mut key_code: Option<String> = None;

        for p in parts {
            let lower = p.to_lowercase();
            match lower.as_str() {
                "ctrl" | "control" => {
                    if has_ctrl {
                        return Err("重复的修饰键: Ctrl".to_string());
                    }
                    has_ctrl = true;
                }
                "alt" => {
                    if has_alt {
                        return Err("重复的修饰键: Alt".to_string());
                    }
                    has_alt = true;
                }
                "shift" => {
                    if has_shift {
                        return Err("重复的修饰键: Shift".to_string());
                    }
                    has_shift = true;
                }
                "super" | "win" | "meta" => {
                    if has_super {
                        return Err("重复的修饰键: Super".to_string());
                    }
                    has_super = true;
                }
                _ => {
                    if key_code.is_some() {
                        return Err("快捷键只能包含一个主按键".to_string());
                    }
                    let normalized_key = match lower.as_str() {
                        "space" => "Space".to_string(),
                        "esc" | "escape" => "Escape".to_string(),
                        "enter" | "return" => "Enter".to_string(),
                        "tab" => "Tab".to_string(),
                        "backspace" => "Backspace".to_string(),
                        "delete" | "del" => "Delete".to_string(),
                        "insert" | "ins" => "Insert".to_string(),
                        "home" => "Home".to_string(),
                        "end" => "End".to_string(),
                        "pageup" | "pgup" => "PageUp".to_string(),
                        "pagedown" | "pgdn" => "PageDown".to_string(),
                        "arrowup" | "up" => "Up".to_string(),
                        "arrowdown" | "down" => "Down".to_string(),
                        "arrowleft" | "left" => "Left".to_string(),
                        "arrowright" | "right" => "Right".to_string(),
                        s if s.len() == 1 && s.chars().next().unwrap().is_ascii_alphanumeric() => {
                            s.to_uppercase()
                        }
                        s if s.len() == 1 && "~!@#$%^&*()_+{}|:\"<>?`-=[]\\;',./".contains(s) => {
                            s.to_string()
                        }
                        s if s.starts_with('f')
                            && s.len() <= 3
                            && s[1..]
                                .parse::<u8>()
                                .map(|n| (1..=24).contains(&n))
                                .unwrap_or(false) =>
                        {
                            s.to_uppercase()
                        }
                        _ => return Err(format!("无法识别的按键: \"{}\"", p)),
                    };
                    key_code = Some(normalized_key);
                }
            }
        }

        let key = key_code.ok_or_else(|| "快捷键缺少主按键".to_string())?;
        let mut result_parts = Vec::new();
        if has_ctrl {
            result_parts.push("Ctrl");
        }
        if has_alt {
            result_parts.push("Alt");
        }
        if has_shift {
            result_parts.push("Shift");
        }
        if has_super {
            result_parts.push("Super");
        }
        result_parts.push(&key);
        let result = result_parts.join("+");

        // 非全局的动作快捷键不接受裸字符键：注册后打字会反复命中全局处理器吞掉输入
        let has_modifier = has_ctrl || has_alt || has_shift || has_super;
        if !is_global && !has_modifier && (key.len() == 1 || key == "Space") {
            return Err(format!(
                "动作快捷键不能用裸的字符键 \"{}\"（会占用该字符的输入）；请搭配 Ctrl/Alt/Shift，或改用 F1–F24 等功能键",
                key
            ));
        }

        if is_global
            && result
                .parse::<tauri_plugin_global_shortcut::Shortcut>()
                .is_err()
        {
            return Err(format!("全局呼出快捷键格式无效: {}", result));
        }

        Ok(result)
    }

    pub fn normalize_shortcut(input: &str) -> Result<String, String> {
        Self::validate_and_normalize_shortcut(input, false)
    }

    /// 四元快捷键唯一性校验（update 与 update_action_shortcuts 共用）；错误文案必须与历史行为一致
    fn ensure_shortcuts_unique(
        hotkey: &str,
        new_note: &str,
        back_to_search: &str,
        dismiss: &str,
    ) -> Result<(), String> {
        let shortcuts = [
            ("全局呼出", hotkey),
            ("新建便签", new_note),
            ("返回搜索", back_to_search),
            ("取消/隐藏", dismiss),
        ];

        for i in 0..shortcuts.len() {
            for j in (i + 1)..shortcuts.len() {
                if shortcuts[i].1.eq_ignore_ascii_case(shortcuts[j].1) {
                    return Err(format!(
                        "快捷键冲突：\"{}\" 与 \"{}\" 均为 \"{}\"",
                        shortcuts[i].0, shortcuts[j].0, shortcuts[i].1
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn update_action_shortcuts(
        &self,
        new_note: &str,
        back_to_search: &str,
        dismiss: &str,
    ) -> Result<AppSettings, String> {
        let current = self.get_all();

        let n_norm = Self::validate_and_normalize_shortcut(new_note, false)?;
        let b_norm = Self::validate_and_normalize_shortcut(back_to_search, false)?;
        let d_norm = Self::validate_and_normalize_shortcut(dismiss, false)?;

        Self::ensure_shortcuts_unique(&current.hotkey, &n_norm, &b_norm, &d_norm)?;

        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "Database lock failed".to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().to_rfc3339();

        for (k, v) in [
            ("shortcutNewNote", &n_norm),
            ("shortcutBackToSearch", &b_norm),
            ("shortcutDismiss", &d_norm),
        ] {
            let val_json = serde_json::to_string(v).map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
                params![k, val_json, now],
            )
            .map_err(|e| e.to_string())?;
        }

        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);

        Ok(self.get_all())
    }

    pub fn update(&self, key: &str, value: Value) -> Result<AppSettings, String> {
        let current = self.get_all();

        // 1. Only allow fixed keys with correct types
        let val_to_store: Value = match key {
            "hotkey" => {
                let s = value
                    .as_str()
                    .ok_or_else(|| format!("{} 必须是字符串", key))?;
                let normalized = Self::validate_and_normalize_shortcut(s, true)?;
                Value::String(normalized)
            }
            "shortcutNewNote" | "shortcutBackToSearch" | "shortcutDismiss" => {
                let s = value
                    .as_str()
                    .ok_or_else(|| format!("{} 必须是字符串", key))?;
                let normalized = Self::validate_and_normalize_shortcut(s, false)?;
                Value::String(normalized)
            }
            "launchAtLogin" | "autoHideOnBlur" => {
                if !value.is_boolean() {
                    return Err(format!("{} 必须是布尔值", key));
                }
                value
            }
            "windowBounds" => {
                if !value.is_null() {
                    serde_json::from_value::<WindowBounds>(value.clone())
                        .map_err(|e| format!("windowBounds 格式错误: {}", e))?;
                }
                value
            }
            _ => return Err(format!("不支持的设置项: {}", key)),
        };

        // 2. Validate shortcut uniqueness if changing a shortcut
        if key == "hotkey"
            || key == "shortcutNewNote"
            || key == "shortcutBackToSearch"
            || key == "shortcutDismiss"
        {
            let new_shortcut = val_to_store.as_str().unwrap().to_string();
            let mut test_settings = current.clone();
            match key {
                "hotkey" => test_settings.hotkey = new_shortcut,
                "shortcutNewNote" => test_settings.shortcut_new_note = new_shortcut,
                "shortcutBackToSearch" => test_settings.shortcut_back_to_search = new_shortcut,
                "shortcutDismiss" => test_settings.shortcut_dismiss = new_shortcut,
                _ => {}
            }

            Self::ensure_shortcuts_unique(
                &test_settings.hotkey,
                &test_settings.shortcut_new_note,
                &test_settings.shortcut_back_to_search,
                &test_settings.shortcut_dismiss,
            )?;
        }

        let json_str = serde_json::to_string(&val_to_store).map_err(|e| e.to_string())?;
        self.set_raw(key, &json_str)?;

        Ok(self.get_all())
    }
}
