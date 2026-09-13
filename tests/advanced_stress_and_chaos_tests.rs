use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn engine_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_omanotes-engine"))
}

fn run_engine(
    bin: &PathBuf,
    args: &[&str],
    stdin_payload: &[u8],
    envs: &[(&str, &PathBuf)],
) -> std::process::Output {
    let mut cmd = Command::new(bin);
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn engine");
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(stdin_payload);
    }
    child.wait_with_output().expect("wait for output")
}

/// 1. Cryptographic Tampering & AEAD Bit-Flipping Test
#[test]
fn test_security_ciphertext_tampering_rejected() {
    let test_dir = PathBuf::from("/tmp/test_omanotes_tamper");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test dir");

    let bin = engine_bin();

    // Add a note via bounded framed stdin
    let add_payload = serde_json::json!({
        "title": "Secret Data",
        "content": "Original untampered payload",
        "color": "yellow"
    });
    let out = run_engine(
        &bin,
        &["--add"],
        format!("{}\n", add_payload).as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );
    assert!(out.status.success());

    // Setup git vault
    let git_repo = test_dir.join("repo");
    fs::create_dir_all(&git_repo).expect("create repo dir");
    let _ = Command::new("git")
        .args(["-C", git_repo.to_str().unwrap(), "init"])
        .output();
    let _ = Command::new("git")
        .args([
            "-C",
            git_repo.to_str().unwrap(),
            "config",
            "user.email",
            "t@t.org",
        ])
        .output();
    let _ = Command::new("git")
        .args([
            "-C",
            git_repo.to_str().unwrap(),
            "config",
            "user.name",
            "Test",
        ])
        .output();

    let cloud_payload = serde_json::json!({
        "provider": "git",
        "rclone_remote": "",
        "git_remote": git_repo.to_str().unwrap(),
        "auto_sync": false
    });
    let _ = run_engine(
        &bin,
        &["--set-cloud"],
        format!("{}\n", cloud_payload).as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );

    // Sync with password via stdin
    let password = "TamperProofPassword2026!";
    let sync_out = run_engine(
        &bin,
        &["--sync"],
        format!("{}\n", password).as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );
    assert!(sync_out.status.success());

    let enc_path = git_repo.join("notes.enc");
    assert!(enc_path.exists());

    let enc_raw = fs::read_to_string(&enc_path).expect("read notes.enc");
    let mut enc_json: serde_json::Value = serde_json::from_str(&enc_raw).expect("parse json");

    // Case A: Bit-flipping in ciphertext
    let orig_cipher = enc_json["ciphertext"].as_str().unwrap().to_string();
    let mut tampered_cipher = orig_cipher.clone();
    let first_char = tampered_cipher.chars().next().unwrap();
    let replacement = if first_char == 'a' { 'b' } else { 'a' };
    tampered_cipher.replace_range(..1, &replacement.to_string());
    enc_json["ciphertext"] = serde_json::Value::String(tampered_cipher);

    fs::write(&enc_path, serde_json::to_string_pretty(&enc_json).unwrap()).expect("write tampered");

    // Delete local notes and attempt pull: MUST fail authentication
    let _ = fs::remove_file(test_dir.join("notes.json"));
    let pull_out = run_engine(
        &bin,
        &["--pull"],
        format!("{}\n", password).as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );
    let pull_res: serde_json::Value =
        serde_json::from_slice(&pull_out.stdout).expect("parse pull json");
    assert_eq!(
        pull_res["success"], false,
        "AEAD must reject tampered ciphertext"
    );
    assert!(
        pull_res["message"]
            .as_str()
            .unwrap()
            .contains("decryption failed"),
        "Error message must specify decryption failure"
    );

    // Case B: Bit-flipping in nonce
    enc_json["ciphertext"] = serde_json::Value::String(orig_cipher);
    let orig_nonce = enc_json["nonce"].as_str().unwrap().to_string();
    let mut tampered_nonce = orig_nonce.clone();
    let n_first = tampered_nonce.chars().next().unwrap();
    let n_repl = if n_first == '0' { '1' } else { '0' };
    tampered_nonce.replace_range(..1, &n_repl.to_string());
    enc_json["nonce"] = serde_json::Value::String(tampered_nonce);

    fs::write(&enc_path, serde_json::to_string_pretty(&enc_json).unwrap())
        .expect("write tampered nonce");
    let pull_nonce_out = run_engine(
        &bin,
        &["--pull"],
        format!("{}\n", password).as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );
    let pull_nonce_res: serde_json::Value =
        serde_json::from_slice(&pull_nonce_out.stdout).expect("parse pull json");
    assert_eq!(
        pull_nonce_res["success"], false,
        "AEAD must reject tampered nonce"
    );

    let _ = fs::remove_dir_all(&test_dir);
}

