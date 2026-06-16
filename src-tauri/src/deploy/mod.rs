//! Remote deploy via FTP. Recursively uploads a local folder to a remote
//! path. SFTP is the Phase 7.x follow-up (needs russh-sftp + async runtime).
//!
//! Plain FTP is intentionally the only transport for now — sftp would
//! double the dependency surface and most casual users on shared hosting
//! still only get FTPS at best, which `suppaftp` handles via the same API
//! when the feature is enabled.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use suppaftp::FtpStream;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DeployReport {
    pub files_uploaded: usize,
    pub bytes_uploaded: u64,
    pub errors: Vec<String>,
}

pub fn ftp_upload_folder(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    remote_dir: &str,
    local_dir: &Path,
) -> Result<DeployReport, String> {
    if !local_dir.is_dir() {
        return Err(format!("local folder not found: {}", local_dir.display()));
    }
    let addr = format!("{host}:{port}");
    let mut ftp = FtpStream::connect(&addr).map_err(|e| format!("connect {addr}: {e}"))?;
    ftp.login(user, password)
        .map_err(|e| format!("login: {e}"))?;
    // Binary transfer mode — text mode mangles non-text files.
    ftp.transfer_type(suppaftp::types::FileType::Binary)
        .map_err(|e| format!("set binary: {e}"))?;

    let mut report = DeployReport::default();
    let remote_root = normalise_remote(remote_dir);
    ensure_remote_dir(&mut ftp, &remote_root, &mut report);
    upload_recursive(&mut ftp, local_dir, &remote_root, &mut report);

    let _ = ftp.quit();
    Ok(report)
}

fn upload_recursive(
    ftp: &mut FtpStream,
    local: &Path,
    remote: &str,
    report: &mut DeployReport,
) {
    let entries = match fs::read_dir(local) {
        Ok(e) => e,
        Err(e) => {
            report
                .errors
                .push(format!("read_dir {}: {e}", local.display()));
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let remote_path = format!("{remote}/{name_str}");
        if path.is_dir() {
            ensure_remote_dir(ftp, &remote_path, report);
            upload_recursive(ftp, &path, &remote_path, report);
        } else {
            match upload_file(ftp, &path, &remote_path) {
                Ok(bytes) => {
                    report.files_uploaded += 1;
                    report.bytes_uploaded += bytes;
                }
                Err(e) => report.errors.push(format!("{}: {e}", path.display())),
            }
        }
    }
}

fn upload_file(ftp: &mut FtpStream, local: &Path, remote_path: &str) -> Result<u64, String> {
    let bytes = fs::read(local).map_err(|e| format!("read local: {e}"))?;
    let size = bytes.len() as u64;
    let mut cursor = std::io::Cursor::new(bytes);
    ftp.put_file(remote_path, &mut cursor)
        .map_err(|e| format!("put_file: {e}"))?;
    Ok(size)
}

fn ensure_remote_dir(ftp: &mut FtpStream, remote: &str, report: &mut DeployReport) {
    // Walk the path components and mkdir each. FTP server returns an error
    // when the directory already exists — we swallow it because there's no
    // portable way to distinguish "exists" from a real failure across
    // servers, and the worst case is a harmless retry on the next file.
    let mut acc = String::new();
    for part in remote.split('/').filter(|p| !p.is_empty()) {
        acc.push('/');
        acc.push_str(part);
        if let Err(e) = ftp.mkdir(&acc) {
            // Surface only the FIRST mkdir failure per branch; subsequent
            // ones are usually "already exists" follow-ups.
            let msg = e.to_string();
            if !msg.contains("550") {
                report.errors.push(format!("mkdir {acc}: {msg}"));
            }
        }
    }
}

fn normalise_remote(p: &str) -> String {
    let trimmed = p.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        ".".to_string()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

#[allow(dead_code)]
fn _unused_path_used() -> PathBuf {
    PathBuf::new()
}
