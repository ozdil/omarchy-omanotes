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
    pub provider: String,      // "none", "rclone", "git"
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
        fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create directory {:?}: {}", dir, e))?;
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
        let meta = fs::symlink_metadata(file_path)
            .map_err(|e| format!("symlink_metadata failed: {}", e))?;
        if meta.file_type().is_symlink() {
            return Err(format!("Security violation: {:?} is a symlink", file_path));
        }
        if !meta.file_type().is_file() {
            return Err(format!(
                "Security violation: {:?} is not a regular file",
                file_path
            ));
        }
        // SAFETY: getuid is a standard POSIX call
        let current_uid = unsafe { getuid() };
        if meta.uid() != current_uid {
            return Err(format!(
                "Security violation: UID mismatch on {:?}",
                file_path
            ));
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

    fs::rename(&tmp_file_path, file_path).map_err(|e| {
        format!(
            "Atomic rename failed from {:?} to {:?}: {}",
            tmp_file_path, file_path, e
        )
    })?;

    Ok(())
}

/// Maximum allowed file read ceiling to protect against unbounded memory allocation (10 MiB)
pub const MAX_STORAGE_READ_SIZE: u64 = 10 * 1024 * 1024;
const O_NOFOLLOW: i32 = 0o400000;

/// Safely reads file with strict mode 0600, UID match, and symlink rejection up to MAX_STORAGE_READ_SIZE
pub fn safe_read_0600(file_path: &Path) -> Result<Vec<u8>, String> {
    safe_read_bounded_0600(file_path, MAX_STORAGE_READ_SIZE)
}

/// Safely reads file with strict mode 0600, UID match, descriptor-bound fstat checks,
/// and take(max_bytes + 1) bounded reading with explicit oversize rejection.
pub fn safe_read_bounded_0600(file_path: &Path, max_bytes: u64) -> Result<Vec<u8>, String> {
    if !file_path.exists() {
        return Err(format!("File does not exist: {:?}", file_path));
    }

    let meta =
        fs::symlink_metadata(file_path).map_err(|e| format!("symlink_metadata failed: {}", e))?;
    if meta.file_type().is_symlink() {
        return Err(format!("Security violation: {:?} is a symlink", file_path));
    }
    if !meta.file_type().is_file() {
        return Err(format!(
            "Security violation: {:?} is not a regular file",
            file_path
        ));
    }
    if meta.len() > max_bytes {
        return Err(format!(
            "Security violation: {:?} exceeds maximum permitted size of {} bytes (got {} bytes)",
            file_path, max_bytes, meta.len()
        ));
    }

    // SAFETY: getuid is a standard POSIX call
    let current_uid = unsafe { getuid() };
    if meta.uid() != current_uid {
        return Err(format!(
            "Security violation: UID mismatch on {:?}",
            file_path
        ));
    }

    let mode = meta.mode() & 0o777;
    if mode != 0o600 && mode != 0o400 {
        // Enforce 0600 if it was lax
        let _ = fs::set_permissions(file_path, fs::Permissions::from_mode(0o600));
    }

    let mut opts = OpenOptions::new();
    opts.read(true).custom_flags(O_NOFOLLOW);

    let f = opts
        .open(file_path)
        .map_err(|e| format!("Failed to open file {:?}: {}", file_path, e))?;

    let fd_meta = f
        .metadata()
        .map_err(|e| format!("fstat failed on {:?}: {}", file_path, e))?;
    if !fd_meta.file_type().is_file() || fd_meta.file_type().is_symlink() {
        return Err(format!(
            "Security violation: descriptor for {:?} is not a regular file",
            file_path
        ));
    }
    if fd_meta.uid() != current_uid {
        return Err(format!(
            "Security violation: UID mismatch on descriptor for {:?}",
            file_path
        ));
    }
    if fd_meta.len() > max_bytes {
        return Err(format!(
            "Security violation: descriptor length {} exceeds max permitted limit {}",
            fd_meta.len(),
            max_bytes
        ));
    }

    let mut buf = Vec::new();
    f.take(max_bytes.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    if buf.len() as u64 > max_bytes {
        return Err(format!(
            "Security violation: file {:?} read buffer exceeded limit of {} bytes",
            file_path, max_bytes
        ));
    }

    Ok(buf)
}

pub fn fastrand() -> u64 {
    let mut bytes = [0u8; 8];
    let _ = getrandom::getrandom(&mut bytes);
    u64::from_le_bytes(bytes)
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ConfigState {
    pub version: u32,
    pub e2ee_enabled: bool,
    pub salt: String,
    pub cloud: CloudConfig,
}

pub fn load_app_state() -> AppState {
    let mut state = AppState::default();

    // Load config from state dir if exists
    let config_path = get_state_dir().join("config.json");
    if config_path.exists() {
        if let Ok(bytes) = safe_read_0600(&config_path) {
            if let Ok(cfg) = serde_json::from_slice::<ConfigState>(&bytes) {
                state.version = cfg.version;
                state.e2ee_enabled = cfg.e2ee_enabled;
                state.salt = cfg.salt;
                state.cloud = cfg.cloud;
            }
        }
    }

    // Load notes from data dir
    let notes_path = get_data_dir().join("notes.json");
    if notes_path.exists() {
        if let Ok(bytes) = safe_read_0600(&notes_path) {
            // Check if legacy full AppState
            if let Ok(full_state) = serde_json::from_slice::<AppState>(&bytes) {
                state.notes = full_state.notes;
                if !config_path.exists() {
                    state.version = full_state.version;
                    state.e2ee_enabled = full_state.e2ee_enabled;
                    state.salt = full_state.salt;
                    state.cloud = full_state.cloud;
                }
            } else if let Ok(notes_vec) = serde_json::from_slice::<Vec<Note>>(&bytes) {
                state.notes = notes_vec;
            }
        }
    }

    state
}

pub fn save_app_state(state: &AppState) -> Result<(), String> {
    let state_dir = get_state_dir();
    ensure_dir_0700(&state_dir)?;
    let config = ConfigState {
        version: state.version,
        e2ee_enabled: state.e2ee_enabled,
        salt: state.salt.clone(),
        cloud: state.cloud.clone(),
    };
    let cfg_bytes =
        serde_json::to_vec_pretty(&config).map_err(|e| format!("Config serialize error: {}", e))?;
    atomic_write_0600(&state_dir.join("config.json"), &cfg_bytes)?;

    let data_dir = get_data_dir();
    ensure_dir_0700(&data_dir)?;
    let notes_bytes = serde_json::to_vec_pretty(&state.notes)
        .map_err(|e| format!("Notes serialize error: {}", e))?;
    atomic_write_0600(&data_dir.join("notes.json"), &notes_bytes)?;

    Ok(())
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

    #[test]
    fn test_oversized_file_rejected_by_bounded_read() {
        let dir = PathBuf::from("/tmp/test_omanotes_storage_oversize");
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("oversized.bin");

        let content = vec![0x41u8; 1024]; // 1 KiB
        atomic_write_0600(&file, &content).expect("atomic_write");

        // Reading with 512 bytes limit must fail with oversize error
        let res = safe_read_bounded_0600(&file, 512);
        assert!(res.is_err());
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("exceeds maximum permitted size"));

        // Reading with 2048 bytes limit must succeed
        let ok_res = safe_read_bounded_0600(&file, 2048);
        assert!(ok_res.is_ok());
        assert_eq!(ok_res.unwrap().len(), 1024);

        let _ = fs::remove_dir_all(&dir);
    }
}
