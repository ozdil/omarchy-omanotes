# OmaNotes - Zero-Knowledge E2EE & Cloud-Sync Google Keep Clone for Omarchy Linux

[![Buy Me A Coffee](https://img.shields.io/badge/Buy_Me_A_Coffee-Support_Development-FFDD00?style=for-the-badge&logo=buy-me-a-coffee&logoColor=black)](https://buymeacoffee.com/ozdil)

A fast, private, and mouse/touch-friendly note-taking and checklist application built natively with Quickshell for Omarchy Linux, backed by a hardened Rust engine with AES-256-GCM Zero-Knowledge End-to-End Encryption and multi-cloud synchronization (Google Drive & Git).

Author: Ozan Özdil (ozdil)  
License: MIT  
Plugin ID: ozdil.omanotes

---

## Key Features

- **Google Keep-Style Color Cards:** Categorize and visually organize notes with vibrant pastel themes (Yellow, Green, Blue, Purple, Red, Teal).
- **Interactive Checklists (To-Do):** Native strikethrough checkboxes, instant item completion, and one-click removal of completed items.
- **Mouse-First & Keyboardless Usability:**
  - One-click **"📋 Paste from Clipboard"** button (via `wl-paste`) creates cards instantly without typing.
  - One-click template buttons: `🛒 Groceries`, `📅 Priorities`, `💡 Idea`.
  - Mouse-driven color-dot and status filter chips (`All`, `📌 Pinned`, `󰄲 Checklist`, and color swatches).
  - Quick action bar on each card: Pin (`󰤱`), Duplicate (`󰆏`), Delete (`󰅖`), and inline color palette.
- **Zero-Knowledge End-to-End Encryption (E2EE):**
  - All notes are encrypted client-side using PBKDF2-HMAC-SHA256 (10,000 iterations), 16-byte random salt, 12-byte random nonce, and AES-256-GCM before ever leaving your machine.
  - Zero plaintext leakage in cloud storage or backups.
- **Multi-Cloud Synchronization:**
  - **Google Drive, Nextcloud, WebDAV:** Seamless encrypted sync via `rclone`.
  - **Git Vault:** Commit and push encrypted `notes.enc` snapshots to any private Git repository.
  - Disaster Recovery: One-click cloud pull and decrypt (`--pull`) to restore all notes onto a fresh system.
- **Full Omarchy Design Language:** Built using `qs.Commons`, `qs.Ui` (`Button`, `BorderSurface`, `KeyboardPanel`, `Style`, `Color`).

---

## System Requirements

- **Rust & Cargo:** `rustc` and `cargo` (for local source compilation).
- **Quickshell:** Native Wayland desktop shell (`quickshell`).
- **Wayland Clipboard Utility:** `wl-clipboard` (for one-click clipboard capture).
- **Optional Cloud Providers:**
  - `rclone` (for Google Drive / WebDAV / Nextcloud synchronization).
  - `git` (for Git-based encrypted repository synchronization).

---

## Installation and Setup

### Why Building from Source is Required
Under the Omarchy Linux Security Standards (`AGENTS.md` Rule 5.3), precompiled binaries are strictly forbidden from Git repositories to guarantee user system integrity. Therefore, the native engine must be compiled from source on your local machine after adding the plugin.

### Step 1: Add the Plugin to Omarchy
```bash
omarchy plugin add https://github.com/ozdil/omarchy-omanotes.git
```

### Step 2: Build the Native Engine
Navigate to the plugin directory and compile the engine:
```bash
cd ~/.config/omarchy/plugins/ozdil.omanotes
cargo build --release --locked
install -m 755 target/release/omanotes-engine ./omanotes-engine
```

### Step 3: Add to Omarchy Shell Configuration
Add `ozdil.omanotes` to `bar.layout.right` in `~/.config/omarchy/shell.json`:
```json
{
  "id": "ozdil.omanotes"
}
```

### Step 4: Reload Shell
```bash
omarchy-restart-shell
```

---

## CLI Usage

OmaNotes provides a robust, zero-panic CLI engine for scripting, terminal usage, and desktop integrations:

```bash
# Display system status, note counts, and cloud sync status
omanotes-engine --status

# Add a note
omanotes-engine --add "Meeting Notes" "Discuss architecture and deployment" "blue" false "work"

# Add a template note (shopping, daily, idea, reminder)
omanotes-engine --add-template daily

# Paste directly from Wayland clipboard
omanotes-engine --add-clipboard "yellow"

# Toggle pin on a note
omanotes-engine --toggle-pin <NOTE_ID>

# Toggle checklist item
omanotes-engine --toggle-check <NOTE_ID> <ITEM_ID>

# Set cloud synchronization provider
omanotes-engine --set-cloud rclone "gdrive:OmarchyNotes_Vault" "" false
omanotes-engine --set-cloud git "" "/path/to/private-vault-repo" false

# Sync encrypted notes to configured cloud
echo "<E2EE_PASSWORD>" | omanotes-engine --sync

# Pull and decrypt notes from cloud (Disaster Recovery)
echo "<E2EE_PASSWORD>" | omanotes-engine --pull
```

---

## Security Architecture Standards (Omarchy Security Compliance)

OmaNotes strictly adheres to the Omarchy Linux Security Architecture Standards:

1. **Subprocess Isolation:** All external commands (`rclone`, `git`, `wl-paste`) are executed in independent process groups (`cmd.process_group(0)`), with non-blocking I/O (`O_NONBLOCK`), bounded polling (`poll()`), and absolute monotonic deadlines (`Instant::now() >= deadline`).
2. **Process Group Reaping:** If a subprocess times out or crashes, `SIGTERM` is sent followed by immediate `SIGKILL` (5-10ms) to clean up all background grandchild processes.
3. **Storage Security (0600 & 0700):**
   - User notes and configuration are saved to `~/.local/share/omanotes/notes.json` and `~/.local/state/omanotes/config.json`.
   - Files are written atomically using temporary files (`.tmp_*`), synced to disk (`sync_all()`), and created with explicit POSIX mode `0600` permissions.
   - Symlinks and foreign UIDs are strictly rejected before reading or writing.
4. **Bounded Cloud Restore & Storage Reads:**
   - Cloud restore downloads exclusively into a private temporary staging file (`.tmp_cloud_restore_*`) enforcing a strict 10 MiB ciphertext ceiling (`--max-size 10M` and `--` flag delimiter).
   - In case of overflow, timeout, or error, the staging file is immediately aborted and unlinked via RAII guard before publication.
   - Staged payloads are validated for structure and envelope integrity before atomic rename over `notes.enc`.
   - All storage reads enforce descriptor-bound `fstat` checks, a strict 10 MiB ceiling, and `take(MAX + 1)` bounded streaming with explicit oversize rejection.
5. **QML UI Hardening:**
   - All user, system, and clipboard strings are rendered with `textFormat: Text.PlainText` to prevent HTML, CSS, or script injection.
   - Zero dynamic code evaluation (`eval()`, `createQmlObject()`).
6. **Git Argument Injection Prevention:**
   - Git commands utilize discrete argument slices with `--` delimiters and `-C <path>` canonical paths. `GIT_TERMINAL_PROMPT=0` prevents hanging prompts.

---

## Support & Sponsorship

If you find OmaNotes useful and want to support independent Linux development:

<a href="https://buymeacoffee.com/ozdil" target="_blank"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" style="height: 50px !important;width: 180px !important;" ></a>

---

## License

MIT License. See [LICENSE](LICENSE) for details.
