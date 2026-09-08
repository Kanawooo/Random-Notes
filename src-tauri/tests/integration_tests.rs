use std::sync::Arc;
use suijian_lib::db::models::*;

use suijian_lib::db::DbService;
use suijian_lib::services::attachment_service::AttachmentService;
use suijian_lib::services::backup_service::BackupService;
use suijian_lib::services::notes_service::NotesService;
use suijian_lib::services::purge_service::PurgeService;
use suijian_lib::services::settings_service::SettingsService;
use suijian_lib::services::tags_service::TagsService;

#[allow(clippy::type_complexity)]
fn create_temp_env() -> (
    tempfile::TempDir,
    Arc<DbService>,
    Arc<NotesService>,
    Arc<TagsService>,
    Arc<AttachmentService>,
    Arc<SettingsService>,
    Arc<PurgeService>,
    Arc<BackupService>,
) {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("suijian.db");
    let attach_dir = temp_dir.path().join("attachments");

    let db_service = Arc::new(DbService::new(db_path).unwrap());
    let attachment_service = Arc::new(AttachmentService::new(db_service.get_conn(), attach_dir));
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

    (
        temp_dir,
        db_service,
        notes_service,
        tags_service,
        attachment_service,
        settings_service,
        purge_service,
        backup_service,
    )
}

#[test]
fn test_database_schema_and_migrations() {
    let (_dir, db_service, _, _, _, _, _, _) = create_temp_env();
    let conn = db_service.get_conn();
    let lock = conn.lock().unwrap();
    suijian_lib::db::migrations::verify_v1_schema_contract(&lock).unwrap();
    suijian_lib::db::migrations::verify_trigram_support(&lock).unwrap();
}

#[test]
fn test_notes_crud_and_revision() {
    let (_dir, _, notes_service, tags_service, _, _, _, _) = create_temp_env();

    // 1. Create tag
    let tag = tags_service
        .create(CreateTagInput {
            name: "工作".to_string(),
            color: Some("#4D6F9F".to_string()),
        })
        .unwrap();

    // 2. Create note
    let note = notes_service
        .create(CreateNoteInput {
            title: Some("测试便签标题".to_string()),
            content_json: None,
            plain_text: Some("这是便签的正文内容，包含中文分词测试。".to_string()),
            title_manually_edited: Some(true),
            is_pinned: Some(false),
            tag_ids: Some(vec![tag.id.clone()]),
            reuse_empty_draft: Some(false),
        })
        .unwrap();

    assert_eq!(note.title, "测试便签标题");
    assert_eq!(note.revision, 0);
    assert_eq!(note.tags.as_ref().unwrap().len(), 1);

    // 3. Update with correct revision
    let updated = notes_service
        .update(UpdateNoteInput {
            id: note.id.clone(),
            expected_revision: 0,
            title: Some("更新后的标题".to_string()),
            content_json: None,
            plain_text: Some("更新后的正文".to_string()),
            title_manually_edited: Some(true),
            is_pinned: Some(true),
            tag_ids: Some(vec![tag.id.clone()]),
        })
        .unwrap();

    assert_eq!(updated.title, "更新后的标题");
    assert_eq!(updated.revision, 1);
    assert!(updated.is_pinned);

    // 4. Update with stale revision -> Error conflict
    let conflict = notes_service.update(UpdateNoteInput {
        id: note.id.clone(),
        expected_revision: 0, // Old revision
        title: Some("冲突更新".to_string()),
        content_json: None,
        plain_text: None,
        title_manually_edited: None,
        is_pinned: None,
        tag_ids: None,
    });
    assert!(conflict.is_err());
    assert!(conflict.unwrap_err().contains("版本冲突"));
}

#[test]
fn test_fts5_trigram_and_short_queries() {
    let (_dir, _, notes_service, _, _, _, _, _) = create_temp_env();

    notes_service
        .create(CreateNoteInput {
            title: Some("机器学习实战记录".to_string()),
            content_json: None,
            plain_text: Some("深度学习自然语言处理与大模型推理优化".to_string()),
            title_manually_edited: Some(true),
            is_pinned: Some(false),
            tag_ids: None,
            reuse_empty_draft: Some(false),
        })
        .unwrap();

    // >= 3 chars trigram search
    let results_long = notes_service
        .search("语言处理", NoteScope::Active, 50)
        .unwrap();
    assert_eq!(results_long.len(), 1);
    assert_eq!(results_long[0].title, "机器学习实战记录");

    // < 3 chars LIKE search
    let results_short = notes_service.search("深度", NoteScope::Active, 50).unwrap();
    assert_eq!(results_short.len(), 1);
    assert_eq!(results_short[0].title, "机器学习实战记录");

    let results_none = notes_service
        .search("无关内容", NoteScope::Active, 50)
        .unwrap();
    assert_eq!(results_none.len(), 0);
}

