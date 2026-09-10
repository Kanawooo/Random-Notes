use crate::db::models::{AssignTagsInput, CreateTagInput, RenameTagInput, Tag};
use crate::utils::paths::validate_uuid;
use crate::AppState;
use tauri::{AppHandle, Manager, State};

fn check_write_permission(state: &AppState) -> Result<(), String> {
    if let Some(err) = &state.read_only_recovery_error {
        return Err(format!("系统处于只读保护模式，禁止写入或修改数据：{}", err));
    }
    Ok(())
}

fn validate_hex_color(c: &str) -> bool {
    let t = c.trim();
    (t.len() == 7 && t.starts_with('#') && t[1..].chars().all(|ch| ch.is_ascii_hexdigit()))
        || (t.len() == 6 && t.chars().all(|ch| ch.is_ascii_hexdigit()))
}

#[tauri::command]
pub fn tags_list(state: State<AppState>) -> Result<Vec<Tag>, String> {
    state.tags_service.list()
}

#[tauri::command]
pub fn tags_create(state: State<AppState>, input: CreateTagInput) -> Result<Tag, String> {
    check_write_permission(&state)?;
    let name_len = input.name.trim().chars().count();
    if name_len == 0 || name_len > 50 {
        return Err("标签名称不能为空且不能超过 50 个字符".to_string());
    }
    if let Some(ref color) = input.color {
        if !validate_hex_color(color) {
            return Err("标签颜色格式无效，必须是 6 位十六进制颜色值".to_string());
        }
    }
    state.tags_service.create(input)
}

#[tauri::command]
pub fn tags_rename(state: State<AppState>, input: RenameTagInput) -> Result<Tag, String> {
    check_write_permission(&state)?;
    if !validate_uuid(&input.id) {
        return Err("无效的标签 UUID".to_string());
    }
    let name_len = input.name.trim().chars().count();
    if name_len == 0 || name_len > 50 {
        return Err("标签名称不能为空且不能超过 50 个字符".to_string());
    }
    state.tags_service.rename(input)
}

#[tauri::command]
pub async fn tags_delete(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    check_write_permission(&state)?;
    if !validate_uuid(&id) {
        return Err("无效的标签 UUID".to_string());
    }
    let svc = state.tags_service.clone();
    drop(state);
    tauri::async_runtime::spawn_blocking(move || svc.delete(&id))
        .await
        .map_err(|e| format!("删除标签任务执行失败: {}", e))?
}

#[tauri::command]
pub fn tags_assign(state: State<AppState>, input: AssignTagsInput) -> Result<(), String> {
    check_write_permission(&state)?;
    if !validate_uuid(&input.note_id) {
        return Err("无效的便签 UUID".to_string());
    }
    if input.tag_ids.len() > 50 {
        return Err("单篇便签关联标签不能超过 50 个".to_string());
    }
    for tag_id in &input.tag_ids {
        if !validate_uuid(tag_id) {
            return Err(format!("无效的标签 UUID: {}", tag_id));
        }
    }
    state.tags_service.assign(input)
}
