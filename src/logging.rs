//! Daemon logging utilities.
//!
//! Provides standardized file logging for FGP daemons.
//!
//! # Example
//!
//! ```rust,no_run
//! use fgp_daemon::logging::init_logging;
//!
//! fn main() -> anyhow::Result<()> {
//!     init_logging("my-service")?;
//!
//!     tracing::info!("Daemon started");
//!     // ... daemon code ...
//!     Ok(())
//! }
//! ```

use anyhow::{Context, Result};
use std::fs::{self, File};
use std::path::PathBuf;

/// Get the standard log directory for a service.
pub fn log_dir(service_name: &str) -> PathBuf {
    crate::lifecycle::fgp_services_dir()
        .join(service_name)
        .join("logs")
}

/// Get the standard log file path for a service.
pub fn log_file_path(service_name: &str) -> PathBuf {
    log_dir(service_name).join("daemon.log")
}

/// Initialize file logging for a daemon.
///
/// Sets up a tracing subscriber that writes JSON-formatted logs to:
/// `~/.fgp/services/<service_name>/logs/daemon.log`
///
/// # Arguments
/// * `service_name` - The name of the service (used for log directory)
///
/// # Example
/// ```rust,no_run
/// use fgp_daemon::logging::init_logging;
/// init_logging("gmail")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn init_logging(service_name: &str) -> Result<()> {
    let log_dir = log_dir(service_name);
    fs::create_dir_all(&log_dir).context("Failed to create log directory")?;

    let log_path = log_dir.join("daemon.log");
    let file = File::create(&log_path).context("Failed to create log file")?;

    // Use tracing_subscriber to write to file
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = tracing_subscriber::registry().with(filter).with(
        fmt::layer()
            .with_writer(file)
            .with_ansi(false)
            .with_target(true)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false),
    );

    tracing::subscriber::set_global_default(subscriber).context("Failed to set subscriber")?;

    Ok(())
}

/// Initialize file logging with rotation (daily).
///
/// Similar to `init_logging` but rotates log files daily.
/// Older logs are kept as `daemon.log.YYYY-MM-DD`.
#[cfg(feature = "log-rotation")]
pub fn init_logging_with_rotation(service_name: &str) -> Result<()> {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};

    let log_dir = log_dir(service_name);
    fs::create_dir_all(&log_dir).context("Failed to create log directory")?;

    let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "daemon.log");

    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = tracing_subscriber::registry().with(filter).with(
        fmt::layer()
            .with_writer(file_appender)
            .with_ansi(false)
            .with_target(true),
    );

    tracing::subscriber::set_global_default(subscriber).context("Failed to set subscriber")?;

    Ok(())
}
