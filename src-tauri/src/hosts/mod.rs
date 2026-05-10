//! Virtual host CRUD + reconciliation of the system `hosts` file.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: i64,
    pub name: String,
    pub docroot: String,
    pub php_version: String,
    #[serde(default)]
    pub apache_extra: String,
    #[serde(default)]
    pub nginx_extra: String,
}

pub fn list(conn: &Connection) -> Result<Vec<Host>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, docroot, php_version, apache_extra, nginx_extra \
             FROM hosts ORDER BY name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Host {
                id: row.get(0)?,
                name: row.get(1)?,
                docroot: row.get(2)?,
                php_version: row.get(3)?,
                apache_extra: row.get(4)?,
                nginx_extra: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn create(
    conn: &Connection,
    name: &str,
    docroot: &str,
    php_version: &str,
) -> Result<Host, String> {
    let name = name.trim();
    let docroot = docroot.trim();
    let php_version = php_version.trim();
    if name.is_empty() {
        return Err("name is required".into());
    }
    if docroot.is_empty() {
        return Err("docroot is required".into());
    }
    if php_version.is_empty() {
        return Err("php_version is required".into());
    }
    conn.execute(
        "INSERT INTO hosts (name, docroot, php_version) VALUES (?1, ?2, ?3)",
        params![name, docroot, php_version],
    )
    .map_err(|e| e.to_string())?;
    Ok(Host {
        id: conn.last_insert_rowid(),
        name: name.to_string(),
        docroot: docroot.to_string(),
        php_version: php_version.to_string(),
        apache_extra: String::new(),
        nginx_extra: String::new(),
    })
}

pub fn update(
    conn: &Connection,
    id: i64,
    name: &str,
    docroot: &str,
    php_version: &str,
    apache_extra: &str,
    nginx_extra: &str,
) -> Result<Host, String> {
    let name = name.trim();
    let docroot = docroot.trim();
    let php_version = php_version.trim();
    if name.is_empty() {
        return Err("name is required".into());
    }
    if docroot.is_empty() {
        return Err("docroot is required".into());
    }
    if php_version.is_empty() {
        return Err("php_version is required".into());
    }
    let affected = conn
        .execute(
            "UPDATE hosts SET name=?1, docroot=?2, php_version=?3, \
             apache_extra=?4, nginx_extra=?5 WHERE id=?6",
            params![name, docroot, php_version, apache_extra, nginx_extra, id],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err(format!("no host with id {id}"));
    }
    Ok(Host {
        id,
        name: name.to_string(),
        docroot: docroot.to_string(),
        php_version: php_version.to_string(),
        apache_extra: apache_extra.to_string(),
        nginx_extra: nginx_extra.to_string(),
    })
}

pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM hosts WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ─── hosts file reconciliation ─────────────────────────────────────────────

const MANAGED_BEGIN: &str = "# === Lamp Bench managed — do not edit between these markers ===";
const MANAGED_END: &str = "# === Lamp Bench end ===";

#[cfg(windows)]
const HOSTS_PATH: &str = r"C:\Windows\System32\drivers\etc\hosts";

#[cfg(not(windows))]
const HOSTS_PATH: &str = "/etc/hosts";

pub fn apply_to_system(hosts: &[Host]) -> Result<(), String> {
    let current = fs::read_to_string(HOSTS_PATH)
        .map_err(|e| format!("read {HOSTS_PATH}: {e}"))?;
    let desired_section = build_managed_section(hosts);
    let desired = replace_section(&current, &desired_section);
    if desired == current {
        return Ok(());
    }
    #[cfg(windows)]
    {
        write_elevated_windows(&desired)
    }
    #[cfg(not(windows))]
    {
        let _ = desired;
        Err("hosts-file editing on this OS is not implemented until Phase 9".into())
    }
}

fn build_managed_section(hosts: &[Host]) -> String {
    if hosts.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    s.push_str(MANAGED_BEGIN);
    s.push_str("\r\n");
    for h in hosts {
        s.push_str(&format!("127.0.0.1\t{}\r\n", h.name));
    }
    s.push_str(MANAGED_END);
    s.push_str("\r\n");
    s
}

fn replace_section(current: &str, new_section: &str) -> String {
    let begin = current.find(MANAGED_BEGIN);
    let end = current.find(MANAGED_END);
    match (begin, end) {
        (Some(bi), Some(ei)) if ei > bi => {
            let before = &current[..bi];
            let end_offset = ei + MANAGED_END.len();
            let mut tail = &current[end_offset..];
            if let Some(stripped) = tail.strip_prefix("\r\n") {
                tail = stripped;
            } else if let Some(stripped) = tail.strip_prefix('\n') {
                tail = stripped;
            }
            format!("{before}{new_section}{tail}")
        }
        _ => {
            let mut s = current.to_string();
            if !s.is_empty() && !s.ends_with('\n') {
                s.push_str("\r\n");
            }
            s.push_str(new_section);
            s
        }
    }
}

#[cfg(windows)]
fn write_elevated_windows(new_content: &str) -> Result<(), String> {
    let tmp_dir = std::env::temp_dir();
    let tmp_hosts = tmp_dir.join("lamp-bench-hosts.tmp");
    let tmp_script = tmp_dir.join("lamp-bench-elev.ps1");

    fs::write(&tmp_hosts, new_content).map_err(|e| format!("write tmp hosts: {e}"))?;
    let elev = format!(
        "$ErrorActionPreference = 'Stop'\r\n\
         Copy-Item -Force -LiteralPath '{}' -Destination '{}'\r\n",
        tmp_hosts.display().to_string().replace('\'', "''"),
        HOSTS_PATH.replace('\'', "''")
    );
    fs::write(&tmp_script, elev).map_err(|e| format!("write tmp script: {e}"))?;

    let runner = format!(
        "try {{\r\n  \
            $p = Start-Process powershell.exe -Verb RunAs -Wait -PassThru \
                 -ArgumentList '-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File','{}'\r\n  \
            exit $p.ExitCode\r\n\
         }} catch {{\r\n  \
            Write-Error $_\r\n  \
            exit 1\r\n\
         }}",
        tmp_script.display().to_string().replace('\'', "''"),
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &runner])
        .output()
        .map_err(|e| format!("spawn powershell: {e}"))?;

    let _ = fs::remove_file(&tmp_hosts);
    let _ = fs::remove_file(&tmp_script);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "hosts file update rejected or failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }
    Ok(())
}