#[test]
fn test_empty_draft_reuse_logic() {
    let (_dir, _, notes_service, _, _, _, _, _) = create_temp_env();

    // 1. Create with reuse_empty_draft = true
    let draft1 = notes_service
        .create(CreateNoteInput {
            title: None,
            content_json: None,
            plain_text: None,
            title_manually_edited: None,
            is_pinned: None,
            tag_ids: None,
            reuse_empty_draft: Some(true),
        })
        .unwrap();

    // 2. Immediate second create with reuse_empty_draft = true -> should return draft1
    let draft2 = notes_service
        .create(CreateNoteInput {
            title: None,
            content_json: None,
            plain_text: None,
            title_manually_edited: None,
            is_pinned: None,
            tag_ids: None,
            reuse_empty_draft: Some(true),
        })
        .unwrap();

    assert_eq!(draft1.id, draft2.id);

    // 3. User types plain text into draft1
    notes_service
        .update(UpdateNoteInput {
            id: draft1.id.clone(),
            expected_revision: 0,
            title: None,
            content_json: None,
            plain_text: Some("用户写下了第一段文字".to_string()),
            title_manually_edited: None,
            is_pinned: None,
            tag_ids: None,
        })
        .unwrap();

    // 4. Now create with reuse_empty_draft = true -> draft1 is no longer empty, must create new!
    let draft3 = notes_service
        .create(CreateNoteInput {
            title: None,
            content_json: None,
            plain_text: None,
            title_manually_edited: None,
            is_pinned: None,
            tag_ids: None,
            reuse_empty_draft: Some(true),
        })
        .unwrap();

    assert_ne!(draft1.id, draft3.id);

    // 5. Create with reuse_empty_draft = false -> creates new even if draft3 is empty!
    let draft4 = notes_service
        .create(CreateNoteInput {
            title: Some("搜索无匹配创建".to_string()),
            content_json: None,
            plain_text: None,
            title_manually_edited: None,
            is_pinned: None,
            tag_ids: None,
            reuse_empty_draft: Some(false),
        })
        .unwrap();

    assert_ne!(draft3.id, draft4.id);

    // 6. Test clear_tracked_empty_draft: after clearing, reuse_empty_draft creates a new note
    notes_service.clear_tracked_empty_draft();
    let draft5 = notes_service
        .create(CreateNoteInput {
            title: None,
            content_json: None,
            plain_text: None,
            title_manually_edited: None,
            is_pinned: None,
            tag_ids: None,
            reuse_empty_draft: Some(true),
        })
        .unwrap();
    assert_ne!(draft3.id, draft5.id);
}

#[test]
fn test_batch_operations_and_permanent_delete() {
    let (_dir, _, notes_service, _, _, _, _, _) = create_temp_env();

    let mut ids = Vec::new();
    for i in 0..5 {
        let n = notes_service
            .create(CreateNoteInput {
                title: Some(format!("便签 {}", i)),
                content_json: None,
                plain_text: Some(format!("正文 {}", i)),
                title_manually_edited: None,
                is_pinned: None,
                tag_ids: None,
                reuse_empty_draft: Some(false),
            })
            .unwrap();
        ids.push(n.id);
    }

    // Batch soft-delete to trash
    let trash_res = notes_service.trash_many(&ids).unwrap();
    assert_eq!(trash_res.affected_count, 5);

    let active_list = notes_service.list(NoteScope::Active, 50).unwrap();
    assert_eq!(active_list.len(), 0);

    let trash_list = notes_service.list(NoteScope::Trash, 50).unwrap();
    assert_eq!(trash_list.len(), 5);

    // Empty trash
    let empty_res = notes_service.empty_trash().unwrap();
    assert_eq!(empty_res.affected_count, 5);

    let trash_after = notes_service.list(NoteScope::Trash, 50).unwrap();
    assert_eq!(trash_after.len(), 0);
}

