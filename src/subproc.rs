use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

const POLLIN: i16 = 0x0001;
const POLLHUP: i16 = 0x0010;
const POLLERR: i16 = 0x0008;

const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const O_NONBLOCK: i32 = 2048; // 0o4000 on Linux

extern "C" {
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
}

/// Strictly reaps a process group:
/// 1. Sends SIGTERM (15) to -pid
/// 2. Waits 5-10ms
/// 3. Sends SIGKILL (9) to -pid to eliminate any surviving processes
/// 4. Waits on direct child to prevent zombies
pub fn reap_process_group(child: &mut std::process::Child, pid: i32) {
    if pid > 1 {
        // SAFETY: kill(-pid, 15) sends SIGTERM to the process group with valid PID.
        unsafe {
            kill(-pid, 15);
        }
        std::thread::sleep(Duration::from_millis(5));
        // SAFETY: kill(-pid, 9) sends SIGKILL to the process group with valid PID.
        unsafe {
            kill(-pid, 9);
        }
    }
    let _ = child.wait();
}

pub struct ProcessGroupGuard<'a> {
    pub child: &'a mut std::process::Child,
    pub pid: i32,
    pub active: bool,
}

impl<'a> Drop for ProcessGroupGuard<'a> {
    fn drop(&mut self) {
        if self.active {
            reap_process_group(self.child, self.pid);
        }
    }
}

