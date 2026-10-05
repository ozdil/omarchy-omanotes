# Changelog

All notable changes to OmaNotes will be documented in this file.

## [1.4.0] - 2026-10-05

### Added
- User-space uninstallation script (`uninstall.sh`) with ownership and SHA-256 verification (Rule 7 standard).
- Installation manifest generator (`install_manifest.json`) in `~/.local/state/omarchy/omanotes/` with 0600 file permissions and 0700 directory permissions.
- Pre-installation ownership verification: automatic refusal when target binaries in `~/.local/bin` are symlinks, untracked foreign files, or modified.
- Automated Python test suite for installer and uninstaller lifecycle verification (`tests/test_installer_manifest.py`).

### Fixed
- Fixed critical script startup failure in `omanotes-dashboard` where `DIR` was referenced before initialization under `set -u` (`DIR: unbound variable`).
- Standardized bounded input reading in `src/main.rs`: upgraded `read_framed_stdin` and `read_password_stdin` to use `take(MAX_PAYLOAD_BYTES + 1)` and `read_to_end`, supporting multiline/formatted JSON payloads while enforcing strict 256 KiB DoS limits.
- Fixed typography standard in `Panel.qml`: converted direct `Style.font.family` instances to `root.fontFamily` fallback chain (`JetBrainsMono Nerd Font, JetBrains Mono, monospace`).
- Replaced dingbat close symbol (`✕`) with Nerd Font glyph (`\uf00d`) in `Panel.qml` modal to comply with Zero Emoji policy.
- Removed unicode emojis from `README.md`.
- Updated CLI usage documentation in `README.md` and `README.tr.md` to show secure framed JSON input over stdin.
- Fixed code formatting with `cargo fmt`.

### Security
- Reinforced HANCORE argument and process group protections.
- Retained full Argon2id v2 KDF (RFC 9106, 64 MiB m_cost, 3 iterations) and AES-256-GCM zero-knowledge encryption with backward compatibility for PBKDF2 v1 envelopes.

## [1.3.1] - 2026-10-02

### Added
- Standardized dynamic manifest modal dialog in Quickshell panel.
- Verified Omarchy plugin badge and metadata alignment.

## [1.3.0] - 2026-09-29

### Added
- Argon2id v2 KDF upgrade with memory-hard password hashing.
- Legacy PBKDF2 v1 migration layer for stored vaults.
- Multi-cloud synchronization with rclone and Git vault.