#[test]
fn test_30_days_purge_service() {
    let (_dir, db_service, notes_service, _, _, _, purge_service, _) = create_temp_env();

    let old_note = notes_service
        .create(CreateNoteInput {
            title: Some("35天前删除".to_string()),
            content_json: None,
            plain_text: None,
            title_manually_edited: None,
            is_pinned: None,
            tag_ids: None,
            reuse_empty_draft: Some(false),
        })
        .unwrap();

    let new_note = notes_service
        .create(CreateNoteInput {
            title: Some("2天前删除".to_string()),
            content_json: None,
            plain_text: None,
            title_manually_edited: None,
            is_pinned: None,
            tag_ids: None,
            reuse_empty_draft: Some(false),
        })
        .unwrap();

    // Manually set deleted_at dates
    let conn = db_service.get_conn();
    let lock = conn.lock().unwrap();
    let old_date = (chrono::Utc::now() - chrono::Duration::days(35)).to_rfc3339();
    let new_date = (chrono::Utc::now() - chrono::Duration::days(2)).to_rfc3339();

    lock.execute(
        "UPDATE notes SET deleted_at = ? WHERE id = ?",
        rusqlite::params![old_date, old_note.id],
    )
    .unwrap();
    lock.execute(
        "UPDATE notes SET deleted_at = ? WHERE id = ?",
        rusqlite::params![new_date, new_note.id],
    )
    .unwrap();
    drop(lock);

    let purged = purge_service
        .purge_deleted_notes_older_than_days(30)
        .unwrap();
    assert_eq!(purged, 1);

    assert!(notes_service.get_by_id(&old_note.id).unwrap().is_none());
    assert!(notes_service.get_by_id(&new_note.id).unwrap().is_some());
}

#[test]
fn test_attachment_validation_and_security() {
    // 1. Magic bytes
    let png_bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0];
    assert_eq!(
        AttachmentService::validate_magic_bytes(&png_bytes)
            .unwrap()
            .0,
        "image/png"
    );

    let jpeg_bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0];
    assert_eq!(
        AttachmentService::validate_magic_bytes(&jpeg_bytes)
            .unwrap()
            .0,
        "image/jpeg"
    );

    let svg_bytes = b"<svg width='100'></svg>";
    assert!(AttachmentService::validate_magic_bytes(svg_bytes).is_none());

    let exe_bytes = [0x4D, 0x5A, 0x90, 0x00];
    assert!(AttachmentService::validate_magic_bytes(&exe_bytes).is_none());
}

#[test]
fn test_backup_export_and_restore() {
    let (dir, _, notes_service, tags_service, _, _, _, backup_service) = create_temp_env();

    let tag = tags_service
        .create(CreateTagInput {
            name: "导出标签".to_string(),
            color: Some("#4D6F9F".to_string()),
        })
        .unwrap();

    notes_service
        .create(CreateNoteInput {
            title: Some("待导出便签".to_string()),
            content_json: None,
            plain_text: Some("导出备份测试正文".to_string()),
            title_manually_edited: Some(true),
            is_pinned: Some(false),
            tag_ids: Some(vec![tag.id.clone()]),
            reuse_empty_draft: Some(false),
        })
        .unwrap();

    let backup_file = dir.path().join("backup_test.zip");
    let export_res = backup_service.export_backup(&backup_file).unwrap();
    assert_eq!(export_res.note_count, Some(1));
    assert!(backup_file.exists());

    // Restore to verify
    let restore_res = backup_service.restore_backup(&backup_file).unwrap();
    assert_eq!(restore_res.restored_note_count, Some(1));
    assert_eq!(restore_res.restored_tag_count, Some(1));

    let restored_notes = notes_service.list(NoteScope::Active, 10).unwrap();
    assert_eq!(restored_notes.len(), 1);
    assert_eq!(restored_notes[0].title, "待导出便签");
}

#[test]
fn test_settings_hotkey_conflict_validation() {
    let (_dir, _, _, _, _, settings_service, _, _) = create_temp_env();

    // Default settings
    let defaults = settings_service.get_all();
    assert_eq!(defaults.hotkey, "Ctrl+Space");
    assert_eq!(defaults.shortcut_new_note, "Ctrl+N");
    assert_eq!(defaults.shortcut_back_to_search, "Ctrl+E");
    assert_eq!(defaults.shortcut_dismiss, "Escape");

    // Setting shortcutNewNote to same as shortcutBackToSearch should fail
    let conflict = settings_service.update(
        "shortcutNewNote",
        serde_json::Value::String("Ctrl+E".to_string()),
    );
    assert!(conflict.is_err());
    assert!(conflict.unwrap_err().contains("快捷键冲突"));
}

