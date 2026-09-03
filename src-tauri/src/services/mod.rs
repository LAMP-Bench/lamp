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

/// Like `hidden_command`, but for the long-running services we later have to
/// shut down as a unit (httpd, mysqld, nginx, php-cgi, redis, mailhog).
///
/// On Unix the child gets its own process group via `setsid`, so `kill_tree`
/// can signal the whole group. Without it, a signal reaches only the parent
/// PID, Apache's and nginx's forked workers survive, keep the listening
/// socket open, and the next `start()` fails with "address already in use".
/// That is the same problem `taskkill /T` solves on Windows.
pub fn service_command<S: AsRef<OsStr>>(program: S) -> Command {
    #[allow(unused_mut)]
    let mut cmd = hidden_command(program);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            // Runs in the forked child before exec; `setsid` is
            // async-signal-safe, which is the bar for pre_exec closures.
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    cmd
}

/// Directories worth searching for a service binary beyond `PATH`.
///
/// Daemons live in `sbin`, which is not on a desktop user's `PATH` on most
/// distros — `apache2`, `mysqld` and `redis-server` are all typically there.
#[cfg(not(windows))]
const EXTRA_BIN_DIRS: [&str; 4] = ["/usr/local/sbin", "/usr/sbin", "/sbin", "/opt/homebrew/bin"];

/// First directory in `dirs` that holds an executable called `name`.
///
/// Split out from `which` so the lookup itself is testable without depending
/// on whatever happens to be installed on the machine running the tests.
#[cfg(not(windows))]
fn find_in_dirs(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|d| d.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(not(windows))]
fn which(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    dirs.extend(EXTRA_BIN_DIRS.iter().map(PathBuf::from));
    find_in_dirs(&dirs, name)
}

/// Locate a service executable: the bundled copy first, then whatever the
/// system provides.
///
/// This exists because upstream simply does not publish prebuilt Unix
/// binaries for most of what Lamp Bench supervises — Apache, nginx, PHP and
/// Redis ship source tarballs only, so on Linux and macOS the package manager
/// is the realistic source. Windows never falls through: there the bundled
/// copy is the only supported one, and silently picking up some unrelated
/// `nginx.exe` from `PATH` would be worse than a clear error.
///
/// Note this resolves the *binary* only. A system Apache or MySQL also needs
/// a config written for its own layout, so those two still require the
/// bundled install; Redis and MailHog are driven entirely by arguments and a
/// generated config with absolute paths, and work either way.
pub fn resolve_binary(bundled: PathBuf, _system_names: &[&str]) -> Option<PathBuf> {
    if bundled.is_file() {
        return Some(bundled);
    }
    #[cfg(not(windows))]
    {
        for name in _system_names {
            if let Some(found) = which(name) {
                return Some(found);
            }
        }
    }
    None
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
/// `taskkill /F /T` which walks the whole tree; on Unix we signal the process
/// group that `service_command` put the child in.
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
    #[cfg(unix)]
    {
        // Signal the whole process group (negative pid) rather than just the
        // leader, so forked workers go down with their parent.
        //
        // SIGTERM first, with a grace period. mysqld flushes InnoDB and
        // releases its lock file on SIGTERM; SIGKILL leaves the data dir
        // dirty and the next start pays for crash recovery, or refuses
        // outright. Only what is still alive after the grace period is
        // killed hard.
        let pid = child.id() as i32;
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => {}
                // Unwaitable (already reaped elsewhere). Nothing useful left
                // to do here.
                Err(_) => break,
            }
            if std::time::Instant::now() >= deadline {
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    #[cfg(not(any(windows, unix)))]
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
