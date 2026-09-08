use base64::Engine;
use crate::db::models::Attachment;
use crate::utils::paths::validate_uuid;
use crate::AppState;
use tauri::{AppHandle, Manager, State};

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

/// 仓库首个 async 命令：base64 解码、sha256、图片尺寸解码与写盘在异步线程池执行，不阻塞主线程；
/// 用 owned AppHandle 而非 State<'_> 参数，规避 async + 生命周期的编译风险
#[tauri::command]
pub async fn attachments_add_from_bytes(
    app: AppHandle,
    note_id: String,
    data: String,
) -> Result<Attachment, String> {
    let state = app.state::<AppState>();
    check_write_permission(&state)?;
    if !validate_uuid(&note_id) {
        return Err("无效的便签 UUID".to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .map_err(|_| "粘贴数据解析失败".to_string())?;
    state.attachment_service.save_image_bytes(&note_id, &bytes)
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
    // Windows WebView2 只拦截 http://<scheme>.localhost/<path> 形式；返回可直接渲染的显示 URL
    Ok(format!("http://suijian-attachment.localhost/{}", id))
}
