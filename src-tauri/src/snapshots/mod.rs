//! Point-in-time captures of a host's docroot, stored as `.tar.zst`.
//!
//! Phase 7 ships file-only snapshots. The database side comes once hosts
//! carry an explicit `db_name` (or we parse it from `wp-config.php` /
//! `configuration.php`). For now users export/import the DB through
//! phpMyAdmin and snapshot only the files.

use crate::hosts::Host;
use crate::services::{bin_path, hidden_command};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: i64,
    pub host_id: i64,
    pub label: String,
    pub path: String,
    pub size_bytes: i64,
    pub created_at: String,
    pub has_db: bool,
    /// MySQL version the dump was taken under (e.g. "5.7", "8.0"). Empty for
    /// files-only snapshots and rows predating the column.
    pub mysql_version: String,
}

/// Parameters for capturing a MySQL database into the snapshot archive. When
/// `Some`, the snapshot will include a `db.sql` entry produced by mysqldump
/// of the named database, and restoring the snapshot will re-import it.
pub struct DbCapture<'a> {
    pub mysql_dir: &'a Path,
    pub port: u16,
    pub db_name: &'a str,
    /// Active MySQL version label, stored alongside the snapshot so we can
    /// warn the user when restoring into a different server version.
    pub version: &'a str,
}