/// 2. Shell Injection & Malicious Input Fuzzing Test
#[test]
fn test_security_malicious_payload_and_shell_injection() {
    let test_dir = PathBuf::from("/tmp/test_omanotes_shell_injection");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test dir");

    let bin = engine_bin();

    // Clean up any canary files beforehand
    let canaries = [
        "/tmp/pwned_title",
        "/tmp/pwned_content",
        "/tmp/pwned_pipe",
        "/tmp/pwned_color",
        "/tmp/pwned_tags",
    ];
    for c in &canaries {
        let _ = fs::remove_file(c);
    }

    let evil_title = "'; touch /tmp/pwned_title ; rm -rf / ; #";
    let evil_content = "$(touch /tmp/pwned_content) | touch /tmp/pwned_pipe && echo 'evil'";
    let evil_color = "red; touch /tmp/pwned_color";
    let evil_tags = "tag1,tag2; touch /tmp/pwned_tags";

    let evil_payload = serde_json::json!({
        "title": evil_title,
        "content": evil_content,
        "color": evil_color,
        "is_checklist": false,
        "tags": [evil_tags]
    });
    let out = run_engine(
        &bin,
        &["--add"],
        format!("{}\n", evil_payload).as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );
    assert!(out.status.success());

    // Verify NONE of the canaries were triggered!
    for c in &canaries {
        assert!(
            !PathBuf::from(c).exists(),
            "CRITICAL SECURITY FAILURE: Canary {} was created via shell injection!",
            c
        );
    }

    // Verify the note was stored safely verbatim as plain text
    let notes_raw = fs::read_to_string(test_dir.join("notes.json")).expect("read notes.json");
    assert!(
        notes_raw.contains(evil_title),
        "Payload must be stored as plain string"
    );
    assert!(
        notes_raw.contains("touch /tmp/pwned_content"),
        "Content must be stored as plain string"
    );

    let _ = fs::remove_dir_all(&test_dir);
}

/// 3. Sudden Power-Cut & Corrupted JSON Resilience Test
#[test]
fn test_storage_corrupted_json_recovery() {
    let test_dir = PathBuf::from("/tmp/test_omanotes_corrupt_recovery");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test dir");

    let bin = engine_bin();

    // Write incomplete, truncated JSON into notes.json
    let corrupted_notes = test_dir.join("notes.json");
    fs::write(
        &corrupted_notes,
        b"{\"version\": 1, \"notes\": [{\"id\": \"123\", \"title\": \"Incompl",
    )
    .expect("write corrupt");

    // Engine --status must not panic or crash
    let status_out = Command::new(&bin)
        .args(["--status"])
        .env("OMANOTES_DATA_DIR", &test_dir)
        .env("OMANOTES_STATE_DIR", &test_dir)
        .output()
        .expect("status check on corrupt notes");
    assert!(
        status_out.status.success(),
        "Engine must recover from corrupt notes.json"
    );

    // Adding a note should safely write fresh, valid JSON
    let payload = serde_json::json!({
        "title": "Recovered Note",
        "content": "State preserved after corruption",
        "color": "green"
    });
    let add_out = run_engine(
        &bin,
        &["--add"],
        format!("{}\n", payload).as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );
    assert!(add_out.status.success());

    // Verify the saved file is now valid JSON
    let notes_bytes = fs::read(&corrupted_notes).expect("read notes");
    let parsed: serde_json::Value =
        serde_json::from_slice(&notes_bytes).expect("must be valid JSON now");
    assert!(parsed.is_array() || parsed.is_object());

    // Also test corrupt config.json
    let corrupted_config = test_dir.join("config.json");
    fs::write(&corrupted_config, b"\x00\xFF\xFE\x12\x34garbage").expect("write corrupt config");

    let status_config = Command::new(&bin)
        .args(["--status"])
        .env("OMANOTES_DATA_DIR", &test_dir)
        .env("OMANOTES_STATE_DIR", &test_dir)
        .output()
        .expect("status on corrupt config");
    assert!(
        status_config.status.success(),
        "Engine must recover from corrupt config.json"
    );

    let _ = fs::remove_dir_all(&test_dir);
}

