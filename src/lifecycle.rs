//! Daemon lifecycle utilities.
//!
//! Helpers for daemonizing processes, managing PID files, socket cleanup,
//! and on-demand service starting.

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

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

/// Validate that an entrypoint is safe to execute.
///
/// Checks:
/// - File has executable permission
/// - File is owned by current user or root (not world-writable)
fn validate_entrypoint(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path)
        .with_context(|| format!("Cannot read entrypoint metadata: {}", path.display()))?;

    let permissions = metadata.permissions();
    let mode = permissions.mode();

    // Check if file is executable (user, group, or other)
    if mode & 0o111 == 0 {
        bail!(
            "Entrypoint is not executable: {}. Run: chmod +x {}",
            path.display(),
            path.display()
        );
    }

    // Security check: warn if world-writable (but don't block)
    if mode & 0o002 != 0 {
        tracing::warn!(
            "Security warning: entrypoint {} is world-writable. Consider: chmod o-w {}",
            path.display(),
            path.display()
        );
    }

    Ok(())
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

/// Get the FGP services base directory.
pub fn fgp_services_dir() -> PathBuf {
    let base = shellexpand::tilde("~/.fgp/services");
    PathBuf::from(base.as_ref())
}

/// Start a daemon service on-demand.
///
/// This function:
/// 1. Reads the service manifest from `~/.fgp/services/{service}/manifest.json`
/// 2. Spawns the daemon entrypoint process
/// 3. Waits for the socket to appear (with timeout)
///
/// # Arguments
/// * `service_name` - Name of the service to start (e.g., "gmail", "browser")
///
/// # Example
///
/// ```rust,no_run
/// use fgp_daemon::lifecycle::start_service;
///
/// // Start the gmail daemon
/// start_service("gmail")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn start_service(service_name: &str) -> Result<()> {
    start_service_with_timeout(service_name, Duration::from_secs(5))
}

/// Start a daemon service with a custom timeout.
///
/// # Arguments
/// * `service_name` - Name of the service to start
/// * `timeout` - Maximum time to wait for socket to appear
pub fn start_service_with_timeout(service_name: &str, timeout: Duration) -> Result<()> {
    let service_dir = fgp_services_dir().join(service_name);

    // Check if service is installed
    let manifest_path = service_dir.join("manifest.json");
    if !manifest_path.exists() {
        bail!(
            "Service '{}' is not installed. Run 'fgp install <path>' first.",
            service_name
        );
    }

    // Check if already running
    let socket_path = service_socket_path(service_name);
    if socket_path.exists() {
        // Try to connect to see if it's actually running
        if std::os::unix::net::UnixStream::connect(&socket_path).is_ok() {
            tracing::debug!("Service '{}' is already running", service_name);
            return Ok(());
        } else {
            // Stale socket, remove it
            let _ = fs::remove_file(&socket_path);
        }
    }

    // Read manifest to get entrypoint
    let manifest_content = fs::read_to_string(&manifest_path)
        .context("Failed to read manifest.json")?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content)
        .context("Failed to parse manifest.json")?;

    let entrypoint = manifest["daemon"]["entrypoint"]
        .as_str()
        .context("manifest.json missing daemon.entrypoint")?;

    let entrypoint_path = service_dir.join(entrypoint);
    if !entrypoint_path.exists() {
        bail!("Daemon entrypoint not found: {}", entrypoint_path.display());
    }

    // Security: Validate entrypoint is executable
    validate_entrypoint(&entrypoint_path)?;

    tracing::info!("Starting service '{}'...", service_name);

    // Start as background process
    let _child = Command::new(&entrypoint_path)
        .current_dir(&service_dir)
        .spawn()
        .context("Failed to start daemon")?;

    // Wait for socket to appear with timeout
    let start = Instant::now();
    while start.elapsed() < timeout {
        if socket_path.exists() {
            // Verify we can connect
            if std::os::unix::net::UnixStream::connect(&socket_path).is_ok() {
                tracing::info!("Service '{}' started successfully", service_name);
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    bail!(
        "Service '{}' started but socket not ready within {:?}",
        service_name,
        timeout
    )
}

/// Stop a daemon service.
///
/// Sends SIGTERM to the daemon process and cleans up socket/PID files.
///
/// # Arguments
/// * `service_name` - Name of the service to stop
pub fn stop_service(service_name: &str) -> Result<()> {
    let socket_path = service_socket_path(service_name);
    let pid_path = service_pid_path(service_name);

    if socket_path.exists() {
        if let Ok(client) = crate::client::FgpClient::new(&socket_path) {
            if let Ok(response) = client.stop() {
                if response.ok {
                    return Ok(());
                }
            }
        }
    }

    // Check if PID file exists
    if let Some(pid) = read_pid_file(&pid_path) {
        if is_process_running(pid) {
            tracing::info!("Stopping service '{}' (PID: {})...", service_name, pid);

            let expected = read_entrypoint_name(service_name)?;
            if !pid_matches_process(pid, expected.as_deref()) {
                bail!(
                    "Refusing to stop PID {}: process does not match expected entrypoint '{}'",
                    pid,
                    expected.unwrap_or_else(|| "unknown".to_string())
                );
            }

            // Send SIGTERM
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }

            // Wait a moment for graceful shutdown
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    // Clean up files
    let _ = fs::remove_file(&socket_path);
    let _ = fs::remove_file(&pid_path);

    tracing::info!("Service '{}' stopped", service_name);
    Ok(())
}

fn read_entrypoint_name(service_name: &str) -> Result<Option<String>> {
    let manifest_path = fgp_services_dir().join(service_name).join("manifest.json");
    if !manifest_path.exists() {
        return Ok(None);
    }

    let manifest_content = fs::read_to_string(&manifest_path)
        .context("Failed to read manifest.json")?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content)
        .context("Failed to parse manifest.json")?;

    let entrypoint = manifest["daemon"]["entrypoint"]
        .as_str()
        .map(|s| s.to_string());

    Ok(entrypoint.and_then(|p| {
        Path::new(&p)
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
    }))
}

fn pid_matches_process(pid: u32, expected_name: Option<&str>) -> bool {
    let Some(expected_name) = expected_name else {
        return false;
    };

    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let command = String::from_utf8_lossy(&output.stdout);
            command.trim().contains(expected_name)
        }
        _ => false,
    }
}

/// Check if a service is currently running.
///
/// # Arguments
/// * `service_name` - Name of the service to check
pub fn is_service_running(service_name: &str) -> bool {
    let socket_path = service_socket_path(service_name);
    if socket_path.exists() {
        std::os::unix::net::UnixStream::connect(&socket_path).is_ok()
    } else {
        false
    }
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
