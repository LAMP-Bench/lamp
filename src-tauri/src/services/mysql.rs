use super::{bin_path, kill_tree, posix, Service, ServiceStatus};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

pub const DEFAULT_PORT: u16 = 3306;

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
        Self {
            installs,
            active: default_version,
            runtime_dir,
            port: DEFAULT_PORT,
            child: None,
        }
    }

    pub fn versions(&self) -> Vec<String> {
        self.installs.iter().map(|i| i.version.clone()).collect()
    }

    pub fn active_version(&self) -> String {
        self.active.clone()
    }

    pub fn set_active(&mut self, version: String) -> Result<(), String> {
        if self.child.is_some() {
            return Err("Stop MySQL before switching versions.".into());
        }
        if !self.installs.iter().any(|i| i.version == version) {
            return Err(format!("Unknown MySQL version: {version}"));
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

        let output = Command::new(&mysqld)
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

        let child = Command::new(&mysqld)
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
