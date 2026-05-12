//! Point-in-time captures of a host's docroot, stored as `.tar.zst`.
//!
//! Phase 7 ships file-only snapshots. The database side comes once hosts
//! carry an explicit `db_name` (or we parse it from `wp-config.php` /
//! `configuration.php`). For now users export/import the DB through
//! phpMyAdmin and snapshot only the files.

use crate::hosts::Host;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufWriter, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: i64,
    pub host_id: i64,
    pub label: String,
    pub path: String,
    pub size_bytes: i64,
    pub created_at: String,
}

pub fn list_for_host(conn: &Connection, host_id: i64) -> Result<Vec<Snapshot>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, host_id, label, path, size_bytes, created_at \
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

    let size = write_tar_zst(&docroot, &target)?;

    conn.execute(
        "INSERT INTO snapshots (host_id, label, path, size_bytes) VALUES (?1, ?2, ?3, ?4)",
        params![host.id, label.trim(), target.to_string_lossy(), size as i64],
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
    })
}

pub fn restore(conn: &Connection, snapshot_id: i64, host: &Host) -> Result<(), String> {
    let snapshot: Snapshot = conn
        .query_row(
            "SELECT id, host_id, label, path, size_bytes, created_at \
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

    extract_tar_zst(&archive_path, &docroot)
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

fn write_tar_zst(docroot: &Path, target: &Path) -> Result<u64, String> {
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
    tar.finish().map_err(|e| format!("tar.finish: {e}"))?;
    drop(tar);

    let size = fs::metadata(target).map(|m| m.len()).unwrap_or(0);
    Ok(size)
}

fn extract_tar_zst(archive: &Path, dest_dir: &Path) -> Result<(), String> {
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

    // We packed `<docroot_basename>/...` paths into the archive. Strip that
    // top-level component on restore so files land back where they came from.
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let mut comps = path.components();
        comps.next(); // skip first component
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
