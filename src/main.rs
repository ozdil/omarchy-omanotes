mod crypto;
mod storage;
mod subproc;
mod sync;

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};
use storage::{AppState, ChecklistItem, Note};

use std::io::{BufRead, Read};

const MAX_PAYLOAD_BYTES: u64 = 262_144; // 256 KiB strict cap to prevent memory exhaustion / DoS

#[derive(serde::Deserialize)]
struct AddPayload {
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
    #[serde(default = "default_color")]
    color: String,
    #[serde(default)]
    is_checklist: bool,
    #[serde(default)]
    checklist_items: Option<Vec<String>>,
    #[serde(default)]
    tags: Vec<String>,
}

fn default_color() -> String {
    "yellow".to_string()
}

#[derive(serde::Deserialize)]
struct EditPayload {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
}

#[derive(serde::Deserialize)]
struct AddCheckItemPayload {
    note_id: String,
    text: String,
}

#[derive(serde::Deserialize)]
struct CloudPayload {
    #[serde(default)]
    provider: String,
    #[serde(default)]
    rclone_remote: String,
    #[serde(default)]
    git_remote: String,
    #[serde(default)]
    auto_sync: bool,
}

#[derive(serde::Deserialize)]
struct PasswordPayload {
    #[serde(default)]
    password: String,
}

fn read_framed_stdin<T: serde::de::DeserializeOwned>() -> Result<T, String> {
    let stdin = std::io::stdin();
    let mut handle = stdin.lock().take(MAX_PAYLOAD_BYTES);
    let mut line = String::new();
    handle
        .read_line(&mut line)
        .map_err(|_| "Failed to read payload from stdin".to_string())?;

    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err("Payload on stdin is empty".to_string());
    }
    serde_json::from_str::<T>(trimmed).map_err(|e| format!("Invalid JSON framing: {}", e))
}

