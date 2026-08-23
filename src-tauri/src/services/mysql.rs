use super::{bin_path, hidden_command, kill_tree, posix, Service, ServiceStatus};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

pub const DEFAULT_PORT: u16 = 3306;

/// How long we wait for `mysqladmin shutdown` to land before falling back to
/// `kill_tree`. A healthy dev database flushes in well under a second; the
/// generous ceiling is for the case where InnoDB has a large dirty buffer
/// pool to write out and we'd rather wait than corrupt it.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(20);

#[derive(Debug, Clone)]
pub struct MysqlInstall {
    pub version: String,
    pub dir: PathBuf,
}

pub struct MysqlService {
    installs: Vec<MysqlInstall>,
    active: String,
    runtime_dir: PathBuf,
    port: u16,
    child: Option<Child>,
}

impl MysqlService {
    pub fn new(
        installs: Vec<MysqlInstall>,
        default_version: String,
        runtime_dir: PathBuf,
    ) -> Self {
        // Prefer the requested default, but don't boot pointing at a version
        // that was never downloaded — fall back to whatever is on disk.
        let active = if installs
            .iter()
            .any(|i| i.version == default_version && Self::is_present(i))
        {
            default_version
        } else {
            installs
                .iter()
                .find(|i| Self::is_present(i))
                .map(|i| i.version.clone())
                .unwrap_or(default_version)
        };
        Self {
            installs,
            active,
            runtime_dir,
            port: DEFAULT_PORT,
            child: None,
        }
    }

    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }

    /// Versions with files on disk. The picker used to list every version
    /// the app knows about, so choosing an undownloaded one produced a
    /// service that refused to start — for a while, silently.
    pub fn versions(&self) -> Vec<String> {
        self.installs
            .iter()
            .filter(|i| Self::is_present(i))
            .map(|i| i.version.clone())
            .collect()
    }

    fn is_present(install: &MysqlInstall) -> bool {
        bin_path(&install.dir, "mysqld").exists()
    }

    pub fn active_version(&self) -> String {
        self.active.clone()
    }

    pub fn set_active(&mut self, version: String) -> Result<(), String> {
        if self.child.is_some() {
            return Err("Stop MySQL before switching versions.".into());
        }
        match self.installs.iter().find(|i| i.version == version) {
            None => return Err(format!("Unknown MySQL version: {version}")),
            Some(i) if !Self::is_present(i) => {
                return Err(format!(
                    "MySQL {version} isn't downloaded yet — install it from Settings → Components."
                ))
            }
            Some(_) => {}
        }
        self.active = version;
        Ok(())
    }

    fn active_install(&self) -> &MysqlInstall {
        self.installs
            .iter()
            .find(|i| i.version == self.active)
            .or_else(|| self.installs.first())
            .expect("at least one MySQL install configured")
    }

    /// Each MySQL version gets its own data dir — `mysqld --initialize` of one
    /// version refuses to reuse another's directory, and the binary
    /// tablespace formats are not always backward-compatible.
    fn data_dir(&self) -> PathBuf {
        self.runtime_dir
            .join(format!("mysql-{}", self.active))
            .join("data")
    }

    fn ensure_initialized(&self) -> Result<(), String> {
        let data = self.data_dir();
        if has_contents(&data) {
            return Ok(());
        }
        if let Some(parent) = data.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create runtime dir: {e}"))?;
        }

        let install = self.active_install();
        let mysqld = bin_path(&install.dir, "mysqld");
        let basedir = posix(&install.dir);
        let datadir = posix(&data);

        let output = hidden_command(&mysqld)
            .arg(format!("--basedir={basedir}"))
            .arg(format!("--datadir={datadir}"))
            .arg("--initialize-insecure")
            .output()
            .map_err(|e| format!("failed to run mysqld --initialize-insecure: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("mysqld --initialize-insecure failed: {stderr}"));
        }
        Ok(())
    }

    fn ensure_conf(&self) -> Result<PathBuf, String> {
        let conf_dir = self.runtime_dir.join(format!("mysql-{}", self.active));
        fs::create_dir_all(&conf_dir).map_err(|e| e.to_string())?;
        let conf_path = conf_dir.join("my.cnf");
        let install = self.active_install();
        let conf = build_minimal_conf(&install.dir, &self.data_dir(), self.port);
        fs::write(&conf_path, conf).map_err(|e| e.to_string())?;
        Ok(conf_path)
    }

    /// Ask mysqld to shut itself down over the network, the way `mysqladmin`
    /// does. Returns false when the client isn't on disk or the command
    /// failed — the caller then falls back to `kill_tree`.
    ///
    /// This matters more than it looks: `kill_tree` is `taskkill /F /T` on
    /// Windows and `SIGKILL` elsewhere, i.e. a hard crash. Doing that on
    /// every single stop means InnoDB replays its redo log on every single
    /// start, and a kill landing mid-checkpoint is exactly how a dev
    /// database ends up unopenable.
    fn request_shutdown(&self) -> bool {
        let install = self.active_install();
        let admin = bin_path(&install.dir, "mysqladmin");
        if !admin.exists() {
            return false;
        }
        hidden_command(&admin)
            .args(["--protocol=TCP", "-h", "127.0.0.1", "-P"])
            .arg(self.port.to_string())
            .args(["-u", "root", "shutdown"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Poll `try_wait` until the child is gone or the deadline passes. Returns
/// true when the process exited on its own.
fn wait_for_exit(child: &mut Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(_) => return false,
        }
    }
}

impl Service for MysqlService {
    fn start(&mut self) -> Result<(), String> {
        if self.child.is_some() {
            return Ok(());
        }
        self.ensure_initialized()?;
        let conf = self.ensure_conf()?;
        let install = self.active_install();
        let mysqld = bin_path(&install.dir, "mysqld");
        if !mysqld.exists() {
            return Err(format!("mysqld binary not found at {}", mysqld.display()));
        }

        let log_path = self
            .runtime_dir
            .join(format!("mysql-{}", self.active))
            .join("mysql.log");
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create mysql log dir: {e}"))?;
        }
        let log_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("open mysql log: {e}"))?;
        let log_clone = log_file
            .try_clone()
            .map_err(|e| format!("clone mysql log handle: {e}"))?;

        let child = hidden_command(&mysqld)
            .arg(format!("--defaults-file={}", conf.display()))
            .arg("--console")
            .stdout(log_file)
            .stderr(log_clone)
            .spawn()
            .map_err(|e| format!("failed to spawn mysqld: {e}"))?;
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        if let Some(mut child) = self.child.take() {
            // Clean shutdown first, hard kill only if it doesn't land.
            if self.request_shutdown() && wait_for_exit(&mut child, SHUTDOWN_GRACE) {
                return Ok(());
            }
            kill_tree(&mut child);
        }
        Ok(())
    }

    fn status(&mut self) -> ServiceStatus {
        match self.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(None) => ServiceStatus::Running { pid: child.id() },
                Ok(Some(_)) => {
                    self.child = None;
                    ServiceStatus::Stopped
                }
                Err(e) => ServiceStatus::Error {
                    message: e.to_string(),
                },
            },
            None => ServiceStatus::Stopped,
        }
    }
}

fn has_contents(p: &Path) -> bool {
    fs::read_dir(p)
        .map(|mut it| it.next().is_some())
        .unwrap_or(false)
}

fn build_minimal_conf(mysql_dir: &Path, data_dir: &Path, port: u16) -> String {
    let basedir = posix(mysql_dir);
    let datadir = posix(data_dir);
    format!(
        "# Generated by Lamp Bench. Do not edit by hand.\n\
         [mysqld]\n\
         basedir = \"{basedir}\"\n\
         datadir = \"{datadir}\"\n\
         port = {port}\n\
         bind-address = 127.0.0.1\n"
    )
}
