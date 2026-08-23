//! Virtual host CRUD + reconciliation of the system `hosts` file.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
use crate::services::hidden_command;

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

/// Trim, lowercase and validate a user-supplied host name.
///
/// This is the only gate on a string that then travels somewhere expensive:
///
/// 1. an **elevated** write to `C:\Windows\…\hosts` / `/etc/hosts` — a bare
///    `\n` would let a typo append arbitrary entries to the system's name
///    resolution,
/// 2. a raw `ServerName` inside the generated Apache vhost,
/// 3. a **filename** under `runtime/ssl/`, where `../..` escapes the dir.
///
/// Lowercasing is not cosmetic either: SQLite's `UNIQUE` index is
/// case-sensitive, so `Site.local` and `site.local` used to coexist as two
/// rows resolving to the same name.
pub fn normalize_hostname(raw: &str) -> Result<String, String> {
    // A trailing dot is legal DNS (fully-qualified) but meaningless here and
    // would produce an empty final label.
    let name = raw.trim().trim_end_matches('.').to_ascii_lowercase();

    if name.is_empty() {
        return Err("name is required".into());
    }
    if name.len() > 253 {
        return Err("host name is too long (max 253 characters)".into());
    }
    if name == "localhost" {
        return Err(
            "`localhost` is reserved — it already serves the default htdocs vhost".into(),
        );
    }
    for label in name.split('.') {
        if label.is_empty() {
            return Err("host name has an empty part (check for a double dot)".into());
        }
        if label.len() > 63 {
            return Err(format!("`{label}` is longer than 63 characters"));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(format!("`{label}` must not start or end with a hyphen"));
        }
        if let Some(bad) = label.chars().find(|c| !c.is_ascii_alphanumeric() && *c != '-') {
            return Err(format!(
                "`{name}` contains `{bad}` — only letters, digits, hyphens and dots are allowed"
            ));
        }
    }
    // An all-numeric name is an IP literal. Mapping an address to itself in
    // the hosts file is always a mistake, and Apache would take it as a
    // literal ServerName that no browser sends.
    if name
        .split('.')
        .all(|l| l.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(
            "that looks like an IP address — use a name such as `myproject.local`".into(),
        );
    }
    Ok(name)
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
    let name = normalize_hostname(name)?;
    let docroot = docroot.trim();
    let php_version = php_version.trim();
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
    .map_err(|e| friendly_constraint_error(e, &name))?;
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
    let name = normalize_hostname(name)?;
    let docroot = docroot.trim();
    let php_version = php_version.trim();
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
        .map_err(|e| friendly_constraint_error(e, &name))?;
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

/// Turn SQLite's raw constraint text into something a user can act on.
fn friendly_constraint_error(e: rusqlite::Error, name: &str) -> String {
    let raw = e.to_string();
    if raw.contains("UNIQUE constraint failed") {
        format!("a host named `{name}` already exists")
    } else {
        raw
    }
}

/// Delete a host and everything it owns.
///
/// The `ON DELETE CASCADE` clauses (now that `PRAGMA foreign_keys` is
/// actually on) clear the `snapshots` and `deploy_profiles` rows, but nothing
/// in the database knows about the files those rows pointed at. Sweeping them
/// here is what keeps a deleted host from leaving its `.tar.zst` archives —
/// often hundreds of MB — and its leaf certificate behind forever.
pub fn delete(conn: &Connection, id: i64, runtime_dir: &Path) -> Result<(), String> {
    // Read the name before the row goes away; the cert files are keyed by it.
    let name: Option<String> = conn
        .query_row("SELECT name FROM hosts WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;

    conn.execute("DELETE FROM hosts WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;

    // Snapshots live under a numeric directory, so there's nothing to
    // sanitise here.
    let _ = fs::remove_dir_all(runtime_dir.join("snapshots").join(id.to_string()));

    // Rows predating `normalize_hostname` could hold anything, and this value
    // is about to be used as a path component — re-validate before touching
    // the filesystem rather than trusting what's already stored.
    if let Some(name) = name {
        if let Ok(safe) = normalize_hostname(&name) {
            let ssl = runtime_dir.join("ssl");
            let _ = fs::remove_file(ssl.join(format!("{safe}.crt")));
            let _ = fs::remove_file(ssl.join(format!("{safe}.key")));
        }
    }
    Ok(())
}

// ─── hosts file reconciliation ─────────────────────────────────────────────

const MANAGED_BEGIN: &str = "# === Lamp Bench managed — do not edit between these markers ===";
const MANAGED_END: &str = "# === Lamp Bench end ===";

/// Line ending for the managed block. CRLF was written on every platform,
/// which left stray carriage returns in a Unix `/etc/hosts` — cosmetic at
/// best, and a parser that treats `\\r` as part of the hostname at worst.
#[cfg(windows)]
const EOL: &str = "\r\n";
#[cfg(not(windows))]
const EOL: &str = "\n";

#[cfg(windows)]
const HOSTS_PATH: &str = r"C:\Windows\System32\drivers\etc\hosts";

#[cfg(not(windows))]
const HOSTS_PATH: &str = "/etc/hosts";

/// Reconcile the managed block in the system hosts file.
///
/// `runtime_dir` is where the staged copy is written before the elevated
/// command picks it up. On Unix that matters: the staged file used to live at
/// a fixed name in the world-writable `/tmp`, so between our write and
/// `pkexec cp` any local user could swap it and have root copy their content
/// into `/etc/hosts`.
pub fn apply_to_system(hosts: &[Host], runtime_dir: &Path) -> Result<(), String> {
    let current = fs::read_to_string(HOSTS_PATH)
        .map_err(|e| format!("read {HOSTS_PATH}: {e}"))?;
    let desired_section = build_managed_section(hosts);
    let desired = replace_section(&current, &desired_section);
    if desired == current {
        return Ok(());
    }
    #[cfg(windows)]
    {
        // %TEMP% is per-user on Windows, so it is already private — more so
        // than the install dir, which the installer deliberately grants the
        // Users group write access to.
        let _ = runtime_dir;
        write_elevated_windows(&desired)
    }
    #[cfg(target_os = "macos")]
    {
        write_elevated_macos(&desired, runtime_dir)
    }
    #[cfg(target_os = "linux")]
    {
        write_elevated_linux(&desired, runtime_dir)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = desired;
        Err("hosts-file editing is not supported on this OS".into())
    }
}

fn build_managed_section(hosts: &[Host]) -> String {
    if hosts.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    s.push_str(MANAGED_BEGIN);
    s.push_str(EOL);
    for h in hosts {
        s.push_str(&format!("127.0.0.1\t{}{EOL}", h.name));
    }
    s.push_str(MANAGED_END);
    s.push_str(EOL);
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
                s.push_str(EOL);
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

    let output = hidden_command("powershell")
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

/// Write the staged hosts file somewhere only this user can reach, and
/// create it 0600 *before* any content lands in it so there is no window
/// where it is world-readable either.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn stage_private(runtime_dir: &Path, content: &str) -> Result<std::path::PathBuf, String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    fs::create_dir_all(runtime_dir).map_err(|e| format!("create runtime dir: {e}"))?;
    let path = runtime_dir.join("hosts.staged");
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("stage hosts file: {e}"))?;
    f.write_all(content.as_bytes())
        .map_err(|e| format!("write staged hosts: {e}"))?;
    Ok(path)
}

#[cfg(target_os = "macos")]
fn write_elevated_macos(new_content: &str, runtime_dir: &Path) -> Result<(), String> {
    // macOS path: stage a file we own, then have osascript run `cp` with
    // administrator privileges. The single Touch ID / password prompt is the
    // macOS equivalent of UAC; cached per app for ~5 min.
    let tmp = stage_private(runtime_dir, new_content)?;
    let src = tmp.display().to_string().replace('"', "\\\"");
    let script = format!(
        "do shell script \"cp '{src}' '{HOSTS_PATH}'\" with administrator privileges"
    );
    let output = hidden_command("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("spawn osascript: {e}"))?;
    let _ = fs::remove_file(&tmp);
    if !output.status.success() {
        return Err(format!(
            "hosts file update rejected or failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn write_elevated_linux(new_content: &str, runtime_dir: &Path) -> Result<(), String> {
    // Linux path: prefer pkexec (graphical polkit prompt, no terminal needed).
    // Fall back to sudo -n in case we're being run in a CLI context where the
    // user already authenticated. Both invoke `cp` rather than a redirect so
    // we don't have to worry about shell quoting.
    let tmp = stage_private(runtime_dir, new_content)?;

    let runners: [&str; 2] = ["pkexec", "sudo"];
    let mut last_err = String::from("no privilege escalation tool found (need pkexec or sudo)");
    for runner in runners {
        // `which` check
        if hidden_command("which")
            .arg(runner)
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            continue;
        }
        let mut cmd = hidden_command(runner);
        if runner == "sudo" {
            cmd.arg("-n"); // non-interactive — fail fast if cached creds expired
        }
        cmd.arg("cp").arg(&tmp).arg(HOSTS_PATH);
        match cmd.output() {
            Ok(out) if out.status.success() => {
                let _ = fs::remove_file(&tmp);
                return Ok(());
            }
            Ok(out) => {
                last_err = format!(
                    "{runner} failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            Err(e) => {
                last_err = format!("{runner} spawn error: {e}");
            }
        }
    }
    let _ = fs::remove_file(&tmp);
    Err(format!("hosts file update failed: {last_err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(name: &str) -> Host {
        Host {
            id: 1,
            name: name.into(),
            docroot: "C:/x".into(),
            php_version: "8.4".into(),
            apache_extra: String::new(),
            nginx_extra: String::new(),
        }
    }

    #[test]
    fn empty_hosts_produce_empty_section() {
        assert_eq!(build_managed_section(&[]), "");
    }

    #[test]
    fn section_lists_every_host() {
        let s = build_managed_section(&[host("a.local"), host("b.local")]);
        assert!(s.contains("127.0.0.1\ta.local"));
        assert!(s.contains("127.0.0.1\tb.local"));
        assert!(s.starts_with(MANAGED_BEGIN));
        assert!(s.trim_end().ends_with(MANAGED_END));
    }

    #[test]
    fn replace_inserts_when_no_existing_section() {
        let current = "127.0.0.1 localhost\n";
        let section = build_managed_section(&[host("x.local")]);
        let out = replace_section(current, &section);
        assert!(out.starts_with("127.0.0.1 localhost"));
        assert!(out.contains("x.local"));
    }

    #[test]
    fn replace_swaps_existing_section_without_touching_user_lines() {
        let first = replace_section(
            "127.0.0.1 localhost\n",
            &build_managed_section(&[host("old.local")]),
        );
        let second = replace_section(&first, &build_managed_section(&[host("new.local")]));
        assert!(second.contains("new.local"));
        assert!(!second.contains("old.local"));
        // user line preserved exactly once
        assert_eq!(second.matches("127.0.0.1 localhost").count(), 1);
    }

    #[test]
    fn hostname_is_trimmed_and_lowercased() {
        assert_eq!(normalize_hostname("  MySite.Local  ").unwrap(), "mysite.local");
        // A fully-qualified trailing dot is legal DNS but would produce an
        // empty final label here.
        assert_eq!(normalize_hostname("site.local.").unwrap(), "site.local");
        // Single-label names are fine — the hosts file resolves them.
        assert_eq!(normalize_hostname("myproject").unwrap(), "myproject");
    }

    #[test]
    fn hostname_rejects_injection_and_traversal() {
        // The value is written to the system hosts file with elevation.
        assert!(normalize_hostname("ok.local\n127.0.0.1 evil.com").is_err());
        // ...and used as a filename under runtime/ssl/.
        assert!(normalize_hostname("../../etc/passwd").is_err());
        assert!(normalize_hostname("a/b").is_err());
        // ...and interpolated raw into the Apache vhost.
        assert!(normalize_hostname("x\"\n</VirtualHost>").is_err());
    }

    #[test]
    fn hostname_rejects_malformed_names() {
        assert!(normalize_hostname("").is_err());
        assert!(normalize_hostname("   ").is_err());
        assert!(normalize_hostname("has space.local").is_err());
        assert!(normalize_hostname("double..dot").is_err());
        assert!(normalize_hostname("-leading.local").is_err());
        assert!(normalize_hostname("trailing-.local").is_err());
        assert!(normalize_hostname(&"a".repeat(64)).is_err());
        assert!(normalize_hostname("localhost").is_err());
        assert!(normalize_hostname("LocalHost").is_err());
        assert!(normalize_hostname("127.0.0.1").is_err());
        assert!(normalize_hostname("8").is_err());
    }

    #[test]
    fn hostname_accepts_realistic_names() {
        for good in [
            "myproject.local",
            "my-project.local",
            "api.my-project.test",
            "shop2.local",
            &"a".repeat(63),
        ] {
            assert!(normalize_hostname(good).is_ok(), "rejected: {good}");
        }
    }

    #[test]
    fn replace_is_idempotent() {
        let section = build_managed_section(&[host("x.local")]);
        let once = replace_section("# header\n", &section);
        let twice = replace_section(&once, &section);
        assert_eq!(once, twice);
    }
}
