use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

extern "C" {
    fn getuid() -> u32;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChecklistItem {
    pub id: String,
    pub text: String,
    pub checked: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub is_checklist: bool,
    pub checklist_items: Vec<ChecklistItem>,
    pub color: String, // "yellow", "green", "blue", "red", "purple", "teal", "default"
    pub pinned: bool,
    pub archived: bool,
    pub tags: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CloudConfig {
    pub provider: String, // "none", "rclone", "git"
    pub rclone_remote: String, // e.g. "gdrive:OmarchyNotes" or "nextcloud:Notes"
    pub git_remote: String,
    pub auto_sync: bool,
    pub last_synced_at: u64,
    pub last_sync_status: String, // "idle", "synced", "syncing", "error"
    pub last_sync_msg: String,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            provider: "none".to_string(),
            rclone_remote: "".to_string(),
            git_remote: "".to_string(),
            auto_sync: false,
            last_synced_at: 0,
            last_sync_status: "idle".to_string(),
            last_sync_msg: "Local only".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppState {
    pub version: u32,
    pub e2ee_enabled: bool,
    pub salt: String,
    pub cloud: CloudConfig,
    pub notes: Vec<Note>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            version: 1,
            e2ee_enabled: false,
            salt: "".to_string(),
            cloud: CloudConfig::default(),
            notes: Vec::new(),
        }
    }
}

pub fn get_data_dir() -> PathBuf {
    if let Ok(path) = std::env::var("OMANOTES_DATA_DIR") {
        return PathBuf::from(path);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".local/share/omanotes")
}

#[allow(dead_code)]
pub fn get_state_dir() -> PathBuf {
    if let Ok(path) = std::env::var("OMANOTES_STATE_DIR") {
        return PathBuf::from(path);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".local/state/omanotes")
}

pub fn ensure_dir_0700(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create directory {:?}: {}", dir, e))?;
    }
    let meta = fs::metadata(dir).map_err(|e| format!("Failed to get directory metadata: {}", e))?;
    // SAFETY: getuid is a standard POSIX libc call
    let current_uid = unsafe { getuid() };
    if meta.uid() != current_uid {
        return Err(format!("Directory ownership mismatch for {:?}", dir));
    }
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    Ok(())
}

/// Atomically writes content to a file with strict 0600 permissions,
/// verified non-symlink, owner matching, and fsync
pub fn atomic_write_0600(file_path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = file_path.parent().ok_or("Invalid file path parent")?;
    ensure_dir_0700(parent)?;

    // If file exists, verify it is not a symlink, owned by user, and a regular file
    if file_path.exists() {
        let meta = fs::symlink_metadata(file_path).map_err(|e| format!("symlink_metadata failed: {}", e))?;
        if meta.file_type().is_symlink() {
            return Err(format!("Security violation: {:?} is a symlink", file_path));
        }
        if !meta.file_type().is_file() {
            return Err(format!("Security violation: {:?} is not a regular file", file_path));
        }
        // SAFETY: getuid is a standard POSIX call
        let current_uid = unsafe { getuid() };
        if meta.uid() != current_uid {
            return Err(format!("Security violation: UID mismatch on {:?}", file_path));
        }
    }

    let tmp_file_path = parent.join(format!(".tmp_{}_{}", std::process::id(), fastrand()));

    {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp_file_path)
            .map_err(|e| format!("Failed to open temp file {:?}: {}", tmp_file_path, e))?;

        // Explicit set_permissions to be independent of umask
        f.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("Failed to set permissions 0600: {}", e))?;

        f.write_all(content)
            .map_err(|e| format!("Failed to write to temp file: {}", e))?;

        f.sync_all()
            .map_err(|e| format!("Failed to sync temp file to disk: {}", e))?;
    }

    fs::rename(&tmp_file_path, file_path)
        .map_err(|e| format!("Atomic rename failed from {:?} to {:?}: {}", tmp_file_path, file_path, e))?;

    Ok(())
}

/// Safely reads file with strict mode 0600, UID match, and symlink rejection
pub fn safe_read_0600(file_path: &Path) -> Result<Vec<u8>, String> {
    if !file_path.exists() {
        return Err(format!("File does not exist: {:?}", file_path));
    }

    let meta = fs::symlink_metadata(file_path).map_err(|e| format!("symlink_metadata failed: {}", e))?;
    if meta.file_type().is_symlink() {
        return Err(format!("Security violation: {:?} is a symlink", file_path));
    }
    if !meta.file_type().is_file() {
        return Err(format!("Security violation: {:?} is not a regular file", file_path));
    }

    // SAFETY: getuid is a standard POSIX call
    let current_uid = unsafe { getuid() };
    if meta.uid() != current_uid {
        return Err(format!("Security violation: UID mismatch on {:?}", file_path));
    }

    let mode = meta.mode() & 0o777;
    if mode != 0o600 && mode != 0o400 {
        // Enforce 0600 if it was lax
        let _ = fs::set_permissions(file_path, fs::Permissions::from_mode(0o600));
    }

    let mut f = fs::File::open(file_path).map_err(|e| format!("Failed to open file {:?}: {}", file_path, e))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| format!("Failed to read file: {}", e))?;
    Ok(buf)
}

fn fastrand() -> u64 {
    let mut bytes = [0u8; 8];
    let _ = getrandom::getrandom(&mut bytes);
    u64::from_le_bytes(bytes)
}

pub fn load_app_state() -> AppState {
    let path = get_data_dir().join("notes.json");
    if !path.exists() {
        return AppState::default();
    }
    match safe_read_0600(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => AppState::default(),
    }
}

pub fn save_app_state(state: &AppState) -> Result<(), String> {
    let data_dir = get_data_dir();
    ensure_dir_0700(&data_dir)?;
    let path = data_dir.join("notes.json");
    let json_bytes = serde_json::to_vec_pretty(state).map_err(|e| format!("JSON serialize error: {}", e))?;
    atomic_write_0600(&path, &json_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_write_and_safe_read_0600() {
        let dir = PathBuf::from("/tmp/test_omanotes_storage");
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("test_note.json");

        let content = b"{\"hello\": \"world\"}";
        atomic_write_0600(&file, content).expect("atomic_write_0600");

        let read_back = safe_read_0600(&file).expect("safe_read_0600");
        assert_eq!(content, read_back.as_slice());

        let meta = fs::metadata(&file).expect("metadata");
        assert_eq!(meta.mode() & 0o777, 0o600);

        let _ = fs::remove_dir_all(&dir);
    }
}
