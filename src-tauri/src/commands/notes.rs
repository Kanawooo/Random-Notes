use crate::db::models::{BatchResult, CreateNoteInput, Note, NoteScope, UpdateNoteInput};
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
pub fn notes_list(
    state: State<AppState>,
    scope: Option<NoteScope>,
    limit: Option<usize>,
) -> Result<Vec<Note>, String> {
    let safe_limit = limit.unwrap_or(50).min(500);
    state
        .notes_service
        .list(scope.unwrap_or(NoteScope::Active), safe_limit)
}

#[tauri::command]
pub fn notes_search(
    state: State<AppState>,
    query: String,
    scope: Option<NoteScope>,
    limit: Option<usize>,
) -> Result<Vec<Note>, String> {
    if query.chars().count() > 500 {
        return Err("搜索关键词过长（上限 500 字符）".to_string());
    }
    let safe_limit = limit.unwrap_or(50).min(500);
    state
        .notes_service
        .search(&query, scope.unwrap_or(NoteScope::Active), safe_limit)
}

#[tauri::command]
pub fn notes_get(state: State<AppState>, id: String) -> Result<Option<Note>, String> {
    if !validate_uuid(&id) {
        return Err(format!("无效的 UUID 格式: {}", id));
    }
    state.notes_service.get_by_id(&id)
}

#[tauri::command]
pub fn notes_create(state: State<AppState>, input: CreateNoteInput) -> Result<Note, String> {
    check_write_permission(&state)?;

    if let Some(t) = &input.title {
        if t.chars().count() > 255 {
            return Err("便签标题过长（上限 255 字符）".to_string());
        }
    }
    if let Some(json) = &input.content_json {
        if serde_json::from_str::<serde_json::Value>(json).is_err() {
            return Err("便签正文 content_json 不是有效的 JSON 格式".to_string());
        }
    }
    if let Some(txt) = &input.plain_text {
        if txt.len() > 2_000_000 {
            return Err("便签纯文本正文大小超限".to_string());
        }
    }
    if let Some(tags) = &input.tag_ids {
        if tags.len() > 50 {
            return Err("单篇便签关联标签不能超过 50 个".to_string());
        }
        for tag_id in tags {
            if !validate_uuid(tag_id) {
                return Err(format!("关联标签 UUID 格式无效: {}", tag_id));
            }
        }
    }

    state.notes_service.create(input)
}

#[tauri::command]
pub fn notes_update(state: State<AppState>, input: UpdateNoteInput) -> Result<Note, String> {
    check_write_permission(&state)?;

    if !validate_uuid(&input.id) {
        return Err(format!("无效的 UUID 格式: {}", input.id));
    }
    if let Some(t) = &input.title {
        if t.chars().count() > 255 {
            return Err("便签标题过长（上限 255 字符）".to_string());
        }
    }
    if let Some(json) = &input.content_json {
        if serde_json::from_str::<serde_json::Value>(json).is_err() {
            return Err("便签正文 content_json 不是有效的 JSON 格式".to_string());
        }
    }
    if let Some(txt) = &input.plain_text {
        if txt.len() > 2_000_000 {
            return Err("便签纯文本正文大小超限".to_string());
        }
    }
    if let Some(tags) = &input.tag_ids {
        if tags.len() > 50 {
            return Err("单篇便签关联标签不能超过 50 个".to_string());
        }
        for tag_id in tags {
            if !validate_uuid(tag_id) {
                return Err(format!("关联标签 UUID 格式无效: {}", tag_id));
            }
        }
    }

    state.notes_service.update(input)
}

#[tauri::command]
pub fn notes_pin(state: State<AppState>, id: String, is_pinned: bool) -> Result<Note, String> {
    check_write_permission(&state)?;
    if !validate_uuid(&id) {
        return Err(format!("无效的 UUID 格式: {}", id));
    }
    state.notes_service.set_pin(&id, is_pinned)
}

#[tauri::command]
pub fn notes_archive(state: State<AppState>, id: String) -> Result<Note, String> {
    check_write_permission(&state)?;
    if !validate_uuid(&id) {
        return Err(format!("无效的 UUID 格式: {}", id));
    }
    state.notes_service.archive(&id)
}

#[tauri::command]
pub fn notes_unarchive(state: State<AppState>, id: String) -> Result<Note, String> {
    check_write_permission(&state)?;
    if !validate_uuid(&id) {
        return Err(format!("无效的 UUID 格式: {}", id));
    }
    state.notes_service.unarchive(&id)
}

#[tauri::command]
pub fn notes_trash(state: State<AppState>, id: String) -> Result<Note, String> {
    check_write_permission(&state)?;
    if !validate_uuid(&id) {
        return Err(format!("无效的 UUID 格式: {}", id));
    }
    state.notes_service.trash(&id)
}

#[tauri::command]
pub fn notes_restore(state: State<AppState>, id: String) -> Result<Note, String> {
    check_write_permission(&state)?;
    if !validate_uuid(&id) {
        return Err(format!("无效的 UUID 格式: {}", id));
    }
    state.notes_service.restore(&id)
}

#[tauri::command]
pub fn notes_trash_many(state: State<AppState>, ids: Vec<String>) -> Result<BatchResult, String> {
    check_write_permission(&state)?;
    if ids.len() > 1000 {
        return Err("批量操作数量超过 1000 上限".to_string());
    }
    for id in &ids {
        if !validate_uuid(id) {
            return Err(format!("无效的 UUID 格式: {}", id));
        }
    }
    state.notes_service.trash_many(&ids)
}

#[tauri::command]
pub fn notes_delete_permanently_many(
    state: State<AppState>,
    ids: Vec<String>,
) -> Result<BatchResult, String> {
    check_write_permission(&state)?;
    if ids.len() > 1000 {
        return Err("批量操作数量超过 1000 上限".to_string());
    }
    for id in &ids {
        if !validate_uuid(id) {
            return Err(format!("无效的 UUID 格式: {}", id));
        }
    }
    state.notes_service.delete_permanently_many(&ids)
}

#[tauri::command]
pub fn notes_empty_trash(state: State<AppState>) -> Result<BatchResult, String> {
    check_write_permission(&state)?;
    state.notes_service.empty_trash()
}
