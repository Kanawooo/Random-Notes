use crate::db::models::Attachment;
use crate::utils::paths::validate_uuid;
use crate::AppState;
use tauri::State;

fn check_write_permission(state: &AppState) -> Result<(), String> {
    if let Some(err) = &state.read_only_recovery_error {
        return Err(format!("系统处于只读保护模式，禁止写入或修改数据：{}", err));
    }
    Ok(())
}

#[tauri::command]
pub fn attachments_add_from_clipboard(
    state: State<AppState>,
    note_id: String,
) -> Result<Attachment, String> {
    check_write_permission(&state)?;
    if !validate_uuid(&note_id) {
        return Err("无效的便签 UUID".to_string());
    }
    state.attachment_service.save_from_clipboard(&note_id)
}

#[tauri::command]
pub fn attachments_remove(state: State<AppState>, id: String) -> Result<(), String> {
    check_write_permission(&state)?;
    if !validate_uuid(&id) {
        return Err("无效的附件 UUID".to_string());
    }
    state.attachment_service.delete(&id)
}

#[tauri::command]
pub fn attachments_get_url(id: String) -> Result<String, String> {
    if !validate_uuid(&id) {
        return Err("无效的附件 UUID".to_string());
    }
    Ok(format!("suijian-attachment://{}", id))
}
