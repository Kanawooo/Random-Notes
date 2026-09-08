use std::path::PathBuf;
use std::sync::OnceLock;

static USER_DATA_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

pub fn set_user_data_override(path: PathBuf) {
    let _ = USER_DATA_OVERRIDE.set(path);
}

pub fn get_user_data_dir() -> PathBuf {
    if let Some(path) = USER_DATA_OVERRIDE.get() {
        return path.clone();
    }

    // Default to %APPDATA%\com.suijian.notes for Windows / Electron compatibility
    if let Some(app_data) = std::env::var_os("APPDATA") {
        let path = PathBuf::from(app_data).join("com.suijian.notes");
        return path;
    }

    // Fallback using directories crate
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "suijian", "notes") {
        return proj_dirs.data_dir().to_path_buf();
    }

    PathBuf::from(".suijian")
}

pub fn get_db_path() -> PathBuf {
    get_user_data_dir().join("suijian.db")
}

pub fn get_attachments_dir() -> PathBuf {
    get_user_data_dir().join("attachments")
}

pub fn get_backups_dir() -> PathBuf {
    get_user_data_dir().join("backups")
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(get_user_data_dir())?;
    std::fs::create_dir_all(get_attachments_dir())?;
    std::fs::create_dir_all(get_backups_dir())?;
    Ok(())
}

pub fn validate_uuid(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok()
}