pub fn list_for_host(conn: &Connection, host_id: i64) -> Result<Vec<Snapshot>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, host_id, label, path, size_bytes, created_at, has_db, mysql_version \
             FROM snapshots WHERE host_id = ?1 ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![host_id], |row| {
            Ok(Snapshot {
                id: row.get(0)?,
                host_id: row.get(1)?,
                label: row.get(2)?,
                path: row.get(3)?,
                size_bytes: row.get(4)?,
                created_at: row.get(5)?,
                has_db: row.get::<_, i64>(6)? != 0,
                mysql_version: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn create(
    conn: &Connection,
    host: &Host,
    label: &str,
    runtime_dir: &Path,
    db: Option<DbCapture>,
) -> Result<Snapshot, String> {
    let docroot = PathBuf::from(&host.docroot);
    if !docroot.exists() {
        return Err(format!("docroot does not exist: {}", docroot.display()));
    }

    // Filesystem-safe timestamp like 20260512-143015.
    let ts = current_timestamp();
    let snapshots_dir = runtime_dir
        .join("snapshots")
        .join(host.id.to_string());
    fs::create_dir_all(&snapshots_dir).map_err(|e| format!("create snapshots dir: {e}"))?;
    let filename = format!("{ts}.tar.zst");
    let target = snapshots_dir.join(&filename);

    let db_dump = match &db {
        Some(d) => Some(run_mysqldump(d)?),
        None => None,
    };
    let has_db = db_dump.is_some();
    let mysql_version = db.as_ref().map(|d| d.version).unwrap_or("").to_string();

    let size = write_tar_zst(&docroot, &target, db_dump.as_deref())?;

    conn.execute(
        "INSERT INTO snapshots (host_id, label, path, size_bytes, has_db, mysql_version) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            host.id,
            label.trim(),
            target.to_string_lossy(),
            size as i64,
            if has_db { 1 } else { 0 },
            mysql_version,
        ],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    Ok(Snapshot {
        id,
        host_id: host.id,
        label: label.trim().to_string(),
        path: target.to_string_lossy().into_owned(),
        size_bytes: size as i64,
        created_at: now_iso(),
        has_db,
        mysql_version,
    })
}

fn run_mysqldump(d: &DbCapture) -> Result<Vec<u8>, String> {
    let dumper = bin_path(d.mysql_dir, "mysqldump");
    if !dumper.exists() {
        return Err(format!("mysqldump not found at {}", dumper.display()));
    }
    // `--databases` makes the dump self-contained: it emits CREATE DATABASE
    // + USE statements, so restoring is a single pipe into `mysql` without
    // having to remember which DB the dump came from.
    let output = hidden_command(&dumper)
        .args([
            "--protocol=TCP",
            "-h",
            "127.0.0.1",
            "-P",
        ])
        .arg(d.port.to_string())
        .args(["-u", "root", "--databases", d.db_name])
        .output()
        .map_err(|e| format!("spawn mysqldump: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "mysqldump failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

pub fn restore(
    conn: &Connection,
    snapshot_id: i64,
    host: &Host,
    mysql_dir: &Path,
    mysql_port: u16,
) -> Result<(), String> {
    let snapshot: Snapshot = conn
        .query_row(
            "SELECT id, host_id, label, path, size_bytes, created_at, has_db, mysql_version \
             FROM snapshots WHERE id = ?1",
            params![snapshot_id],
            |row| {
                Ok(Snapshot {
                    id: row.get(0)?,
                    host_id: row.get(1)?,
                    label: row.get(2)?,
                    path: row.get(3)?,
                    size_bytes: row.get(4)?,
                    created_at: row.get(5)?,
                    has_db: row.get::<_, i64>(6)? != 0,
                    mysql_version: row.get(7)?,
                })
            },
        )
        .map_err(|e| format!("snapshot not found: {e}"))?;
    if snapshot.host_id != host.id {
        return Err("snapshot does not belong to this host".into());
    }
    let archive_path = PathBuf::from(&snapshot.path);
    if !archive_path.exists() {
        return Err(format!(
            "snapshot file missing on disk: {}",
            archive_path.display()
        ));
    }
    let docroot = PathBuf::from(&host.docroot);
    fs::create_dir_all(&docroot).map_err(|e| e.to_string())?;

    extract_tar_zst(&archive_path, &docroot, mysql_dir, mysql_port)
}

pub fn delete(conn: &Connection, snapshot_id: i64) -> Result<(), String> {
    let path: String = conn
        .query_row(
            "SELECT path FROM snapshots WHERE id = ?1",
            params![snapshot_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("snapshot not found: {e}"))?;
    let _ = fs::remove_file(&path);
    conn.execute(
        "DELETE FROM snapshots WHERE id = ?1",
        params![snapshot_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn write_tar_zst(docroot: &Path, target: &Path, db_sql: Option<&[u8]>) -> Result<u64, String> {
    let file = fs::File::create(target)
        .map_err(|e| format!("create snapshot file: {e}"))?;
    let buf = BufWriter::new(file);
    let zstd_encoder = zstd::stream::Encoder::new(buf, 6)
        .map_err(|e| format!("init zstd: {e}"))?
        .auto_finish();
    let mut tar = tar::Builder::new(zstd_encoder);

    // append_dir_all writes the entire tree relative to the directory's name.
    // We strip the parent so the archive contains paths relative to the
    // docroot root (i.e. `index.php`, not `myhost/index.php`).
    let docroot_name = docroot
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| std::ffi::OsString::from("docroot"));
    tar.append_dir_all(&docroot_name, docroot)
        .map_err(|e| format!("tar.append_dir_all: {e}"))?;

    // Optional MySQL dump alongside the files at the archive root. Restore
    // picks it up by path equality and pipes it into `mysql` to recreate
    // the database.
    if let Some(sql) = db_sql {
        let mut header = tar::Header::new_gnu();
        header.set_size(sql.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, "db.sql", sql)
            .map_err(|e| format!("tar.append db.sql: {e}"))?;
    }

    tar.finish().map_err(|e| format!("tar.finish: {e}"))?;
    drop(tar);

    let size = fs::metadata(target).map(|m| m.len()).unwrap_or(0);
    Ok(size)
}

fn extract_tar_zst(
    archive: &Path,
    dest_dir: &Path,
    mysql_dir: &Path,
    mysql_port: u16,
) -> Result<(), String> {
    let file = fs::File::open(archive)
        .map_err(|e| format!("open snapshot file: {e}"))?;
    let mut decoder = zstd::stream::Decoder::new(file)
        .map_err(|e| format!("init zstd decoder: {e}"))?;
    let mut buf = Vec::new();
    decoder
        .read_to_end(&mut buf)
        .map_err(|e| format!("read zstd: {e}"))?;
    let cursor = std::io::Cursor::new(buf);
    let mut tar = tar::Archive::new(cursor);

    // We packed `<docroot_basename>/...` paths into the archive plus an
    // optional `db.sql` at the archive root. File entries get their top
    // component stripped (files land back at docroot); db.sql is buffered
    // and piped into the mysql client after the file restore finishes so
    // a DB failure doesn't leave files half-restored without warning.
    let mut sql_blob: Option<Vec<u8>> = None;
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        if path == Path::new("db.sql") {
            let mut data = Vec::new();
            entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
            sql_blob = Some(data);
            continue;
        }
        let mut comps = path.components();
        comps.next(); // skip first component (docroot basename)
        let rel: PathBuf = comps.collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dest_dir.join(&rel);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            entry
                .unpack(&target)
                .map_err(|e| format!("unpack {}: {e}", target.display()))?;
        }
    }

    if let Some(sql) = sql_blob {
        restore_mysql_dump(&sql, mysql_dir, mysql_port)?;
    }

    Ok(())
}

fn restore_mysql_dump(sql: &[u8], mysql_dir: &Path, port: u16) -> Result<(), String> {
    let client = bin_path(mysql_dir, "mysql");
    if !client.exists() {
        return Err(format!("mysql client not found at {}", client.display()));
    }
    let mut child = hidden_command(&client)
        .args([
            "--protocol=TCP",
            "-h",
            "127.0.0.1",
            "-P",
        ])
        .arg(port.to_string())
        .args(["-u", "root"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn mysql client: {e}"))?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "mysql stdin unavailable".to_string())?;
        stdin
            .write_all(sql)
            .map_err(|e| format!("pipe sql to mysql: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("wait mysql: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "mysql restore failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn current_timestamp() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
}

fn now_iso() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
}