#[test]
fn test_active_note_cannot_be_permanently_deleted() {
    let (_dir, _, notes_service, _, _, _, _, _) = create_temp_env();

    let note = notes_service
        .create(CreateNoteInput {
            title: Some("未移入回收站的便签".to_string()),
            content_json: None,
            plain_text: Some("正文".to_string()),
            title_manually_edited: None,
            is_pinned: None,
            tag_ids: None,
            reuse_empty_draft: Some(false),
        })
        .unwrap();

    // Directly attempting permanent delete on an active note must affect 0 rows
    let res = notes_service
        .delete_permanently_many(std::slice::from_ref(&note.id))
        .unwrap();
    assert_eq!(res.affected_count, 0);

    // Note must still exist in active list
    let fetched = notes_service.get_by_id(&note.id).unwrap();
    assert!(fetched.is_some());

    // Only after soft-deleting can it be permanently deleted
    notes_service.trash(&note.id).unwrap();
    let res2 = notes_service
        .delete_permanently_many(std::slice::from_ref(&note.id))
        .unwrap();
    assert_eq!(res2.affected_count, 1);
    assert!(notes_service.get_by_id(&note.id).unwrap().is_none());
}

#[test]
fn test_permanent_delete_cleans_orphan_attachments() {
    let (_dir, _, notes_service, _, attachment_service, _, _, _) = create_temp_env();

    let note = notes_service
        .create(CreateNoteInput {
            title: Some("含附件便签".to_string()),
            content_json: None,
            plain_text: None,
            title_manually_edited: None,
            is_pinned: None,
            tag_ids: None,
            reuse_empty_draft: Some(false),
        })
        .unwrap();

    let png_bytes = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89,
    ];
    let att = attachment_service
        .save_image_bytes(&note.id, &png_bytes)
        .unwrap();
    let abs_path = attachment_service.get_absolute_path(&att.relative_path);
    assert!(abs_path.exists());

    // Trash and permanent delete
    notes_service.trash(&note.id).unwrap();
    let del_res = notes_service.delete_permanently_many(&[note.id]).unwrap();
    assert_eq!(del_res.affected_count, 1);

    // Physical attachment must be automatically cleaned up!
    assert!(!abs_path.exists());
}

#[test]
fn test_backup_inspect_zip_slip_and_manifest_tampering() {
    let (dir, _, _, _, _, _, _, backup_service) = create_temp_env();

    // 1. Create a malicious zip with Zip Slip path
    let bad_zip = dir.path().join("malicious_slip.zip");
    {
        let file = std::fs::File::create(&bad_zip).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("../evil.txt", options).unwrap();
        std::io::Write::write_all(&mut zip, b"malicious content").unwrap();
        zip.finish().unwrap();
    }

    let inspect_bad = backup_service.inspect_backup(&bad_zip);
    assert!(inspect_bad.is_err());
    let err_msg = inspect_bad.unwrap_err();
    assert!(err_msg.contains("路径穿越") || err_msg.contains("Zip Slip"));

    // 2. Create a zip with missing manifest.json
    let no_manifest_zip = dir.path().join("no_manifest.zip");
    {
        let file = std::fs::File::create(&no_manifest_zip).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("data/notes.json", options).unwrap();
        std::io::Write::write_all(&mut zip, b"[]").unwrap();
        zip.finish().unwrap();
    }

    let inspect_no_manifest = backup_service.inspect_backup(&no_manifest_zip);
    assert!(inspect_no_manifest.is_err());
    assert!(inspect_no_manifest.unwrap_err().contains("manifest.json"));
}

#[test]
fn test_backup_inspect_duplicate_manifest_paths() {
    let (dir, _, _, _, _, _, _, backup_service) = create_temp_env();

    let dup_zip = dir.path().join("dup_manifest.zip");
    {
        let file = std::fs::File::create(&dup_zip).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        let manifest = serde_json::json!({
            "version": 1,
            "appVersion": "0.1.0",
            "exportedAt": "2026-01-01T00:00:00Z",
            "files": [
                {
                    "path": "data/notes.json",
                    "sha256": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945",
                    "byteSize": 2
                },
                {
                    "path": "data/notes.json",
                    "sha256": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945",
                    "byteSize": 2
                }
            ]
        });

        zip.start_file("manifest.json", options).unwrap();
        std::io::Write::write_all(
            &mut zip,
            serde_json::to_string(&manifest).unwrap().as_bytes(),
        )
        .unwrap();

        zip.start_file("data/notes.json", options).unwrap();
        std::io::Write::write_all(&mut zip, b"[]").unwrap();

        zip.finish().unwrap();
    }

    let inspect_res = backup_service.inspect_backup(&dup_zip);
    assert!(inspect_res.is_err());
    assert!(inspect_res.unwrap_err().contains("重复的文件条目路径"));
}

