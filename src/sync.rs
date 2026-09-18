use crate::crypto::{self, EncryptedEnvelope};
use crate::storage::{self, AppState};
use crate::subproc::{run_cmd_bounded, run_cmd_stream_to_file};
use std::fs;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct SyncResult {
    pub success: bool,
    pub message: String,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Prepares the encrypted envelope for cloud export
pub fn export_encrypted_envelope(state: &AppState, password: &str) -> Result<PathBuf, String> {
    if password.trim().is_empty() {
        return Err("Cloud synchronization requires an E2EE encryption password".to_string());
    }

    let salt_bytes = if state.salt.is_empty() {
        crypto::generate_salt()?
    } else {
        let decoded =
            hex::decode(&state.salt).map_err(|e| format!("Invalid stored salt: {}", e))?;
        if decoded.len() != 16 {
            return Err("Invalid stored salt length".to_string());
        }
        let mut arr = [0u8; 16];
        arr.copy_from_slice(&decoded);
        arr
    };

    let key = crypto::derive_key(password, &salt_bytes)?;
    let plaintext_bytes =
        serde_json::to_vec(&state.notes).map_err(|e| format!("Serialization error: {}", e))?;

    let envelope = crypto::encrypt_payload(&plaintext_bytes, &key, &salt_bytes)?;
    let envelope_bytes = serde_json::to_vec_pretty(&envelope)
        .map_err(|e| format!("Envelope serialize error: {}", e))?;

    let enc_path = storage::get_data_dir().join("notes.enc");
    storage::atomic_write_0600(&enc_path, &envelope_bytes)?;

    Ok(enc_path)
}

/// Imports and decrypts a remote envelope into local notes
#[allow(dead_code)]
pub fn import_encrypted_envelope(
    enc_path: &Path,
    password: &str,
) -> Result<Vec<storage::Note>, String> {
    let bytes = storage::safe_read_0600(enc_path)?;
    let envelope: EncryptedEnvelope =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid envelope JSON: {}", e))?;

    let decrypted_bytes = crypto::decrypt_envelope(&envelope, password)?;

    let notes: Vec<storage::Note> = serde_json::from_slice(&decrypted_bytes)
        .map_err(|e| format!("Decrypted payload is not valid notes JSON: {}", e))?;

    Ok(notes)
}

/// Synchronizes encrypted notes to/from configured cloud provider
pub fn sync_cloud(state: &mut AppState, password: &str) -> SyncResult {
    let provider = state.cloud.provider.to_lowercase();
    if provider == "none" || provider.is_empty() {
        return SyncResult {
            success: true,
            message: "Local only (cloud sync disabled)".to_string(),
        };
    }

    let enc_file = match export_encrypted_envelope(state, password) {
        Ok(p) => p,
        Err(e) => {
            state.cloud.last_sync_status = "error".to_string();
            state.cloud.last_sync_msg = e.clone();
            let _ = storage::save_app_state(state);
            return SyncResult {
                success: false,
                message: e,
            };
        }
    };

    let enc_file_str = match enc_file.to_str() {
        Some(s) => s,
        None => {
            return SyncResult {
                success: false,
                message: "Invalid path UTF-8".to_string(),
            }
        }
    };

    match provider.as_str() {
        "rclone" => {
            if state.cloud.rclone_remote.trim().is_empty() {
                return SyncResult {
                    success: false,
                    message: "Rclone remote path is not configured (e.g., gdrive:OmarchyNotes)"
                        .to_string(),
                };
            }

            let remote_dest = format!(
                "{}/notes.enc",
                state.cloud.rclone_remote.trim_end_matches('/')
            );
            let deadline = Instant::now() + Duration::from_secs(30);

            // rclone copyto <local> <remote>
            let args = ["copyto", enc_file_str, &remote_dest];
            let out = run_cmd_bounded("/usr/bin/rclone", &args, &[], deadline, 65536);

            if out.is_some() {
                state.cloud.last_synced_at = now_secs();
                state.cloud.last_sync_status = "synced".to_string();
                state.cloud.last_sync_msg =
                    format!("Encrypted notes synced to {}", state.cloud.rclone_remote);
                let _ = storage::save_app_state(state);
                SyncResult {
                    success: true,
                    message: format!(
                        "Successfully synced encrypted notes to {}",
                        state.cloud.rclone_remote
                    ),
                }
            } else {
                state.cloud.last_sync_status = "error".to_string();
                state.cloud.last_sync_msg = "Rclone command failed or timed out".to_string();
                let _ = storage::save_app_state(state);
                SyncResult {
                    success: false,
                    message: "Failed to upload via rclone: verify remote configuration".to_string(),
                }
            }
        }
        "git" => {
            if state.cloud.git_remote.trim().is_empty() {
                return SyncResult {
                    success: false,
                    message: "Git repository path or remote is not configured".to_string(),
                };
            }

            let repo_path = PathBuf::from(state.cloud.git_remote.trim());
            if !repo_path.exists() {
                return SyncResult {
                    success: false,
                    message: format!("Git repo path does not exist: {:?}", repo_path),
                };
            }

            let repo_str = match repo_path.to_str() {
                Some(s) => s,
                None => {
                    return SyncResult {
                        success: false,
                        message: "Invalid git path UTF-8".to_string(),
                    }
                }
            };

            let target_file = repo_path.join("notes.enc");
            if let Err(e) = fs::copy(&enc_file, &target_file) {
                return SyncResult {
                    success: false,
                    message: format!("Failed to copy notes.enc to git directory: {}", e),
                };
            }

            let deadline = Instant::now() + Duration::from_secs(20);

            // git -C <repo> add -- notes.enc
            let add_args = ["-C", repo_str, "add", "--", "notes.enc"];
            let _ = run_cmd_bounded(
                "/usr/bin/git",
                &add_args,
                &[("GIT_TERMINAL_PROMPT", "0")],
                deadline,
                32768,
            );

            // Check if there are changes to commit
            let status_args = [
                "-C",
                repo_str,
                "status",
                "--porcelain=v1",
                "--",
                "notes.enc",
            ];
            let status_out = run_cmd_bounded(
                "/usr/bin/git",
                &status_args,
                &[("GIT_TERMINAL_PROMPT", "0")],
                deadline,
                4096,
            );
            let has_changes = status_out.map(|b| !b.is_empty()).unwrap_or(false);

            if has_changes {
                // git -C <repo> commit -m "sync: e2ee encrypted notes update" -- notes.enc
                let commit_args = [
                    "-C",
                    repo_str,
                    "commit",
                    "-m",
                    "sync: e2ee encrypted notes update",
                    "--",
                    "notes.enc",
                ];
                let _ = run_cmd_bounded(
                    "/usr/bin/git",
                    &commit_args,
                    &[("GIT_TERMINAL_PROMPT", "0")],
                    deadline,
                    32768,
                );
            }

            // Check if remote exists
            let remote_args = ["-C", repo_str, "remote"];
            let remote_out = run_cmd_bounded(
                "/usr/bin/git",
                &remote_args,
                &[("GIT_TERMINAL_PROMPT", "0")],
                deadline,
                4096,
            );
            let has_remote = remote_out
                .map(|b| !b.trim_ascii().is_empty())
                .unwrap_or(false);

            if has_remote {
                // git -C <repo> push -- origin HEAD
                let push_args = ["-C", repo_str, "push", "--", "origin", "HEAD"];
                let push_out = run_cmd_bounded(
                    "/usr/bin/git",
                    &push_args,
                    &[("GIT_TERMINAL_PROMPT", "0")],
                    deadline,
                    32768,
                );
                if push_out.is_none() {
                    state.cloud.last_sync_status = "error".to_string();
                    state.cloud.last_sync_msg = "Git push failed".to_string();
                    let _ = storage::save_app_state(state);
                    return SyncResult {
                        success: false,
                        message: "Git push to remote failed. Verify network or SSH authentication."
                            .to_string(),
                    };
                }
            }

            state.cloud.last_synced_at = now_secs();
            state.cloud.last_sync_status = "synced".to_string();
            state.cloud.last_sync_msg = if has_remote {
                "Pushed encrypted notes to Git"
            } else {
                "Saved encrypted notes to Git vault"
            }
            .to_string();
            let _ = storage::save_app_state(state);
            SyncResult {
                success: true,
                message: if has_remote {
                    "Pushed encrypted notes to Git remote"
                } else {
                    "Committed encrypted notes to local Git vault"
                }
                .to_string(),
            }
        }
        _ => SyncResult {
            success: false,
            message: format!("Unknown cloud provider: {}", provider),
        },
    }
}

/// Maximum allowed ciphertext size for cloud synchronization (10 MiB)
pub const MAX_CIPHERTEXT_SIZE: u64 = 10 * 1024 * 1024;

struct StagingCleanupGuard {
    path: PathBuf,
    active: bool,
}

impl Drop for StagingCleanupGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Pulls and decrypts remote notes into local state
pub fn pull_cloud(state: &mut AppState, password: &str) -> SyncResult {
    let provider = state.cloud.provider.to_lowercase();
    let data_dir = storage::get_data_dir();
    if let Err(e) = storage::ensure_dir_0700(&data_dir) {
        return SyncResult {
            success: false,
            message: format!("Failed to ensure data directory: {}", e),
        };
    }

    let local_enc = data_dir.join("notes.enc");
    let staging_enc = data_dir.join(format!(
        ".tmp_cloud_restore_{}_{}",
        std::process::id(),
        storage::fastrand()
    ));

    let mut staging_guard = StagingCleanupGuard {
        path: staging_enc.clone(),
        active: true,
    };

    let deadline = Instant::now() + Duration::from_secs(30);

    match provider.as_str() {
        "rclone" => {
            if state.cloud.rclone_remote.trim().is_empty() {
                return SyncResult {
                    success: false,
                    message: "Rclone remote not configured".to_string(),
                };
            }
            let remote_src = format!(
                "{}/notes.enc",
                state.cloud.rclone_remote.trim_end_matches('/')
            );

            // Exclusively create the private staging file with mode 0600
            let mut staging_file = match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&staging_enc)
            {
                Ok(f) => f,
                Err(e) => {
                    return SyncResult {
                        success: false,
                        message: format!("Failed to create private staging file: {}", e),
                    };
                }
            };

            // Stream remote object through parent-controlled pipe with strict byte counting
            // Terminate rclone process group at MAX_CIPHERTEXT_SIZE + 1
            // Keep --max-size 10M as an early rejection metadata filter optimization
            let args = [
                "cat",
                "--max-size",
                "10M",
                "--",
                &remote_src,
            ];
            if let Err(e) = run_cmd_stream_to_file(
                "/usr/bin/rclone",
                &args,
                &[],
                &mut staging_file,
                deadline,
                MAX_CIPHERTEXT_SIZE as usize,
            ) {
                return SyncResult {
                    success: false,
                    message: format!("Failed to stream notes.enc from rclone remote: {}", e),
                };
            }
        }
        "git" => {
            let repo_path = PathBuf::from(state.cloud.git_remote.trim());
            if !repo_path.exists() {
                return SyncResult {
                    success: false,
                    message: "Git repository path does not exist".to_string(),
                };
            }
            let repo_str = match repo_path.to_str() {
                Some(s) => s,
                None => {
                    return SyncResult {
                        success: false,
                        message: "Invalid git path".to_string(),
                    }
                }
            };

            // Check if remote exists, git pull
            let remote_args = ["-C", repo_str, "remote"];
            let has_remote = run_cmd_bounded(
                "/usr/bin/git",
                &remote_args,
                &[("GIT_TERMINAL_PROMPT", "0")],
                deadline,
                4096,
            )
            .map(|b| !b.trim_ascii().is_empty())
            .unwrap_or(false);

            if has_remote {
                let pull_args = ["-C", repo_str, "pull", "--", "origin", "HEAD"];
                let _ = run_cmd_bounded(
                    "/usr/bin/git",
                    &pull_args,
                    &[("GIT_TERMINAL_PROMPT", "0")],
                    deadline,
                    32768,
                );
            }

            let remote_enc = repo_path.join("notes.enc");
            if !remote_enc.exists() {
                return SyncResult {
                    success: false,
                    message: "notes.enc does not exist in Git repository".to_string(),
                };
            }

            // Copy bounded bytes from git repo to private staging
            let bytes = match storage::safe_read_bounded_0600(&remote_enc, MAX_CIPHERTEXT_SIZE) {
                Ok(b) => b,
                Err(e) => {
                    return SyncResult {
                        success: false,
                        message: format!("Git vault notes.enc rejected: {}", e),
                    };
                }
            };

            if let Err(e) = storage::atomic_write_0600(&staging_enc, &bytes) {
                return SyncResult {
                    success: false,
                    message: format!("Failed to stage git notes: {}", e),
                };
            }
        }
        _ => {
            return SyncResult {
                success: false,
                message: "No cloud provider configured for pull".to_string(),
            };
        }
    }

    // Ensure staging file has strict 0600 permissions
    let _ = fs::set_permissions(&staging_enc, fs::Permissions::from_mode(0o600));

    // Verify bounded staging file before atomically publishing as notes.enc
    let notes = match import_encrypted_envelope(&staging_enc, password) {
        Ok(notes) => notes,
        Err(e) => {
            state.cloud.last_sync_status = "error".to_string();
            state.cloud.last_sync_msg = format!("Decryption error on pull: {}", e);
            let _ = storage::save_app_state(state);
            return SyncResult {
                success: false,
                message: format!("Decryption error: {}", e),
            };
        }
    };

    // Atomically publish validated staging file to local notes.enc
    if let Err(e) = fs::rename(&staging_enc, &local_enc) {
        return SyncResult {
            success: false,
            message: format!("Failed to atomically publish notes.enc: {}", e),
        };
    }

    // Defuse staging cleanup guard now that atomic publication succeeded
    staging_guard.active = false;

    state.notes = notes;
    state.cloud.last_synced_at = now_secs();
    state.cloud.last_sync_status = "synced".to_string();
    state.cloud.last_sync_msg =
        "Successfully pulled and decrypted notes from cloud".to_string();
    let _ = storage::save_app_state(state);
    SyncResult {
        success: true,
        message: "Successfully pulled and decrypted notes from cloud".to_string(),
    }
}
