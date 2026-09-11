use rusqlite::{params, Connection};

pub const LATEST_MIGRATION_VERSION: i64 = 1;

pub fn verify_trigram_support(conn: &Connection) -> Result<(), String> {
    let probe_table = format!("temp._probe_trigram_{}", uuid::Uuid::new_v4().simple());
    let sql = format!(
        "CREATE VIRTUAL TABLE {} USING fts5(content, tokenize='trigram'); DROP TABLE {};",
        probe_table, probe_table
    );
    conn.execute_batch(&sql).map_err(|e| {
        format!(
            "Fatal: SQLite build does not support FTS5 trigram tokenizer ({}). Ensure SQLite is compiled with FTS5 and trigram tokenizer enabled.",
            e
        )
    })
}

pub fn check_migrations_integrity(
    applied_versions: &[i64],
    max_allowed: i64,
) -> Result<(), String> {
    for &v in applied_versions {
        if v <= 0 {
            return Err(format!("Invalid migration version ({}) recorded in database. Versions must be positive integers.", v));
        }
    }

    if applied_versions.is_empty() {
        return Ok(());
    }

    let max_applied = *applied_versions.iter().max().unwrap();
    if max_applied > max_allowed {
        return Err(format!(
            "数据库版本 ({}) 高于当前程序支持的最高版本 ({})。请使用最新版本的随笺打开此数据库。",
            max_applied, max_allowed
        ));
    }

    let mut sorted = applied_versions.to_vec();
    sorted.sort_unstable();
    for (i, &v) in sorted.iter().enumerate() {
        let expected = (i + 1) as i64;
        if v != expected {
            return Err(format!(
                "Migration version gap or invalid sequence detected: expected version {} at sequence index {}, but found {}",
                expected, i, v
            ));
        }
    }

    Ok(())
}

pub fn verify_v1_schema_contract(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%'")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;

    let table_names: std::collections::HashSet<String> = rows
        .collect::<Result<std::collections::HashSet<String>, _>>()
        .map_err(|e| e.to_string())?;

    let required_tables = [
        "notes",
        "tags",
        "note_tags",
        "attachments",
        "settings",
        "notes_fts",
    ];
    for &req_table in &required_tables {
        if !table_names.contains(req_table) {
            return Err(format!(
                "随笺 v1 架构契约校验失败：缺失表 \"{}\"",
                req_table
            ));
        }
    }

    // Verify notes columns
    let mut pragma_stmt = conn
        .prepare("PRAGMA table_info(notes)")
        .map_err(|e| e.to_string())?;
    let note_cols: Vec<String> = pragma_stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let req_note_cols = [
        "id",
        "title",
        "content_json",
        "plain_text",
        "title_manually_edited",
        "is_pinned",
        "archived_at",
        "deleted_at",
        "revision",
        "created_at",
        "updated_at",
    ];
    for col in req_note_cols {
        if !note_cols.contains(&col.to_string()) {
            return Err(format!(
                "随笺 v1 架构契约校验失败：notes 表缺失列 \"{}\"",
                col
            ));
        }
    }

    // Verify tags columns
    let mut pragma_tags = conn
        .prepare("PRAGMA table_info(tags)")
        .map_err(|e| e.to_string())?;
    let tag_cols: Vec<String> = pragma_tags
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for col in ["id", "name", "normalized_name", "color", "created_at"] {
        if !tag_cols.contains(&col.to_string()) {
            return Err(format!(
                "随笺 v1 架构契约校验失败：tags 表缺失列 \"{}\"",
                col
            ));
        }
    }

    // Verify attachments columns（列清单取自 v1 迁移建表语句）
    let mut pragma_attachments = conn
        .prepare("PRAGMA table_info(attachments)")
        .map_err(|e| e.to_string())?;
    let attachment_cols: Vec<String> = pragma_attachments
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for col in [
        "id",
        "note_id",
        "relative_path",
        "mime_type",
        "byte_size",
        "width",
        "height",
        "sha256",
        "created_at",
    ] {
        if !attachment_cols.contains(&col.to_string()) {
            return Err(format!(
                "随笺 v1 架构契约校验失败：attachments 表缺失列 \"{}\"",
                col
            ));
        }
    }

    // Verify note_tags columns
    let mut pragma_note_tags = conn
        .prepare("PRAGMA table_info(note_tags)")
        .map_err(|e| e.to_string())?;
    let note_tag_cols: Vec<String> = pragma_note_tags
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for col in ["note_id", "tag_id"] {
        if !note_tag_cols.contains(&col.to_string()) {
            return Err(format!(
                "随笺 v1 架构契约校验失败：note_tags 表缺失列 \"{}\"",
                col
            ));
        }
    }

    // Verify foreign keys：v1 建表已声明外键，故逐条校验；
    // foreign_key_list 返回列序为 id, seq, table, from, to, on_update, on_delete, match
    let fk_specs = [
        ("attachments", "note_id", "notes"),
        ("note_tags", "note_id", "notes"),
        ("note_tags", "tag_id", "tags"),
    ];
    for (table, from_col, ref_table) in fk_specs {
        let mut fk_stmt = conn
            .prepare(&format!("PRAGMA foreign_key_list({})", table))
            .map_err(|e| e.to_string())?;
        let fks: Vec<(String, String)> = fk_stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(2)?, row.get::<_, String>(3)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if !fks.iter().any(|(t, f)| t == ref_table && f == from_col) {
            return Err(format!(
                "随笺 v1 架构契约校验失败：{} 表缺失外键 {} → {}(id)",
                table, from_col, ref_table
            ));
        }
    }

    // Verify settings columns
    let mut pragma_settings = conn
        .prepare("PRAGMA table_info(settings)")
        .map_err(|e| e.to_string())?;
    let setting_cols: Vec<String> = pragma_settings
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for col in ["key", "value_json", "updated_at"] {
        if !setting_cols.contains(&col.to_string()) {
            return Err(format!(
                "随笺 v1 架构契约校验失败：settings 表缺失列 \"{}\"",
                col
            ));
        }
    }

    // Verify notes_fts is FTS5 with trigram
    // 区分 NoRows 与真实错误：NoRows 才按“缺失虚拟表”处理；
    // 其他错误（如元数据读取失败）按真实原因报告，避免误导排障方向
    let fts_sql: Option<String> = match conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='notes_fts'",
        [],
        |r| r.get(0),
    ) {
        Ok(sql) => Some(sql),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(e.to_string()),
    };

    if let Some(sql) = fts_sql {
        let sql_lower = sql.to_lowercase();
        if !sql_lower.contains("using fts5") || !sql_lower.contains("trigram") {
            return Err(
                "随笺 v1 架构契约校验失败：\"notes_fts\" 必须是使用 trigram 分词器的 FTS5 虚拟表"
                    .to_string(),
            );
        }
    } else {
        return Err("随笺 v1 架构契约校验失败：缺失虚拟表 \"notes_fts\"".to_string());
    }

    Ok(())
}

