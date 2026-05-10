//! Service supervisors for bundled binaries (Apache, Nginx, MySQL, Redis,
//! Memcached, MailHog).

pub mod apache;
pub mod mysql;
pub mod nginx;
pub mod redis;

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Child;

/// Returns `<root>/bin/<name>` with `.exe` appended on Windows.
pub fn bin_path(root: &Path, name: &str) -> PathBuf {
    root.join("bin")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

/// Apache, MySQL and friends want forward slashes in their config files even
/// on Windows. This converts a path to that form.
pub fn posix(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Reliably kill a spawned child *and its descendants*.
///
/// `Child::kill()` on Windows calls `TerminateProcess`, which only kills the
/// targeted PID. Apache spawns a worker (mpm_winnt) under its parent, and
/// MySQL similarly forks helper threads — leaving the worker alive holds the
/// listening port and breaks the next `start()`. On Windows we shell out to
/// `taskkill /F /T` which walks the whole tree.
pub fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args([
                "/F",
                "/T",
                "/PID",
                &child.id().to_string(),
            ])
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServiceStatus {
    Stopped,
    Running { pid: u32 },
    Error { message: String },
}

pub trait Service: Send {
    fn start(&mut self) -> Result<(), String>;
    fn stop(&mut self) -> Result<(), String>;
    fn status(&mut self) -> ServiceStatus;
}
