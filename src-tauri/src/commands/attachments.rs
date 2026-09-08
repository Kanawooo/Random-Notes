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
pub async fn attachments_add_from_clipboard(
    app: AppHandle,
    note_id: String,
) -> Result<Attachment, String> {
    let state = app.state::<AppState>();
    check_write_permission(&state)?;
    if !validate_uuid(&note_id) {
        return Err("无效的便签 UUID".to_string());
    }
    let svc = state.attachment_service.clone();
    drop(state);
    // arboard Clipboard 在阻塞线程内创建与读取（同步命令原在主线程执行，arboard 读+PNG 重编码会冻结 UI）
    tauri::async_runtime::spawn_blocking(move || svc.save_from_clipboard(&note_id))
        .await
        .map_err(|e| format!("剪贴板读取任务执行失败: {}", e))?
}

/// 仓库首批 async 命令之一：入口校验与 base64 解码在 async 上下文完成，
/// sha256/图片解码/写盘等 CPU+IO 密集工作移交 blocking 池（spawn_blocking），
/// 不在 tokio worker 线程上同步阻塞；用 owned AppHandle 而非 State<'_> 参数，规避 async + 生命周期的编译风险
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
    let svc = state.attachment_service.clone();
    drop(state);
    tauri::async_runtime::spawn_blocking(move || svc.save_image_bytes(&note_id, &bytes))
        .await
        .map_err(|e| format!("附件处理任务执行失败: {}", e))?
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