/// Executes an isolated subprocess with:
/// - cmd.process_group(0)
/// - Non-blocking I/O via fcntl O_NONBLOCK
/// - Bounded polling loop via POSIX poll()
/// - Monotonic deadline enforcement
/// - Maximum output buffer caps
/// - Clean environment with whitelisted variables
pub fn run_cmd_bounded(
    cmd_path: &str,
    args: &[&str],
    extra_envs: &[(&str, &str)],
    deadline: Instant,
    max_output_bytes: usize,
) -> Option<Vec<u8>> {
    if Instant::now() >= deadline {
        return None;
    }

    let mut cmd = Command::new(cmd_path);
    cmd.args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    for (k, v) in extra_envs {
        cmd.env(k, v);
    }

    cmd.process_group(0);

    let mut child = cmd.spawn().ok()?;
    let pid = child.id() as i32;
    let mut stdout = child.stdout.take()?;
    let raw_fd = stdout.as_raw_fd();

    // SAFETY: F_GETFL and F_SETFL are standard POSIX fcntl operations on a valid pipe file descriptor.
    unsafe {
        let flags = fcntl(raw_fd, F_GETFL, 0);
        if flags >= 0 {
            let _ = fcntl(raw_fd, F_SETFL, flags | O_NONBLOCK);
        }
    }

    let mut guard = ProcessGroupGuard {
        child: &mut child,
        pid,
        active: true,
    };

    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut stdout_closed = false;
    let mut direct_child_exited = false;
    let mut direct_child_success = false;
    let mut overrun = false;
    let mut failed = false;

    loop {
        if Instant::now() >= deadline {
            break;
        }

        if !direct_child_exited {
            match guard.child.try_wait() {
                Ok(Some(status)) => {
                    direct_child_exited = true;
                    direct_child_success = status.success();
                }
                Ok(None) => {}
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }

        if direct_child_exited && stdout_closed {
            break;
        }

        if stdout_closed {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5).min(remaining));
            continue;
        }

        let now = Instant::now();
        let remaining_ms =
            (deadline.saturating_duration_since(now).as_millis().min(50) as i32).max(1);
        let mut pfd = PollFd {
            fd: raw_fd,
            events: POLLIN | POLLHUP | POLLERR,
            revents: 0,
        };

        // SAFETY: poll() is called with a valid pointer to PollFd and 1 descriptor.
        let ret = unsafe { poll(&mut pfd, 1, remaining_ms) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            failed = true;
            break;
        } else if ret == 0 {
            continue;
        }

        if pfd.revents & POLLIN != 0 {
            loop {
                match stdout.read(&mut chunk) {
                    Ok(0) => {
                        stdout_closed = true;
                        break;
                    }
                    Ok(n) => {
                        if buffer.len() + n > max_output_bytes {
                            let take = max_output_bytes.saturating_sub(buffer.len());
                            buffer.extend_from_slice(&chunk[..take]);
                            overrun = true;
                            break;
                        }
                        buffer.extend_from_slice(&chunk[..n]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                        continue;
                    }
                    Err(_) => {
                        failed = true;
                        break;
                    }
                }
            }
            if overrun || failed {
                break;
            }
        }

        if pfd.revents & (POLLHUP | POLLERR) != 0 {
            loop {
                match stdout.read(&mut chunk) {
                    Ok(0) => {
                        break;
                    }
                    Ok(n) => {
                        if buffer.len() + n > max_output_bytes {
                            let take = max_output_bytes.saturating_sub(buffer.len());
                            buffer.extend_from_slice(&chunk[..take]);
                            overrun = true;
                            break;
                        }
                        buffer.extend_from_slice(&chunk[..n]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                        continue;
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
            stdout_closed = true;
            if overrun {
                break;
            }
        }
    }

    if overrun || failed || !direct_child_success || Instant::now() >= deadline {
        return None;
    }

    guard.active = false;
    reap_process_group(guard.child, guard.pid);

    Some(buffer)
}

/// Streams subprocess stdout directly to an open file descriptor, enforcing a hard byte limit.
/// If max_bytes is exceeded or deadline is reached, the subprocess process group
/// is terminated immediately with SIGTERM/SIGKILL, and an error is returned.
pub fn run_cmd_stream_to_file(
    cmd_path: &str,
    args: &[&str],
    extra_envs: &[(&str, &str)],
    target_file: &mut std::fs::File,
    deadline: Instant,
    max_bytes: usize,
) -> Result<usize, String> {
    use std::io::Write;

    if Instant::now() >= deadline {
        return Err("Execution deadline already expired".to_string());
    }

    let mut cmd = Command::new(cmd_path);
    cmd.args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    for (k, v) in extra_envs {
        cmd.env(k, v);
    }

    cmd.process_group(0);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", cmd_path, e))?;
    let pid = child.id() as i32;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to take child stdout pipe".to_string())?;
    let raw_fd = stdout.as_raw_fd();

    // Set O_NONBLOCK on pipe
    unsafe {
        let flags = fcntl(raw_fd, F_GETFL, 0);
        if flags >= 0 {
            let _ = fcntl(raw_fd, F_SETFL, flags | O_NONBLOCK);
        }
    }

    let mut guard = ProcessGroupGuard {
        child: &mut child,
        pid,
        active: true,
    };

    let mut total_written = 0usize;
    let mut chunk = [0u8; 8192];
    let mut stdout_closed = false;
    let mut direct_child_exited = false;
    let mut direct_child_success = false;

    loop {
        if Instant::now() >= deadline {
            return Err("Subprocess exceeded monotonic deadline".to_string());
        }

        if !direct_child_exited {
            match guard.child.try_wait() {
                Ok(Some(status)) => {
                    direct_child_exited = true;
                    direct_child_success = status.success();
                }
                Ok(None) => {}
                Err(e) => {
                    return Err(format!("Child wait error: {}", e));
                }
            }
        }

        if direct_child_exited && stdout_closed {
            break;
        }

        if stdout_closed {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("Timeout waiting for subprocess exit".to_string());
            }
            std::thread::sleep(Duration::from_millis(5).min(remaining));
            continue;
        }

        let now = Instant::now();
        let remaining_ms =
            (deadline.saturating_duration_since(now).as_millis().min(50) as i32).max(1);
        let mut pfd = PollFd {
            fd: raw_fd,
            events: POLLIN | POLLHUP | POLLERR,
            revents: 0,
        };

        let ret = unsafe { poll(&mut pfd, 1, remaining_ms) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("Poll error: {}", err));
        } else if ret == 0 {
            continue;
        }

        if pfd.revents & POLLIN != 0 {
            loop {
                match stdout.read(&mut chunk) {
                    Ok(0) => {
                        stdout_closed = true;
                        break;
                    }
                    Ok(n) => {
                        if total_written + n > max_bytes {
                            // Ceiling exceeded! Guard drop will kill rclone process group!
                            return Err(format!(
                                "Transfer exceeded maximum byte limit ({} bytes)",
                                max_bytes
                            ));
                        }
                        target_file
                            .write_all(&chunk[..n])
                            .map_err(|e| format!("Failed to write to staging file: {}", e))?;
                        total_written += n;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                        continue;
                    }
                    Err(e) => {
                        return Err(format!("Read pipe error: {}", e));
                    }
                }
            }
        }

        if pfd.revents & (POLLHUP | POLLERR) != 0 {
            loop {
                match stdout.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if total_written + n > max_bytes {
                            return Err(format!(
                                "Transfer exceeded maximum byte limit ({} bytes)",
                                max_bytes
                            ));
                        }
                        target_file
                            .write_all(&chunk[..n])
                            .map_err(|e| format!("Failed to write to staging file: {}", e))?;
                        total_written += n;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            stdout_closed = true;
        }
    }

    if !direct_child_success {
        return Err("Subprocess exited with failure status".to_string());
    }

    target_file
        .sync_all()
        .map_err(|e| format!("Failed to fsync staging file: {}", e))?;

    guard.active = false;
    Ok(total_written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_group_deadline_enforcement() {
        let deadline = Instant::now() + Duration::from_millis(100);
        let out = run_cmd_bounded("/bin/sleep", &["5"], &[], deadline, 1024);
        assert!(out.is_none());
    }

    #[test]
    fn test_process_group_buffer_cap_overrun() {
        let deadline = Instant::now() + Duration::from_millis(1000);
        let out = run_cmd_bounded("/bin/sh", &["-c", "yes | head -n 10000"], &[], deadline, 64);
        assert!(out.is_none());
    }
}
