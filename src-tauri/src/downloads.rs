//! Runtime on-demand binary downloads. Mirrors `scripts/fetch-binaries.mjs`
//! so an installed Lamp Bench can fetch optional components (Redis, Nginx,
//! alternative PHP/MySQL versions, CMSes) without the user touching a CLI.
//!
//! The manifest is the same `scripts/binaries.json` the dev fetch uses —
//! embedded at compile time via `include_str!` so the installed app carries
//! the pinned URLs and SHA256s.

use crate::services::apache::PhpInstall;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MANIFEST_JSON: &str = include_str!("../../scripts/binaries.json");

#[derive(Debug, Deserialize)]
pub struct Manifest {
    #[serde(flatten)]
    pub entries: HashMap<String, Entry>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Entry {
    #[allow(dead_code)]
    pub version: String,
    pub extract_to: Option<String>,
    pub raw_file: Option<String>,
    #[serde(default = "default_bundled")]
    #[allow(dead_code)]
    pub bundled: bool,
    pub platforms: HashMap<String, PlatformEntry>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PlatformEntry {
    #[allow(dead_code)]
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub strip_root_dir: Option<String>,
}

fn default_bundled() -> bool {
    true
}

pub fn load_manifest() -> Result<Manifest, String> {
    serde_json::from_str(MANIFEST_JSON).map_err(|e| format!("parse binaries.json: {e}"))
}

/// One PHP version exposed to the UI's version dropdown.
#[derive(Debug, Serialize)]
pub struct PhpCatalogEntry {
    pub version: String,
    pub installed: bool,
}

/// All PHP versions in the manifest (`php-X.Y` entries), with a flag for
/// whether the files are present on disk under `resources_dir`. Used by the
/// Hosts form so the user can pick any PHP version and we'll fetch it on
/// demand if missing.
pub fn php_catalog(resources_dir: &Path) -> Vec<PhpCatalogEntry> {
    let manifest = match load_manifest() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<PhpCatalogEntry> = manifest
        .entries
        .iter()
        .filter_map(|(name, _)| {
            name.strip_prefix("php-").map(|v| PhpCatalogEntry {
                version: v.to_string(),
                installed: is_installed(name, resources_dir),
            })
        })
        .collect();
    out.sort_by(|a, b| a.version.cmp(&b.version));
    out
}

/// Scan `resources/` for `php-X.Y/` directories that look like real PHP
/// installs (have a `php-cgi.exe`). Returns the same `PhpInstall` shape the
/// services already consume so we can rebuild the install list at every
/// service start without restarting the whole app.
pub fn discover_php_installs(resources_dir: &Path) -> Vec<PhpInstall> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(resources_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(version) = name.strip_prefix("php-") else {
                continue;
            };
            let dir = entry.path();
            let cgi = dir.join(format!("php-cgi{}", std::env::consts::EXE_SUFFIX));
            if cgi.exists() {
                out.push(PhpInstall {
                    version: version.to_string(),
                    dir,
                });
            }
        }
    }
    out.sort_by(|a, b| a.version.cmp(&b.version));
    out
}

/// Convenience for the runtime download command: given a PHP version,
/// download the matching Xdebug DLL too if present in the manifest. Failing
/// to find an Xdebug build for a future PHP version is non-fatal.
pub fn install_php_with_xdebug(version: &str, resources_dir: &Path) -> Result<(), String> {
    download(&format!("php-{version}"), resources_dir)?;
    let _ = download(&format!("xdebug-{version}"), resources_dir);
    Ok(())
}

#[allow(dead_code)]
fn _path_used() -> PathBuf {
    PathBuf::new()
}

pub fn current_platform() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return "windows-x64";
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "macos-arm64";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return "macos-x64";
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return "linux-x64";
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
    )))]
    return "unsupported";
}

/// Does this binary currently exist on disk under `resources_dir`?
pub fn is_installed(name: &str, resources_dir: &Path) -> bool {
    let manifest = match load_manifest() {
        Ok(m) => m,
        Err(_) => return false,
    };
    let entry = match manifest.entries.get(name) {
        Some(e) => e,
        None => return false,
    };
    if let Some(raw) = &entry.raw_file {
        resources_dir.join(raw).exists()
    } else if let Some(extract_to) = &entry.extract_to {
        let target = resources_dir.join(extract_to);
        target.is_dir() && fs::read_dir(&target).map(|mut i| i.next().is_some()).unwrap_or(false)
    } else {
        false
    }
}

/// Fetch + verify + extract a manifest entry into `resources_dir`. Synchronous
/// (blocks the Tauri command). The caller is expected to update a "busy"
/// flag in the UI so the user knows something's happening.
pub fn download(name: &str, resources_dir: &Path) -> Result<(), String> {
    let manifest = load_manifest()?;
    let entry = manifest
        .entries
        .get(name)
        .ok_or_else(|| format!("unknown binary: {name}"))?;
    let platform = current_platform();
    let pe = entry.platforms.get(platform).ok_or_else(|| {
        format!("no {platform} binary configured for {name}")
    })?;

    // 1. HTTP fetch (rustls TLS, no system OpenSSL needed).
    let resp = ureq::get(&pe.url)
        .call()
        .map_err(|e| format!("HTTP {name}: {e}"))?;
    if resp.status() != 200 {
        return Err(format!("HTTP {} fetching {name}", resp.status()));
    }
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| format!("read body: {e}"))?;

    // 2. SHA256 verify against the pinned manifest.
    let mut hasher = Sha256::new();
    hasher.update(&buf);
    let actual = hex_upper(&hasher.finalize());
    if actual != pe.sha256.to_uppercase() {
        return Err(format!(
            "SHA256 mismatch for {name}: expected {} got {actual}",
            pe.sha256
        ));
    }

    // 3a. Raw file mode — drop the downloaded bytes at a fixed path.
    if let Some(raw) = &entry.raw_file {
        let target = resources_dir.join(raw);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&target, &buf).map_err(|e| e.to_string())?;
        return Ok(());
    }

    // 3b. Archive mode — extract zip into resources_dir/<extract_to>.
    let extract_to = entry
        .extract_to
        .as_deref()
        .ok_or_else(|| format!("{name}: neither extract_to nor raw_file set"))?;
    let target = resources_dir.join(extract_to);
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|e| format!("clear target: {e}"))?;
    }
    fs::create_dir_all(&target).map_err(|e| e.to_string())?;

    let cursor = std::io::Cursor::new(buf);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("open zip: {e}"))?;

    let strip_prefix = pe.strip_root_dir.as_deref();
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("zip entry {i}: {e}"))?;
        let entry_name = file
            .enclosed_name()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| file.name().to_string());

        let rel: &str = if let Some(prefix) = strip_prefix {
            let prefix_slash = format!("{prefix}/");
            match entry_name.strip_prefix(&prefix_slash) {
                Some(rest) => rest,
                None => {
                    if entry_name == prefix {
                        continue;
                    }
                    continue;
                }
            }
        } else {
            entry_name.as_str()
        };
        if rel.is_empty() {
            continue;
        }
        let out_path = target.join(rel);
        if file.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = fs::File::create(&out_path).map_err(|e| e.to_string())?;
            let mut chunk = [0u8; 65536];
            loop {
                let n = file.read(&mut chunk).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                out.write_all(&chunk[..n]).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

fn hex_upper(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02X}", b));
    }
    s
}
