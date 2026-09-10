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

    // 1. Add a note with secret content
    let add_out = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "--add",
            "Top Secret Plan",
            "Launch rocket at midnight with code 998877.",
            "yellow",
            "false",
        ])
        .env("OMANOTES_DATA_DIR", &data_dir)
        .env("OMANOTES_STATE_DIR", &state_dir)
        .output()
        .expect("add note");
    assert!(add_out.status.success());

    // 2. Configure git cloud backup
    let conf_out = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "--set-cloud",
            "git",
            "",
            git_repo.to_str().unwrap(),
            "true",
        ])
        .env("OMANOTES_DATA_DIR", &data_dir)
        .env("OMANOTES_STATE_DIR", &state_dir)
        .output()
        .expect("set cloud");
    assert!(conf_out.status.success());

    // 3. Sync with password
    let password = "UltraSecurePassword#2026";
    let sync_out = Command::new("cargo")
        .args(["run", "--quiet", "--", "--sync", password])
        .env("OMANOTES_DATA_DIR", &data_dir)
        .env("OMANOTES_STATE_DIR", &state_dir)
        .output()
        .expect("sync note");
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

    // 7. Pull and Decrypt
    let pull_out = Command::new("cargo")
        .args(["run", "--quiet", "--", "--pull", password])
        .env("OMANOTES_DATA_DIR", &data_dir)
        .env("OMANOTES_STATE_DIR", &state_dir)
        .output()
        .expect("pull notes");
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
    let bad_pull = Command::new("cargo")
        .args(["run", "--quiet", "--", "--pull", "wrong_password"])
        .env("OMANOTES_DATA_DIR", &data_dir)
        .env("OMANOTES_STATE_DIR", &state_dir)
        .output()
        .expect("bad pull");
    let bad_json: serde_json::Value =
        serde_json::from_slice(&bad_pull.stdout).expect("parse bad pull");
    assert_eq!(bad_json["success"], false, "Wrong password pull must fail");

    let _ = fs::remove_dir_all(&base_dir);
}

#[test]
fn test_cli_argument_resilience_no_panics() {
    let test_dir = PathBuf::from("/tmp/test_omanotes_cli_resilience");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test dir");

    // Test with various truncated arguments to ensure zero index panics
    let test_cases = [
        vec!["--add", "Short Note"],
        vec!["--add", "Short Note", "Some content"],
        vec!["--add", "Short Note", "Some content", "purple"],
        vec!["--edit", "non_existent_id", "New Title"],
        vec!["--set-cloud", "git"],
        vec!["--set-cloud", "rclone", "remote:vault"],
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
