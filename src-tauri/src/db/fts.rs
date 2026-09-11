use rusqlite::{params, Connection};

pub fn sanitize_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return "\"\"".to_string();
    }
    let escaped = trimmed.replace('"', "\"\"");
    format!("\"{}\"", escaped)
}

pub fn sync_note(
    conn: &Connection,
    note_id: &str,
    title: &str,
    plain_text: &str,
    tags_text: &str,
) -> Result<(), String> {
    conn.execute("DELETE FROM notes_fts WHERE note_id = ?", params![note_id])
        .map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO notes_fts (note_id, title, plain_text, tags_text) VALUES (?, ?, ?, ?)",
        params![note_id, title, plain_text, tags_text],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn remove_note(conn: &Connection, note_id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM notes_fts WHERE note_id = ?", params![note_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn sync_note_by_id(conn: &Connection, note_id: &str) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.title, n.plain_text,
                    COALESCE(GROUP_CONCAT(t.name, ' '), '') AS tags_text
             FROM notes n
             LEFT JOIN note_tags nt ON n.id = nt.note_id
             LEFT JOIN tags t ON nt.tag_id = t.id
             WHERE n.id = ?
             GROUP BY n.id",
        )
        .map_err(|e| e.to_string())?;

    // 区分“无该行”与真实查询错误：无行（便签已删）走删索引分支，
    // 其他错误（预留列不符/表损坏等）必须上抛，不能误当成“没这行”把索引删掉
    let note_row = match stmt.query_row(params![note_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    }) {
        Ok(row) => Some(row),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(e.to_string()),
    };

    if let Some((id, title, plain_text, tags_text)) = note_row {
        sync_note(conn, &id, &title, &plain_text, &tags_text)?;
    } else {
        remove_note(conn, note_id)?;
    }

    Ok(())
}

pub fn sync_notes_for_tag(conn: &Connection, tag_id: &str) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT note_id FROM note_tags WHERE tag_id = ?")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![tag_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;

    let note_ids: Vec<String> = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for note_id in note_ids {
        sync_note_by_id(conn, &note_id)?;
    }

    Ok(())
}

pub fn reindex_all(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM notes_fts;", [])
        .map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.title, n.plain_text,
                    COALESCE(GROUP_CONCAT(t.name, ' '), '') AS tags_text
             FROM notes n
             LEFT JOIN note_tags nt ON n.id = nt.note_id
             LEFT JOIN tags t ON nt.tag_id = t.id
             GROUP BY n.id",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    for r in rows {
        let (id, title, plain_text, tags_text) = r.map_err(|e| e.to_string())?;
        sync_note(conn, &id, &title, &plain_text, &tags_text)?;
    }

    Ok(())
}
