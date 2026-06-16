use super::{hidden_command, kill_tree, Service, ServiceStatus};
use std::fs;
use std::path::PathBuf;
use std::process::Child;

pub const SMTP_PORT: u16 = 1025;
pub const UI_PORT: u16 = 8025;

pub struct MailhogService {
    mailhog_dir: PathBuf,
    runtime_dir: PathBuf,
    child: Option<Child>,
}

impl MailhogService {
    pub fn new(mailhog_dir: PathBuf, runtime_dir: PathBuf) -> Self {
        Self {
            mailhog_dir,
            runtime_dir,
            child: None,
        }
    }
}

impl Service for MailhogService {
    fn start(&mut self) -> Result<(), String> {
        if self.child.is_some() {
            return Ok(());
        }
        // MailHog ships as a single executable on Windows. The fetched file
        // lives at `resources/mailhog/MailHog.exe` (see binaries.json).
        let bin = self
            .mailhog_dir
            .join(format!("MailHog{}", std::env::consts::EXE_SUFFIX));
        if !bin.exists() {
            return Err(format!(
                "MailHog binary not found at {}. Install it from the sidebar first.",
                bin.display()
            ));
        }
        let messages_dir = self.runtime_dir.join("mailhog").join("messages");
        fs::create_dir_all(&messages_dir).map_err(|e| e.to_string())?;
        let log_path = self.runtime_dir.join("mailhog").join("mailhog.log");
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("open mailhog log: {e}"))?;
        let log_clone = log.try_clone().map_err(|e| e.to_string())?;

        let child = hidden_command(&bin)
            .arg("-smtp-bind-addr")
            .arg(format!("127.0.0.1:{SMTP_PORT}"))
            .arg("-ui-bind-addr")
            .arg(format!("127.0.0.1:{UI_PORT}"))
            .arg("-api-bind-addr")
            .arg(format!("127.0.0.1:{UI_PORT}"))
            .arg("-storage")
            .arg("maildir")
            .arg("-maildir-path")
            .arg(&messages_dir)
            .stdout(log)
            .stderr(log_clone)
            .spawn()
            .map_err(|e| format!("failed to spawn MailHog: {e}"))?;
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
