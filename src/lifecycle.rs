//! Daemon lifecycle utilities.
//!
//! Helpers for daemonizing processes, managing PID files, and socket cleanup.

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Daemonize the current process.
///
/// This forks the process, detaches from the terminal, and runs in the background.
///
/// # Arguments
/// * `pid_file` - Path to write the daemon's PID (supports `~` expansion)
/// * `working_dir` - Working directory for the daemon (defaults to `/`)
///
/// # Example
///
/// ```rust,no_run
/// use fgp_daemon::lifecycle::daemonize;
///
/// daemonize("~/.fgp/services/my-service/daemon.pid", None)?;
/// // Now running as a daemon
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn daemonize(pid_file: impl AsRef<Path>, working_dir: Option<&Path>) -> Result<()> {
    let pid_path = expand_path(pid_file.as_ref())?;

    // Create parent directory if needed
    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let daemonize = daemonize::Daemonize::new()
        .pid_file(&pid_path)
        .working_directory(working_dir.unwrap_or(Path::new("/")));

    daemonize.start().context("Failed to daemonize process")?;

    Ok(())
}

/// Write a PID file for the current process.
///
/// # Arguments
/// * `pid_file` - Path to write the PID (supports `~` expansion)
pub fn write_pid_file(pid_file: impl AsRef<Path>) -> Result<()> {
    let pid_path = expand_path(pid_file.as_ref())?;

    // Create parent directory if needed
    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let pid = std::process::id();
    let mut file = fs::File::create(&pid_path)?;
    writeln!(file, "{}", pid)?;

    Ok(())
}

/// Read a PID from a PID file.
///
/// Returns `None` if the file doesn't exist or can't be parsed.
pub fn read_pid_file(pid_file: impl AsRef<Path>) -> Option<u32> {
    let pid_path = expand_path(pid_file.as_ref()).ok()?;
    let content = fs::read_to_string(pid_path).ok()?;
    content.trim().parse().ok()
}

/// Check if a process with the given PID is running.
pub fn is_process_running(pid: u32) -> bool {
    // Use kill(pid, 0) to check if process exists without actually sending a signal
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Clean up a socket file if it's stale (no process listening).
///
/// # Arguments
/// * `socket_path` - Path to the socket file
/// * `pid_file` - Optional PID file to check
///
/// # Returns
/// * `Ok(true)` - Socket was stale and removed
/// * `Ok(false)` - Socket is active or doesn't exist
pub fn cleanup_socket(socket_path: impl AsRef<Path>, pid_file: Option<&Path>) -> Result<bool> {
    let socket = expand_path(socket_path.as_ref())?;

    if !socket.exists() {
        return Ok(false);
    }

    // Check PID file if provided
    if let Some(pid_path) = pid_file {
        if let Some(pid) = read_pid_file(pid_path) {
            if is_process_running(pid) {
                return Ok(false); // Process is running
            }
        }
    }

    // Try to connect to the socket
    match std::os::unix::net::UnixStream::connect(&socket) {
        Ok(_) => Ok(false), // Socket is active
        Err(_) => {
            // Socket is stale, remove it
            fs::remove_file(&socket)?;
            if let Some(pid_path) = pid_file {
                let _ = fs::remove_file(expand_path(pid_path)?);
            }
            Ok(true)
        }
    }
}

/// Remove socket and PID files.
pub fn cleanup_files(socket_path: impl AsRef<Path>, pid_file: Option<&Path>) -> Result<()> {
    let socket = expand_path(socket_path.as_ref())?;
    let _ = fs::remove_file(&socket);

    if let Some(pid_path) = pid_file {
        let _ = fs::remove_file(expand_path(pid_path)?);
    }

    Ok(())
}

/// Expand `~` in path to home directory.
fn expand_path(path: &Path) -> Result<PathBuf> {
    let path_str = path.to_string_lossy();
    let expanded = shellexpand::tilde(&path_str);
    Ok(PathBuf::from(expanded.as_ref()))
}

/// Standard socket path for a service.
pub fn service_socket_path(service_name: &str) -> PathBuf {
    let base = shellexpand::tilde("~/.fgp/services");
    PathBuf::from(base.as_ref())
        .join(service_name)
        .join("daemon.sock")
}

/// Standard PID file path for a service.
pub fn service_pid_path(service_name: &str) -> PathBuf {
    let base = shellexpand::tilde("~/.fgp/services");
    PathBuf::from(base.as_ref())
        .join(service_name)
        .join("daemon.pid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_path() {
        let expanded = expand_path(Path::new("~/.fgp/test")).unwrap();
        assert!(!expanded.to_string_lossy().contains('~'));
    }

    #[test]
    fn test_service_paths() {
        let socket = service_socket_path("gmail");
        let pid = service_pid_path("gmail");

        assert!(socket.to_string_lossy().contains("gmail/daemon.sock"));
        assert!(pid.to_string_lossy().contains("gmail/daemon.pid"));
    }
}
