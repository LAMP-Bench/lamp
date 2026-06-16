//! Service supervisors for bundled binaries (Apache, Nginx, MySQL, Redis,
//! Memcached, MailHog).

pub mod apache;
pub mod mailhog;
pub mod mysql;
pub mod nginx;
pub mod redis;

use serde::Serialize;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

/// Returns `<root>/bin/<name>` with `.exe` appended on Windows.
pub fn bin_path(root: &Path, name: &str) -> PathBuf {
    root.join("bin")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

/// Build a `Command` that won't pop a console window on Windows. Every
/// long-running service (httpd, mysqld, nginx, php-cgi, redis, mailhog) and
/// every one-shot helper (taskkill, PowerShell hosts/cert edits, php -l,
/// composer, git) must be created through this so the user never sees a
/// flashing black CMD on screen. No-op on non-Windows.
pub fn hidden_command<S: AsRef<OsStr>>(program: S) -> Command {
    // `mut` is only needed on Windows where we mutate `cmd` to set the
    // creation_flags below. Non-Windows targets compile out that block,
    // leaving the binding unmutated — `allow(unused_mut)` keeps both
    // platforms warning-free without duplicating the let-binding.
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW — keep the spawned process detached from any
        // console. Children inherit this too, so php-cgi pools stay quiet.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
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
        let _ = hidden_command("taskkill")
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