/// 4. High-Volume Stress & Large Payload Benchmark Test
#[test]
fn test_stress_high_volume_notes() {
    let test_dir = PathBuf::from("/tmp/test_omanotes_stress_volume");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test dir");

    let bin = engine_bin();

    // 1. Generate 1,000 realistic notes
    let mut notes_vec = Vec::with_capacity(1000);
    for i in 0..1000 {
        notes_vec.push(serde_json::json!({
            "id": format!("stress_note_{:04}", i),
            "title": format!("Stress Test Note #{}", i),
            "content": format!("Simulated note content for card index {} with tag #performance", i),
            "is_checklist": i % 3 == 0,
            "checklist_items": if i % 3 == 0 {
                vec![
                    serde_json::json!({"id": format!("item_{}_1", i), "text": "Item Alpha", "checked": true}),
                    serde_json::json!({"id": format!("item_{}_2", i), "text": "Item Beta", "checked": false}),
                ]
            } else {
                vec![]
            },
            "color": match i % 6 {
                0 => "yellow",
                1 => "green",
                2 => "blue",
                3 => "purple",
                4 => "red",
                _ => "teal",
            },
            "pinned": i < 10,
            "archived": false,
            "tags": vec![format!("tag{}", i % 10)],
            "created_at": 1789000000 + i,
            "updated_at": 1789000000 + i,
        }));
    }

    let notes_json = serde_json::to_vec_pretty(&notes_vec).unwrap();
    fs::write(test_dir.join("notes.json"), &notes_json).unwrap();

    // 2. Measure status check response time on 1,000 notes
    let t0 = Instant::now();
    let status_out = Command::new(&bin)
        .args(["--status"])
        .env("OMANOTES_DATA_DIR", &test_dir)
        .env("OMANOTES_STATE_DIR", &test_dir)
        .output()
        .expect("status on 1000 notes");
    let elapsed = t0.elapsed();
    assert!(status_out.status.success());
    assert!(
        elapsed < Duration::from_millis(500),
        "1,000 notes status check must be fast! Took {:?}",
        elapsed
    );

    let parsed_status: serde_json::Value = serde_json::from_slice(&status_out.stdout).unwrap();
    assert_eq!(parsed_status["total_notes"], 1000);
    assert_eq!(parsed_status["pinned_notes"], 10);

    // 3. Add note #1001 with large payload (64 KB string) via bounded framed stdin
    let large_body = "A".repeat(65536);
    let payload = serde_json::json!({
        "title": "Massive Payload Note",
        "content": large_body,
        "color": "blue"
    });
    let add_large = run_engine(
        &bin,
        &["--add"],
        format!("{}\n", payload).as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );
    assert!(add_large.status.success());

    let status_after = Command::new(&bin)
        .args(["--status"])
        .env("OMANOTES_DATA_DIR", &test_dir)
        .env("OMANOTES_STATE_DIR", &test_dir)
        .output()
        .expect("status after large note");
    let parsed_after: serde_json::Value = serde_json::from_slice(&status_after.stdout).unwrap();
    assert_eq!(parsed_after["total_notes"], 1001);

    let _ = fs::remove_dir_all(&test_dir);
}

/// 5. Concurrency & Multi-Threaded Atomic Writes Test
#[test]
fn test_concurrency_atomic_writes() {
    let test_dir = PathBuf::from("/tmp/test_omanotes_concurrency");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test dir");

    let bin = engine_bin();

    // Spawn 10 concurrent threads invoking the engine CLI simultaneously
    let mut handles = vec![];
    for i in 0..10 {
        let b = bin.clone();
        let td = test_dir.clone();
        handles.push(thread::spawn(move || {
            let payload = serde_json::json!({
                "title": format!("Thread Note #{}", i),
                "content": "Concurrent write content",
                "color": "purple"
            });
            let out = run_engine(
                &b,
                &["--add"],
                format!("{}\n", payload).as_bytes(),
                &[("OMANOTES_DATA_DIR", &td), ("OMANOTES_STATE_DIR", &td)],
            );
            assert!(out.status.success());
        }));
    }

    for h in handles {
        h.join().expect("thread join");
    }

    // Verify final notes.json is clean, valid JSON
    let notes_path = test_dir.join("notes.json");
    assert!(notes_path.exists());
    let bytes = fs::read(&notes_path).expect("read notes");
    let parsed: serde_json::Value =
        serde_json::from_slice(&bytes).expect("notes.json must be valid JSON");
    assert!(parsed.is_array() || parsed.is_object());

    // Verify 0600 permissions
    let meta = fs::metadata(&notes_path).expect("meta");
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);

    // Verify no temporary .tmp_* files left behind
    for entry in fs::read_dir(&test_dir).expect("read dir") {
        let entry = entry.unwrap();
        let file_name = entry.file_name().to_string_lossy().to_string();
        assert!(
            !file_name.starts_with(".tmp_"),
            "Leaked temporary file found: {}",
            file_name
        );
    }

    let _ = fs::remove_dir_all(&test_dir);
}