fn read_password_stdin() -> Result<String, String> {
    let stdin = std::io::stdin();
    let mut handle = stdin.lock().take(MAX_PAYLOAD_BYTES);
    let mut line = String::new();
    handle
        .read_line(&mut line)
        .map_err(|_| "Failed to read password from stdin".to_string())?;

    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if let Ok(val) = serde_json::from_str::<PasswordPayload>(trimmed) {
        return Ok(val.password);
    }
    Ok(trimmed.to_string())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn generate_id() -> String {
    let mut bytes = [0u8; 8];
    let _ = getrandom::getrandom(&mut bytes);
    format!("{:x}", u64::from_le_bytes(bytes))
}

fn print_json_state(state: &AppState) {
    let pinned_count = state
        .notes
        .iter()
        .filter(|n| n.pinned && !n.archived)
        .count();
    let active_count = state.notes.iter().filter(|n| !n.archived).count();
    let archived_count = state.notes.iter().filter(|n| n.archived).count();

    let output = serde_json::json!({
        "success": true,
        "version": state.version,
        "e2ee_enabled": state.e2ee_enabled,
        "has_salt": !state.salt.is_empty(),
        "total_notes": state.notes.len(),
        "active_notes": active_count,
        "pinned_notes": pinned_count,
        "archived_notes": archived_count,
        "cloud": state.cloud,
        "notes": state.notes,
    });

    println!("{}", output);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut state = storage::load_app_state();

    // If state is completely empty, insert a helpful welcome note
    if state.notes.is_empty() {
        let welcome_note = Note {
            id: generate_id(),
            title: "Welcome to OmaNotes!".to_string(),
            content: "Machine-age Google Keep clone for Omarchy Linux with zero-latency Quickshell UI and Zero-Knowledge AES-256-GCM encryption.\n\n• Group notes using pastel color palettes\n• Pin priority cards to the top\n• Back up encrypted notes to Google Drive or Git in Settings".to_string(),
            is_checklist: false,
            checklist_items: Vec::new(),
            color: "yellow".to_string(),
            pinned: true,
            archived: false,
            tags: vec!["omarchy".to_string(), "keep".to_string()],
            created_at: now_secs(),
            updated_at: now_secs(),
        };

        let todo_note = Note {
            id: generate_id(),
            title: "Daily Tasks & Checklist".to_string(),
            content: "".to_string(),
            is_checklist: true,
            checklist_items: vec![
                ChecklistItem {
                    id: generate_id(),
                    text: "Inspect the Quickshell desktop UI".to_string(),
                    checked: true,
                },
                ChecklistItem {
                    id: generate_id(),
                    text: "Create a color-coded sticky note".to_string(),
                    checked: false,
                },
                ChecklistItem {
                    id: generate_id(),
                    text: "Set up E2EE master encryption password".to_string(),
                    checked: false,
                },
            ],
            color: "green".to_string(),
            pinned: false,
            archived: false,
            tags: vec!["todo".to_string()],
            created_at: now_secs(),
            updated_at: now_secs(),
        };

        state.notes.push(welcome_note);
        state.notes.push(todo_note);
        let _ = storage::save_app_state(&state);
    }

    if args.len() < 2 || args[1] == "--json" || args[1] == "--status" {
        print_json_state(&state);
        return;
    }

    match args[1].as_str() {
        "--add-clipboard" => {
            let color = if args.len() > 2 { &args[2] } else { "yellow" };
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
            if let Some(bytes) = subproc::run_cmd_bounded(
                "/usr/bin/wl-paste",
                &["--no-newline"],
                &[],
                deadline,
                65536,
            ) {
                if let Ok(text) = String::from_utf8(bytes) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        let first_line = trimmed.lines().next().unwrap_or("Clipboard Note");
                        let title = if first_line.len() > 40 {
                            format!("{}...", &first_line[..40])
                        } else {
                            first_line.to_string()
                        };
                        let is_checklist = trimmed.contains('\n');
                        let mut items = Vec::new();
                        if is_checklist {
                            for line in trimmed.lines() {
                                let l = line.trim();
                                if !l.is_empty() {
                                    items.push(ChecklistItem {
                                        id: generate_id(),
                                        text: l.to_string(),
                                        checked: false,
                                    });
                                }
                            }
                        }

                        let new_note = Note {
                            id: generate_id(),
                            title,
                            content: if is_checklist {
                                "".to_string()
                            } else {
                                trimmed.to_string()
                            },
                            is_checklist,
                            checklist_items: items,
                            color: color.to_string(),
                            pinned: false,
                            archived: false,
                            tags: vec!["clipboard".to_string()],
                            created_at: now_secs(),
                            updated_at: now_secs(),
                        };
                        state.notes.insert(0, new_note);
                        let _ = storage::save_app_state(&state);
                    }
                }
            }
            print_json_state(&state);
        }
        "--add-template" => {
            if args.len() < 3 {
                eprintln!("Usage: omanotes-engine --add-template <shopping|daily|idea|reminder>");
                std::process::exit(1);
            }
            let tmpl = &args[2];
            let new_note = match tmpl.as_str() {
                "shopping" => Note {
                    id: generate_id(),
                    title: "Grocery Shopping List".to_string(),
                    content: "".to_string(),
                    is_checklist: true,
                    checklist_items: vec![
                        ChecklistItem {
                            id: generate_id(),
                            text: "Fresh Bread".to_string(),
                            checked: false,
                        },
                        ChecklistItem {
                            id: generate_id(),
                            text: "Whole Milk".to_string(),
                            checked: false,
                        },
                        ChecklistItem {
                            id: generate_id(),
                            text: "Organic Eggs".to_string(),
                            checked: false,
                        },
                        ChecklistItem {
                            id: generate_id(),
                            text: "Dark Roast Coffee".to_string(),
                            checked: false,
                        },
                    ],
                    color: "green".to_string(),
                    pinned: true,
                    archived: false,
                    tags: vec!["groceries".to_string()],
                    created_at: now_secs(),
                    updated_at: now_secs(),
                },
                "daily" => Note {
                    id: generate_id(),
                    title: "Daily Priorities".to_string(),
                    content: "".to_string(),
                    is_checklist: true,
                    checklist_items: vec![
                        ChecklistItem {
                            id: generate_id(),
                            text: "Morning inbox & system triage".to_string(),
                            checked: false,
                        },
                        ChecklistItem {
                            id: generate_id(),
                            text: "Core feature engineering".to_string(),
                            checked: false,
                        },
                        ChecklistItem {
                            id: generate_id(),
                            text: "Sync notes vault & push commits".to_string(),
                            checked: false,
                        },
                    ],
                    color: "blue".to_string(),
                    pinned: false,
                    archived: false,
                    tags: vec!["priorities".to_string()],
                    created_at: now_secs(),
                    updated_at: now_secs(),
                },
                "idea" => Note {
                    id: generate_id(),
                    title: "New Project Idea".to_string(),
                    content: "Architecture notes, brainstorming, and design draft...".to_string(),
                    is_checklist: false,
                    checklist_items: Vec::new(),
                    color: "yellow".to_string(),
                    pinned: false,
                    archived: false,
                    tags: vec!["idea".to_string()],
                    created_at: now_secs(),
                    updated_at: now_secs(),
                },
                "reminder" => Note {
                    id: generate_id(),
                    title: "High Priority Reminder".to_string(),
                    content: "Critical action item requiring prompt follow-up!".to_string(),
                    is_checklist: false,
                    checklist_items: Vec::new(),
                    color: "red".to_string(),
                    pinned: true,
                    archived: false,
                    tags: vec!["urgent".to_string()],
                    created_at: now_secs(),
                    updated_at: now_secs(),
                },
                _ => Note {
                    id: generate_id(),
                    title: "New Note".to_string(),
                    content: "".to_string(),
                    is_checklist: false,
                    checklist_items: Vec::new(),
                    color: "teal".to_string(),
                    pinned: false,
                    archived: false,
                    tags: Vec::new(),
                    created_at: now_secs(),
                    updated_at: now_secs(),
                },
            };
            state.notes.insert(0, new_note);
            let _ = storage::save_app_state(&state);
            print_json_state(&state);
        }
        "--duplicate" => {
            if args.len() < 3 {
                eprintln!("Usage: omanotes-engine --duplicate <id>");
                std::process::exit(1);
            }
            let id = &args[2];
            if let Some(note) = state.notes.iter().find(|n| &n.id == id).cloned() {
                let mut dup = note;
                dup.id = generate_id();
                dup.title = format!("{} (Copy)", dup.title);
                dup.created_at = now_secs();
                dup.updated_at = now_secs();
                state.notes.insert(0, dup);
                let _ = storage::save_app_state(&state);
            }
            print_json_state(&state);
        }
        "--clear-completed" => {
            if args.len() < 3 {
                eprintln!("Usage: omanotes-engine --clear-completed <id>");
                std::process::exit(1);
            }
            let id = &args[2];
            if let Some(note) = state.notes.iter_mut().find(|n| &n.id == id) {
                note.checklist_items.retain(|item| !item.checked);
                note.updated_at = now_secs();
                let _ = storage::save_app_state(&state);
            }
            print_json_state(&state);
        }
        "--add" => {
            if args.len() > 2 {
                eprintln!("Security Error: Note content is prohibited in process arguments to prevent /proc/<pid>/cmdline leaks. Pass framed JSON payload via stdin.");
                std::process::exit(1);
            }
            let payload: AddPayload = match read_framed_stdin() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Payload error: {}", e);
                    std::process::exit(1);
                }
            };

            let title = if payload.title.trim().is_empty() {
                "New Note".to_string()
            } else {
                payload.title.trim().to_string()
            };

            let mut items = Vec::new();
            if payload.is_checklist {
                if let Some(explicit_items) = payload.checklist_items {
                    for it in explicit_items {
                        let trimmed = it.trim();
                        if !trimmed.is_empty() {
                            items.push(ChecklistItem {
                                id: generate_id(),
                                text: trimmed.to_string(),
                                checked: false,
                            });
                        }
                    }
                } else {
                    for line in payload.content.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            items.push(ChecklistItem {
                                id: generate_id(),
                                text: trimmed.to_string(),
                                checked: false,
                            });
                        }
                    }
                }
            }

            let new_note = Note {
                id: generate_id(),
                title,
                content: payload.content,
                is_checklist: payload.is_checklist,
                checklist_items: items,
                color: payload.color,
                pinned: false,
                archived: false,
                tags: payload.tags,
                created_at: now_secs(),
                updated_at: now_secs(),
            };

            state.notes.insert(0, new_note);
            let _ = storage::save_app_state(&state);
            print_json_state(&state);
        }
        "--delete" => {
            if args.len() < 3 {
                eprintln!("Usage: omanotes-engine --delete <id>");
                std::process::exit(1);
            }
            let id = &args[2];
            state.notes.retain(|n| &n.id != id);
            let _ = storage::save_app_state(&state);
            print_json_state(&state);
        }
        "--toggle-pin" => {
            if args.len() < 3 {
                eprintln!("Usage: omanotes-engine --toggle-pin <id>");
                std::process::exit(1);
            }
            let id = &args[2];
            if let Some(note) = state.notes.iter_mut().find(|n| &n.id == id) {
                note.pinned = !note.pinned;
                note.updated_at = now_secs();
                let _ = storage::save_app_state(&state);
            }
            print_json_state(&state);
        }
        "--toggle-archive" => {
            if args.len() < 3 {
                eprintln!("Usage: omanotes-engine --toggle-archive <id>");
                std::process::exit(1);
            }
            let id = &args[2];
            if let Some(note) = state.notes.iter_mut().find(|n| &n.id == id) {
                note.archived = !note.archived;
                note.updated_at = now_secs();
                let _ = storage::save_app_state(&state);
            }
            print_json_state(&state);
        }
        "--set-color" => {
            if args.len() < 4 {
                eprintln!("Usage: omanotes-engine --set-color <id> <color>");
                std::process::exit(1);
            }
            let id = &args[2];
            let color = &args[3];
            if let Some(note) = state.notes.iter_mut().find(|n| &n.id == id) {
                note.color = color.clone();
                note.updated_at = now_secs();
                let _ = storage::save_app_state(&state);
            }
            print_json_state(&state);
        }
        "--toggle-check" => {
            if args.len() < 4 {
                eprintln!("Usage: omanotes-engine --toggle-check <note_id> <item_id>");
                std::process::exit(1);
            }
            let note_id = &args[2];
            let item_id = &args[3];
            if let Some(note) = state.notes.iter_mut().find(|n| &n.id == note_id) {
                if let Some(item) = note.checklist_items.iter_mut().find(|i| &i.id == item_id) {
                    item.checked = !item.checked;
                    note.updated_at = now_secs();
                    let _ = storage::save_app_state(&state);
                }
            }
            print_json_state(&state);
        }
        "--add-check-item" => {
            if args.len() > 2 {
                eprintln!("Security Error: Checklist item text is prohibited in process arguments to prevent /proc/<pid>/cmdline leaks. Pass framed JSON payload via stdin.");
                std::process::exit(1);
            }
            let payload: AddCheckItemPayload = match read_framed_stdin() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Payload error: {}", e);
                    std::process::exit(1);
                }
            };
            if let Some(note) = state.notes.iter_mut().find(|n| n.id == payload.note_id) {
                note.checklist_items.push(ChecklistItem {
                    id: generate_id(),
                    text: payload.text,
                    checked: false,
                });
                note.updated_at = now_secs();
                let _ = storage::save_app_state(&state);
            }
            print_json_state(&state);
        }
        "--edit" => {
            if args.len() > 2 {
                eprintln!("Security Error: Note content is prohibited in process arguments to prevent /proc/<pid>/cmdline leaks. Pass framed JSON payload via stdin.");
                std::process::exit(1);
            }
            let payload: EditPayload = match read_framed_stdin() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Payload error: {}", e);
                    std::process::exit(1);
                }
            };
            if let Some(note) = state.notes.iter_mut().find(|n| n.id == payload.id) {
                if !payload.title.is_empty() {
                    note.title = payload.title;
                }
                note.content = payload.content;
                note.updated_at = now_secs();
                let _ = storage::save_app_state(&state);
            }
            print_json_state(&state);
        }
        "--set-e2ee" => {
            if args.len() > 2 {
                eprintln!("Security Error: E2EE password is prohibited in process arguments to prevent /proc/<pid>/cmdline leaks. Pass framed payload via stdin.");
                std::process::exit(1);
            }
            let password = match read_password_stdin() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Password read error: {}", e);
                    std::process::exit(1);
                }
            };
            if password.is_empty() {
                state.e2ee_enabled = false;
            } else {
                state.e2ee_enabled = true;
                if state.salt.is_empty() {
                    if let Ok(s) = crypto::generate_salt() {
                        state.salt = hex::encode(s);
                    }
                }
            }
            let _ = storage::save_app_state(&state);
            print_json_state(&state);
        }
        "--set-cloud" => {
            if args.len() > 2 {
                eprintln!("Security Error: Cloud configuration is prohibited in process arguments to prevent /proc/<pid>/cmdline leaks. Pass framed JSON payload via stdin.");
                std::process::exit(1);
            }
            let payload: CloudPayload = match read_framed_stdin() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Payload error: {}", e);
                    std::process::exit(1);
                }
            };
            state.cloud.provider = payload.provider;
            state.cloud.rclone_remote = payload.rclone_remote;
            state.cloud.git_remote = payload.git_remote;
            state.cloud.auto_sync = payload.auto_sync;
            let _ = storage::save_app_state(&state);
            print_json_state(&state);
        }
        "--sync" => {
            if args.len() > 2 {
                eprintln!("Security Error: E2EE password is prohibited in process arguments to prevent /proc/<pid>/cmdline leaks. Pass password via stdin.");
                std::process::exit(1);
            }
            let password = read_password_stdin().unwrap_or_default();
            let res = sync::sync_cloud(&mut state, &password);
            let output = serde_json::json!({
                "success": res.success,
                "message": res.message,
                "cloud": state.cloud,
            });
            println!("{}", output);
        }
        "--pull" => {
            if args.len() > 2 {
                eprintln!("Security Error: E2EE password is prohibited in process arguments to prevent /proc/<pid>/cmdline leaks. Pass password via stdin.");
                std::process::exit(1);
            }
            let password = read_password_stdin().unwrap_or_default();
            let res = sync::pull_cloud(&mut state, &password);
            let output = serde_json::json!({
                "success": res.success,
                "message": res.message,
                "cloud": state.cloud,
            });
            println!("{}", output);
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            std::process::exit(1);
        }
    }
}
