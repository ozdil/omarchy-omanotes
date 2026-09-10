mod crypto;
mod storage;
mod subproc;
mod sync;

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};
use storage::{AppState, ChecklistItem, Note};

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
            // --add <title> [content] [color] [is_checklist] [tags_csv]
            if args.len() < 3 {
                eprintln!("Usage: omanotes-engine --add <title> [content] [color] [is_checklist] [tags_csv]");
                std::process::exit(1);
            }
            let title = args.get(2).map(|s| s.as_str()).unwrap_or("New Note");
            let content = args.get(3).map(|s| s.as_str()).unwrap_or("");
            let color = args.get(4).map(|s| s.as_str()).unwrap_or("yellow");
            let is_checklist = args
                .get(5)
                .and_then(|s| s.parse::<bool>().ok())
                .unwrap_or(false);
            let tags: Vec<String> = if let Some(t) = args.get(6) {
                if !t.is_empty() {
                    t.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            let mut items = Vec::new();
            if is_checklist {
                for line in content.lines() {
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

            let new_note = Note {
                id: generate_id(),
                title: title.to_string(),
                content: content.to_string(),
                is_checklist,
                checklist_items: items,
                color: color.to_string(),
                pinned: false,
                archived: false,
                tags,
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
            if args.len() < 4 {
                eprintln!("Usage: omanotes-engine --add-check-item <note_id> <item_text>");
                std::process::exit(1);
            }
            let note_id = &args[2];
            let item_text = &args[3];
            if let Some(note) = state.notes.iter_mut().find(|n| &n.id == note_id) {
                note.checklist_items.push(ChecklistItem {
                    id: generate_id(),
                    text: item_text.clone(),
                    checked: false,
                });
                note.updated_at = now_secs();
                let _ = storage::save_app_state(&state);
            }
            print_json_state(&state);
        }
        "--edit" => {
            if args.len() < 4 {
                eprintln!("Usage: omanotes-engine --edit <id> <title> [content]");
                std::process::exit(1);
            }
            let id = &args[2];
            let title = &args[3];
            let content = args.get(4).map(|s| s.as_str()).unwrap_or("");
            if let Some(note) = state.notes.iter_mut().find(|n| &n.id == id) {
                note.title = title.clone();
                note.content = content.to_string();
                note.updated_at = now_secs();
                let _ = storage::save_app_state(&state);
            }
            print_json_state(&state);
        }
        "--set-e2ee" => {
            if args.len() < 3 {
                eprintln!("Usage: omanotes-engine --set-e2ee <password>");
                std::process::exit(1);
            }
            let password = &args[2];
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
            // --set-cloud <provider> [rclone_remote] [git_remote] [auto_sync]
            if args.len() < 3 {
                eprintln!("Usage: omanotes-engine --set-cloud <provider> [rclone_remote] [git_remote] [auto_sync]");
                std::process::exit(1);
            }
            state.cloud.provider = args.get(2).cloned().unwrap_or_else(|| "none".to_string());
            state.cloud.rclone_remote = args.get(3).cloned().unwrap_or_default();
            state.cloud.git_remote = args.get(4).cloned().unwrap_or_default();
            state.cloud.auto_sync = args
                .get(5)
                .and_then(|s| s.parse::<bool>().ok())
                .unwrap_or(false);
            let _ = storage::save_app_state(&state);
            print_json_state(&state);
        }
        "--sync" => {
            let password = if args.len() > 2 { args[2].as_str() } else { "" };
            let res = sync::sync_cloud(&mut state, password);
            let output = serde_json::json!({
                "success": res.success,
                "message": res.message,
                "cloud": state.cloud,
            });
            println!("{}", output);
        }
        "--pull" => {
            let password = if args.len() > 2 { args[2].as_str() } else { "" };
            let res = sync::pull_cloud(&mut state, password);
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