/// 6. Broken Cloud & Chaos Network Timeout Test
#[test]
fn test_chaos_broken_cloud_timeout() {
    let test_dir = PathBuf::from("/tmp/test_omanotes_chaos_cloud");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test dir");

    let bin = engine_bin();

    // 1. Add a sample note
    let payload = serde_json::json!({
        "title": "Chaos Note",
        "content": "Cloud error resilience test",
        "color": "yellow"
    });
    let _ = run_engine(
        &bin,
        &["--add"],
        format!("{}\n", payload).as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );

    // 2. Point git remote to an invalid/non-existent destination
    let cloud_payload = serde_json::json!({
        "provider": "git",
        "rclone_remote": "",
        "git_remote": "/non_existent_folder_path/vault_repo",
        "auto_sync": false
    });
    let _ = run_engine(
        &bin,
        &["--set-cloud"],
        format!("{}\n", cloud_payload).as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );

    // 3. Attempt sync: must NOT hang and must fail gracefully with success=false
    let t0 = Instant::now();
    let sync_out = run_engine(
        &bin,
        &["--sync"],
        b"any_password\n",
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );
    let elapsed = t0.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "Broken cloud must fail fast! Took {:?}",
        elapsed
    );
    assert!(sync_out.status.success()); // Command returns exit 0 with JSON payload

    let res_json: serde_json::Value =
        serde_json::from_slice(&sync_out.stdout).expect("parse response");
    assert_eq!(
        res_json["success"], false,
        "Sync to non-existent repo must report success=false"
    );
    assert!(
        res_json["message"]
            .as_str()
            .unwrap()
            .contains("does not exist"),
        "Error message must indicate path problem"
    );

    // 4. Point rclone to an invalid remote configuration
    let rclone_payload = serde_json::json!({
        "provider": "rclone",
        "rclone_remote": "invalid_remote_404:Vault",
        "git_remote": "",
        "auto_sync": false
    });
    let _ = run_engine(
        &bin,
        &["--set-cloud"],
        format!("{}\n", rclone_payload).as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );

    let rclone_out = run_engine(
        &bin,
        &["--sync"],
        b"any_password\n",
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );
    let rclone_json: serde_json::Value =
        serde_json::from_slice(&rclone_out.stdout).expect("parse rclone response");
    assert_eq!(
        rclone_json["success"], false,
        "Broken rclone must report success=false without panic"
    );

    let _ = fs::remove_dir_all(&test_dir);
}

/// 7. Security: Overrun Buffer Limit (> 256 KiB) & Malformed JSON Protection
#[test]
fn test_security_payload_overrun_and_malformed_json() {
    let test_dir = PathBuf::from("/tmp/test_omanotes_overrun_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test dir");

    let bin = engine_bin();

    // A. Malformed JSON must fail cleanly with exit code 1 and no panics
    let bad_out = run_engine(
        &bin,
        &["--add"],
        b"{\"broken_json\": [unclosed\n",
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );
    assert!(!bad_out.status.success());
    let stderr = String::from_utf8_lossy(&bad_out.stderr);
    assert!(!stderr.contains("panicked"));

    // B. Overrun payload (> 256 KiB): send 300 KiB to ensure handle.take() caps safely
    let mut huge_payload = "X".repeat(300_000);
    huge_payload.push('\n');
    let overrun_out = run_engine(
        &bin,
        &["--add"],
        huge_payload.as_bytes(),
        &[("OMANOTES_DATA_DIR", &test_dir), ("OMANOTES_STATE_DIR", &test_dir)],
    );
    assert!(!overrun_out.status.success());
    let err_str = String::from_utf8_lossy(&overrun_out.stderr);
    assert!(!err_str.contains("panicked"));

    let _ = fs::remove_dir_all(&test_dir);
}