#[test]
fn test_backup_inspect_unreferenced_or_mismatched_asset() {
    let (dir, _, _, _, _, _, _, backup_service) = create_temp_env();

    // 1. Unreferenced asset in manifest
    let unref_zip = dir.path().join("unref_asset.zip");
    {
        let file = std::fs::File::create(&unref_zip).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        let manifest = serde_json::json!({
            "version": 1,
            "appVersion": "0.1.0",
            "exportedAt": "2026-01-01T00:00:00Z",
            "files": [
                {
                    "path": "assets/orphan.png",
                    "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                    "byteSize": 0
                }
            ]
        });

        zip.start_file("manifest.json", options).unwrap();
        std::io::Write::write_all(
            &mut zip,
            serde_json::to_string(&manifest).unwrap().as_bytes(),
        )
        .unwrap();

        zip.start_file("assets/orphan.png", options).unwrap();

        zip.finish().unwrap();
    }

    let inspect_unref = backup_service.inspect_backup(&unref_zip);
    assert!(inspect_unref.is_err());
    assert!(inspect_unref
        .unwrap_err()
        .contains("未被任何便签引用的附件"));
}

#[test]
fn test_tag_deletion_syncs_fts() {
    let (_dir, _, notes_service, tags_service, _, _, _, _) = create_temp_env();

    let tag = tags_service
        .create(CreateTagInput {
            name: "重点项目".to_string(),
            color: None,
        })
        .unwrap();

    let note = notes_service
        .create(CreateNoteInput {
            title: Some("普通标题".to_string()),
            content_json: None,
            plain_text: Some("正文内容".to_string()),
            title_manually_edited: None,
            is_pinned: None,
            tag_ids: Some(vec![tag.id.clone()]),
            reuse_empty_draft: Some(false),
        })
        .unwrap();

    // Before deletion: FTS finds note by tag name
    let search_before = notes_service
        .search("重点项目", NoteScope::Active, 50)
        .unwrap();
    assert_eq!(search_before.len(), 1);
    assert_eq!(search_before[0].id, note.id);

    // Delete tag
    tags_service.delete(&tag.id).unwrap();

    // After deletion: FTS must no longer match the deleted tag name!
    let search_after = notes_service
        .search("重点项目", NoteScope::Active, 50)
        .unwrap();
    assert_eq!(search_after.len(), 0);
}

#[test]
fn test_shortcut_action_atomic_update_and_conflict() {
    let (_dir, _, _, _, _, settings_service, _, _) = create_temp_env();

    // 1. validate_and_normalize_shortcut checks
    assert_eq!(
        SettingsService::validate_and_normalize_shortcut("ctrl + n", false).unwrap(),
        "Ctrl+N"
    );
    assert_eq!(
        SettingsService::validate_and_normalize_shortcut("Shift+Ctrl+N", false).unwrap(),
        "Ctrl+Shift+N"
    );
    assert_eq!(
        SettingsService::validate_and_normalize_shortcut("Super+Alt+Shift+Ctrl+K", false).unwrap(),
        "Ctrl+Alt+Shift+Super+K"
    );
    assert!(SettingsService::validate_and_normalize_shortcut("Banana", false).is_err());
    assert!(SettingsService::validate_and_normalize_shortcut("Ctrl+Ctrl+N", false).is_err());
    assert!(SettingsService::validate_and_normalize_shortcut("Ctrl", false).is_err());

    // 2. Conflict between action shortcuts
    let err_conflict = settings_service.update_action_shortcuts("Ctrl+N", "Ctrl+N", "Escape");
    assert!(err_conflict.is_err());
    assert!(err_conflict.unwrap_err().contains("冲突"));

    // 3. Valid atomic update
    let ok = settings_service.update_action_shortcuts("Ctrl+Alt+N", "Ctrl+Alt+B", "Ctrl+Alt+D");
    assert!(ok.is_ok());
    let s = settings_service.get_all();
    assert_eq!(s.shortcut_new_note, "Ctrl+Alt+N");
    assert_eq!(s.shortcut_back_to_search, "Ctrl+Alt+B");
    assert_eq!(s.shortcut_dismiss, "Ctrl+Alt+D");
}
