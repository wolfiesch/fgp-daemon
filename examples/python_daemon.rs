//! Example: Running an FGP daemon with a Python module.
//!
//! This example demonstrates loading a Python module and serving it via FgpServer.
//!
//! # Prerequisites
//!
//! Build with the `python` feature:
//! ```bash
//! cargo build --features python --example python_daemon
//! ```
//!
//! # Usage
//!
//! Start the daemon:
//! ```bash
//! cargo run --features python --example python_daemon
//! ```
//!
//! Test with the FGP CLI:
//! ```bash
//! fgp call echo.ping
//! fgp call echo.reverse --text "hello"
//! fgp call echo.add --a 5 --b 3
//! ```

use anyhow::Result;
#[cfg(feature = "python")]
use fgp_daemon::FgpService;

#[cfg(feature = "python")]
fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("fgp_daemon=debug")
        .init();

    // Get the path to the Python module
    let module_path = std::env::current_dir()?
        .join("examples")
        .join("python_module.py");

    println!("Loading Python module from: {}", module_path.display());

    // Load the Python module
    let module = fgp_daemon::PythonModule::load(&module_path, "EchoModule")?;

    println!("Loaded module: {} v{}", module.name(), module.version());

    // Create and run the server
    let socket_path = "~/.fgp/services/echo/daemon.sock";
    println!("Starting daemon at: {}", socket_path);

    let server = fgp_daemon::FgpServer::new(module, socket_path)?;
    server.serve()?;

    Ok(())
}

#[cfg(not(feature = "python"))]
fn main() {
    eprintln!("This example requires the 'python' feature.");
    eprintln!("Run with: cargo run --features python --example python_daemon");
    std::process::exit(1);
}
