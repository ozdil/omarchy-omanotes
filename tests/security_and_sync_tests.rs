use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn test_security_symlink_attack_rejected() {
    let test_dir = PathBuf::from("/tmp/test_omanotes_symlink_attack");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test dir");

    let target_file = test_dir.join("innocent_target.txt");
    fs::write(&target_file, b"Do not overwrite me!").expect("write target");

    let symlink_file = test_dir.join("notes.json");
    std::os::unix::fs::symlink(&target_file, &symlink_file).expect("create symlink");

    // Run engine CLI targeting this file: safe_read and atomic_write must reject symlink
    let status = Command::new("cargo")
        .args(["run", "--quiet", "--", "--status"])
        .env("OMANOTES_DATA_DIR", &test_dir)
        .env("OMANOTES_STATE_DIR", &test_dir)
        .output()
        .expect("cargo run");

    // Should not panic, must handle safely
    assert!(status.status.success());
    // Innocent target must remain intact
    let content = fs::read_to_string(&target_file).expect("read");
    assert_eq!(content, "Do not overwrite me!");

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_security_stubborn_grandchild_killed() {
    let start = Instant::now();
    let status = Command::new("cargo")
        .args(["test", "--", "test_process_group_deadline_enforcement"])
        .output()
        .expect("run deadline test");

    assert!(status.status.success());
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn test_git_e2ee_cloud_sync_and_pull_roundtrip() {
    let base_dir = PathBuf::from("/tmp/test_omanotes_git_e2ee_sync");
    let _ = fs::remove_dir_all(&base_dir);
    fs::create_dir_all(&base_dir).expect("create base dir");

    let data_dir = base_dir.join("data");
    let state_dir = base_dir.join("state");
    let git_repo = base_dir.join("vault_repo");
    fs::create_dir_all(&data_dir).expect("create data dir");
    fs::create_dir_all(&state_dir).expect("create state dir");
    fs::create_dir_all(&git_repo).expect("create git dir");

    // Initialize git repository
    let git_init = Command::new("git")
        .args(["-C", git_repo.to_str().unwrap(), "init"])
        .output()
        .expect("git init");
    assert!(git_init.status.success());

    // Configure user in test git repo
    let _ = Command::new("git")
        .args([
            "-C",
            git_repo.to_str().unwrap(),
            "config",
            "user.email",
            "test@omarchy.org",
        ])
        .output();
    let _ = Command::new("git")
        .args([
            "-C",
            git_repo.to_str().unwrap(),
            "config",
            "user.name",
            "Omarchy Test",
        ])
        .output();

    // Helper to run cargo with stdin piping
    let run_cargo_stdin = |args: &[&str], stdin_data: &[u8]| -> std::process::Output {
        use std::io::Write;
        let mut cmd = Command::new("cargo");
        cmd.args(args);
        cmd.env("OMANOTES_DATA_DIR", &data_dir);
        cmd.env("OMANOTES_STATE_DIR", &state_dir);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().expect("spawn cargo");
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(stdin_data);
        }
        child.wait_with_output().expect("wait for output")
    };

    // 1. Add a note with secret content via bounded framed stdin
    let add_payload = serde_json::json!({
        "title": "Top Secret Plan",
        "content": "Launch rocket at midnight with code 998877.",
        "color": "yellow",
        "is_checklist": false
    });
    let add_out = run_cargo_stdin(
        &["run", "--quiet", "--", "--add"],
        format!("{}\n", add_payload).as_bytes(),
    );
    assert!(add_out.status.success());

    // 2. Configure git cloud backup via bounded framed stdin
    let cloud_payload = serde_json::json!({
        "provider": "git",
        "rclone_remote": "",
        "git_remote": git_repo.to_str().unwrap(),
        "auto_sync": true
    });
    let conf_out = run_cargo_stdin(
        &["run", "--quiet", "--", "--set-cloud"],
        format!("{}\n", cloud_payload).as_bytes(),
    );
    assert!(conf_out.status.success());

    // 3. Sync with password via stdin
    let password = "UltraSecurePassword#2026";
    let sync_out = run_cargo_stdin(
        &["run", "--quiet", "--", "--sync"],
        format!("{}\n", password).as_bytes(),
    );
    assert!(sync_out.status.success());

    let sync_json: serde_json::Value =
        serde_json::from_slice(&sync_out.stdout).expect("parse sync response");
    assert_eq!(sync_json["success"], true, "Sync must succeed");

    // 4. Verify notes.enc in git repo
    let enc_in_git = git_repo.join("notes.enc");
    assert!(
        enc_in_git.exists(),
        "notes.enc must exist in git repository"
    );

    let enc_content = fs::read_to_string(&enc_in_git).expect("read notes.enc");
    // CRITICAL SECURITY ASSERTION: Plain text MUST NEVER be in notes.enc!
    assert!(
        !enc_content.contains("Top Secret Plan"),
        "Plaintext leak in encrypted backup!"
    );
    assert!(
        !enc_content.contains("998877"),
        "Plaintext content leak in encrypted backup!"
    );
    assert!(
        enc_content.contains("\"ciphertext\""),
        "Must contain ciphertext"
    );

    // 5. Verify git commit exists
    let log_out = Command::new("git")
        .args([
            "-C",
            git_repo.to_str().unwrap(),
            "log",
            "-n",
            "1",
            "--oneline",
        ])
        .output()
        .expect("git log");
    assert!(log_out.status.success());
    let log_str = String::from_utf8_lossy(&log_out.stdout);
    assert!(log_str.contains("sync: e2ee encrypted notes update"));

    // 6. Delete local notes.json to simulate clean machine / disaster recovery
    let local_notes = data_dir.join("notes.json");
    let _ = fs::remove_file(&local_notes);

    // 7. Pull and Decrypt via stdin
    let pull_out = run_cargo_stdin(
        &["run", "--quiet", "--", "--pull"],
        format!("{}\n", password).as_bytes(),
    );
    assert!(pull_out.status.success());

    let pull_json: serde_json::Value =
        serde_json::from_slice(&pull_out.stdout).expect("parse pull response");
    assert_eq!(
        pull_json["success"], true,
        "Pull must succeed: {}",
        pull_json["message"]
    );

    // Verify restored notes
    let status_out = Command::new("cargo")
        .args(["run", "--quiet", "--", "--status"])
        .env("OMANOTES_DATA_DIR", &data_dir)
        .env("OMANOTES_STATE_DIR", &state_dir)
        .output()
        .expect("status check");
    assert!(status_out.status.success());
    let status_str = String::from_utf8_lossy(&status_out.stdout);
    assert!(
        status_str.contains("Top Secret Plan"),
        "Restored note title must match"
    );
    assert!(
        status_str.contains("998877"),
        "Restored note content must match"
    );

    // 8. Pull with WRONG password must fail
    let bad_pull = run_cargo_stdin(
        &["run", "--quiet", "--", "--pull"],
        b"wrong_password\n",
    );
    let bad_json: serde_json::Value =
        serde_json::from_slice(&bad_pull.stdout).expect("parse bad pull");
    assert_eq!(bad_json["success"], false, "Wrong password pull must fail");

    let _ = fs::remove_dir_all(&base_dir);
}

#[test]
fn test_security_sensitive_payloads_in_argv_strictly_rejected() {
    let test_dir = PathBuf::from("/tmp/test_omanotes_argv_rejection");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test dir");

    let prohibited_cases = [
        vec!["--set-e2ee", "mySecretMasterPassword!"],
        vec!["--sync", "mySecretPassword!"],
        vec!["--pull", "mySecretPassword!"],
        vec!["--add", "Secret Note Title", "Secret Note Body"],
        vec!["--edit", "123", "Secret Title", "Secret Body"],
        vec!["--add-check-item", "123", "Secret item"],
        vec!["--set-cloud", "git", "", "/path/to/repo", "true"],
    ];

    for args in prohibited_cases {
        let mut full_args = vec!["run", "--quiet", "--"];
        full_args.extend(&args);

        let out = Command::new("cargo")
            .args(&full_args)
            .env("OMANOTES_DATA_DIR", &test_dir)
            .env("OMANOTES_STATE_DIR", &test_dir)
            .output()
            .expect("cargo run");

        assert!(
            !out.status.success(),
            "Sensitive payload in argv must be strictly rejected: {:?}",
            args
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("Security Error") || stderr.contains("prohibited in process arguments"),
            "Stderr must explicitly declare security violation: {}",
            stderr
        );
    }

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_cli_argument_resilience_no_panics() {
    let test_dir = PathBuf::from("/tmp/test_omanotes_cli_resilience");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Test with various truncated arguments to ensure zero index panics
    let test_cases = [
        vec!["--add"],
        vec!["--edit"],
        vec!["--add-check-item"],
        vec!["--set-cloud"],
        vec!["--set-e2ee"],
        vec!["--sync"],
        vec!["--pull"],
        vec!["--delete"],
        vec!["--toggle-pin"],
        vec!["--unknown-action"],
    ];

    for args in test_cases {
        let mut full_args = vec!["run", "--quiet", "--"];
        full_args.extend(args);

        let out = Command::new("cargo")
            .args(&full_args)
            .env("OMANOTES_DATA_DIR", &test_dir)
            .env("OMANOTES_STATE_DIR", &test_dir)
            .output()
            .expect("cargo run");

        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("panicked"),
            "CLI must never panic on arguments: {:?}",
            full_args
        );
    }

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_cloud_pull_oversize_rejected_and_cleaned_up() {
    let base_dir = PathBuf::from("/tmp/test_omanotes_cloud_pull_oversize");
    let _ = fs::remove_dir_all(&base_dir);
    fs::create_dir_all(&base_dir).expect("create base dir");

    let data_dir = base_dir.join("data");
    let state_dir = base_dir.join("state");
    let git_repo = base_dir.join("vault_repo");
    fs::create_dir_all(&data_dir).expect("create data dir");
    fs::create_dir_all(&state_dir).expect("create state dir");
    fs::create_dir_all(&git_repo).expect("create git dir");

    // Create an oversized file > 10 MiB (10 MiB + 1024 bytes) in the git vault
    let oversize_data = vec![0x58u8; 10 * 1024 * 1024 + 1024];
    fs::write(git_repo.join("notes.enc"), &oversize_data).expect("write oversized notes.enc");

    // Configure git provider
    let config_json = format!(
        r#"{{"version":1,"e2ee_enabled":true,"salt":"00112233445566778899aabbccddeeff","cloud":{{"provider":"git","git_remote":"{}","rclone_remote":"","auto_sync":false,"last_synced_at":0,"last_sync_status":"idle","last_sync_msg":""}}}}"#,
        git_repo.display()
    );
    fs::write(state_dir.join("config.json"), config_json.as_bytes()).expect("write config");

    use std::io::Write;

    // Run --pull with password passed securely via stdin
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "--quiet", "--", "--pull"]);
    cmd.env("OMANOTES_DATA_DIR", &data_dir);
    cmd.env("OMANOTES_STATE_DIR", &state_dir);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().expect("spawn cargo");
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"TestPassword123!\n");
    }
    let out = child.wait_with_output().expect("wait for output");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // Should indicate failure and rejection of oversized payload
    assert!(
        combined.contains("rejected") || combined.contains("exceeds") || combined.contains("error"),
        "Should reject oversized cloud notes: {}",
        combined
    );

    // Ensure no .tmp_cloud_restore files linger in data_dir
    if let Ok(entries) = fs::read_dir(&data_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            assert!(
                !name_str.starts_with(".tmp_cloud_restore"),
                "Staging file must be cleaned up on failure: {}",
                name_str
            );
        }
    }

    let _ = fs::remove_dir_all(&base_dir);
}
