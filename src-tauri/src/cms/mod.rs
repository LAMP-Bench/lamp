//! One-click CMS installers.
//!
//! Each CMS gets a pinned upstream zip in `resources/<cms>/`. Installing
//! materialises a fresh copy in the user's project dir, creates an empty
//! MySQL database, and registers the host. WordPress also generates a
//! `wp-config.php` with credentials baked in; the other CMSes complete
//! their setup in their own web installer wizard.

pub mod wordpress;

use crate::services::{bin_path, hidden_command};
use std::fs;
use std::path::{Path, PathBuf};

/// Build a clean MySQL identifier from a free-form site name. Non-alphanumeric
/// characters become underscores. `prefix` separates CMS namespaces so a
/// "blog" Joomla site can coexist with a "blog" Drupal site.
pub fn sanitize_db_name(prefix: &str, site_name: &str) -> String {
    let cleaned: String = site_name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("{prefix}_{}", cleaned.trim_matches('_').to_lowercase())
}

pub fn create_database(mysql_dir: &Path, port: u16, db_name: &str) -> Result<(), String> {
    let mysql = bin_path(mysql_dir, "mysql");
    if !mysql.exists() {
        return Err(format!("mysql client not found at {}", mysql.display()));
    }
    let sql = format!(
        "CREATE DATABASE IF NOT EXISTS `{db_name}` \
         CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;"
    );
    let output = hidden_command(&mysql)
        .args([
            "--protocol=TCP",
            "-h",
            "127.0.0.1",
            "-P",
        ])
        .arg(port.to_string())
        .args(["-u", "root", "-e"])
        .arg(&sql)
        .output()
        .map_err(|e| format!("spawn mysql client: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "CREATE DATABASE failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

pub fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dest = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// Generic "copy files into target, create an empty DB" used by every CMS.
/// Returns the target dir for the caller to register as a host.
pub fn install_files_and_db(
    source_dir: &Path,
    target_dir: &Path,
    mysql_dir: &Path,
    mysql_port: u16,
    db_name: &str,
) -> Result<PathBuf, String> {
    if target_dir.exists() {
        return Err(format!(
            "Directory already exists: {}",
            target_dir.display()
        ));
    }
    if !source_dir.exists() {
        return Err(format!(
            "CMS source not found at {}. Run `pnpm scripts:fetch-binaries`.",
            source_dir.display()
        ));
    }
    if let Some(parent) = target_dir.parent() {
        if !parent.exists() {
            return Err(format!(
                "Parent directory does not exist: {}",
                parent.display()
            ));
        }
    }
    copy_dir_all(source_dir, target_dir).map_err(|e| format!("copy files: {e}"))?;
    create_database(mysql_dir, mysql_port, db_name)?;
    Ok(target_dir.to_path_buf())
}