pub fn run_migrations(conn: &mut Connection) -> Result<(), String> {
    verify_trigram_support(conn)?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )
    .map_err(|e| e.to_string())?;

    let applied_versions: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version ASC")
            .map_err(|e| e.to_string())?;
        let applied_rows = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        applied_rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    check_migrations_integrity(&applied_versions, LATEST_MIGRATION_VERSION)?;

    if !applied_versions.contains(&1) {
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS notes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content_json TEXT NOT NULL,
                plain_text TEXT NOT NULL,
                title_manually_edited INTEGER NOT NULL DEFAULT 0 CHECK(title_manually_edited IN (0, 1)),
                is_pinned INTEGER NOT NULL DEFAULT 0 CHECK(is_pinned IN (0, 1)),
                archived_at TEXT NULL,
                deleted_at TEXT NULL,
                revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_notes_updated_at ON notes(updated_at);
            CREATE INDEX IF NOT EXISTS idx_notes_pinned_updated ON notes(is_pinned, updated_at);
            CREATE INDEX IF NOT EXISTS idx_notes_archived ON notes(archived_at);
            CREATE INDEX IF NOT EXISTS idx_notes_deleted ON notes(deleted_at);

            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                normalized_name TEXT NOT NULL UNIQUE,
                color TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_tags_normalized_name ON tags(normalized_name);

            CREATE TABLE IF NOT EXISTS note_tags (
                note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
                tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY(note_id, tag_id)
            );

            CREATE INDEX IF NOT EXISTS idx_note_tags_tag ON note_tags(tag_id);

            CREATE TABLE IF NOT EXISTS attachments (
                id TEXT PRIMARY KEY,
                note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
                relative_path TEXT NOT NULL UNIQUE,
                mime_type TEXT NOT NULL,
                byte_size INTEGER NOT NULL CHECK(byte_size >= 0),
                width INTEGER NULL CHECK(width IS NULL OR width > 0),
                height INTEGER NULL CHECK(height IS NULL OR height > 0),
                sha256 TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_attachments_note ON attachments(note_id);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
                note_id UNINDEXED,
                title,
                plain_text,
                tags_text,
                tokenize='trigram'
            );"
        ).map_err(|e| e.to_string())?;

        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?, ?)",
            params![1, now],
        )
        .map_err(|e| e.to_string())?;

        tx.commit().map_err(|e| e.to_string())?;
    }

    Ok(())
}
