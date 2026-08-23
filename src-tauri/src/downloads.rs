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
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub strip_root_dir: Option<String>,
    /// Overrides the entry-level `raw_file` for this platform. MailHog is the
    /// motivating case: one Go binary per OS, landing at `MailHog.exe` on
    /// Windows and plain `MailHog` everywhere else.
    pub raw_file: Option<String>,
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
        .keys()
        .filter_map(|name| {
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
    download(&format!("php-{version}"), resources_dir, None)?;
    let _ = download(&format!("xdebug-{version}"), resources_dir, None);
    Ok(())
}

/// Remove an installed binary. Idempotent — returns Ok even if nothing was
/// on disk. Used by the Settings → Versions panel to reclaim disk for PHP
/// versions, optional services, etc. that the user is done with.
pub fn remove(name: &str, resources_dir: &Path) -> Result<(), String> {
    let manifest = load_manifest()?;
    let entry = manifest
        .entries
        .get(name)
        .ok_or_else(|| format!("unknown binary: {name}"))?;
    if let Some(raw) = raw_file_target(entry, entry.platforms.get(current_platform())) {
        let target = resources_dir.join(raw);
        if target.exists() {
            fs::remove_file(&target).map_err(|e| e.to_string())?;
        }
    }
    if let Some(extract_to) = &entry.extract_to {
        let target = resources_dir.join(extract_to);
        if target.exists() {
            fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// List every name in the manifest. Used by the Versions UI so we can show
/// everything that COULD be installed, not just what currently is.
pub fn list_manifest_entries() -> Vec<String> {
    let manifest = match load_manifest() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    let mut names: Vec<String> = manifest.entries.keys().cloned().collect();
    names.sort();
    names
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

/// Where a raw-file entry lands, honouring a per-platform override.
fn raw_file_target<'a>(entry: &'a Entry, pe: Option<&'a PlatformEntry>) -> Option<&'a str> {
    pe.and_then(|p| p.raw_file.as_deref())
        .or(entry.raw_file.as_deref())
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
    if let Some(raw) = raw_file_target(entry, entry.platforms.get(current_platform())) {
        resources_dir.join(raw).exists()
    } else if let Some(extract_to) = &entry.extract_to {
        let target = resources_dir.join(extract_to);
        target.is_dir() && fs::read_dir(&target).map(|mut i| i.next().is_some()).unwrap_or(false)
    } else {
        false
    }
}

/// Streaming progress event for `download`. Callers pass a closure that
/// gets called periodically with `(downloaded_bytes, total_bytes_or_none)`.
pub type ProgressCb<'a> = &'a mut dyn FnMut(u64, Option<u64>);

/// Which container an entry's archive uses, worked out from its filename.
///
/// Windows builds are uniformly zips. Unix ones are not: MySQL's Linux build
/// is `.tar.xz` and its macOS build `.tar.gz`, so a zip-only extractor could
/// never unpack a single non-Windows service no matter what URLs the manifest
/// pinned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    TarXz,
}

fn detect_format(filename: &str) -> Result<ArchiveFormat, String> {
    let f = filename.to_ascii_lowercase();
    if f.ends_with(".zip") {
        Ok(ArchiveFormat::Zip)
    } else if f.ends_with(".tar.gz") || f.ends_with(".tgz") {
        Ok(ArchiveFormat::TarGz)
    } else if f.ends_with(".tar.xz") || f.ends_with(".txz") {
        Ok(ArchiveFormat::TarXz)
    } else if f.ends_with(".tar") {
        Ok(ArchiveFormat::Tar)
    } else {
        Err(format!(
            "don't know how to extract `{filename}` — expected .zip, .tar, .tar.gz or .tar.xz"
        ))
    }
}

/// A stalled connection used to hang the first-launch wizard indefinitely,
/// with no cancel button and no way into the app. These are *per-read*
/// timeouts, so a slow-but-alive 900 MB download still completes; only a
/// genuinely dead socket trips them.
fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(60))
        .build()
}

/// Fetch + verify + extract a manifest entry into `resources_dir`. Synchronous
/// (blocks the Tauri command). The optional progress callback is invoked
/// periodically during the HTTP fetch so the UI can show a percentage instead
/// of an indeterminate spinner.
pub fn download(
    name: &str,
    resources_dir: &Path,
    progress: Option<ProgressCb<'_>>,
) -> Result<(), String> {
    let manifest = load_manifest()?;
    let entry = manifest
        .entries
        .get(name)
        .ok_or_else(|| format!("unknown binary: {name}"))?;
    let platform = current_platform();
    let pe = entry
        .platforms
        .get(platform)
        .ok_or_else(|| format!("no {platform} binary configured for {name}"))?;

    // Downloads land on disk, not in a Vec. MySQL's Linux tarball is ~900 MB
    // and its macOS one ~170 MB; buffering those in memory and then
    // decompressing from an in-memory cursor was a straightforward way to
    // exhaust RAM on a modest machine.
    let scratch = resources_dir.join(".cache");
    fs::create_dir_all(&scratch).map_err(|e| format!("create cache dir: {e}"))?;
    let archive_path = scratch.join(&pe.filename);

    fetch_to_file(&pe.url, &archive_path, &pe.sha256, name, progress)?;

    let result = install_from_archive(entry, pe, &archive_path, resources_dir);

    // The dev-time `fetch-binaries.mjs` keeps its cache on purpose; the app
    // shouldn't leave a second copy of every service on a user's disk.
    let _ = fs::remove_file(&archive_path);
    result
}

/// Stream `url` into `dest`, hashing as it goes, and fail before anything is
/// unpacked if the digest doesn't match the pin.
fn fetch_to_file(
    url: &str,
    dest: &Path,
    expected_sha: &str,
    name: &str,
    mut progress: Option<ProgressCb<'_>>,
) -> Result<(), String> {
    let resp = http_agent()
        .get(url)
        .call()
        .map_err(|e| format!("HTTP {name}: {e}"))?;
    if resp.status() != 200 {
        return Err(format!("HTTP {} fetching {name}", resp.status()));
    }
    let content_len: Option<u64> = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok());
    if let Some(cb) = progress.as_mut() {
        cb(0, content_len);
    }

    let mut reader = resp.into_reader();
    let mut file = std::io::BufWriter::new(
        fs::File::create(dest).map_err(|e| format!("create {}: {e}", dest.display()))?,
    );
    let mut hasher = Sha256::new();
    let mut chunk = vec![0u8; 65536];
    let mut total: u64 = 0;
    let mut last_reported: u64 = 0;
    loop {
        let n = reader
            .read(&mut chunk)
            .map_err(|e| format!("read body: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&chunk[..n]);
        file.write_all(&chunk[..n])
            .map_err(|e| format!("write {}: {e}", dest.display()))?;
        total += n as u64;
        if let Some(cb) = progress.as_mut() {
            // Throttle to ~64 KB granularity so we don't flood the IPC bridge
            // with millions of tiny events for a fast download.
            if total - last_reported >= 65536 {
                cb(total, content_len);
                last_reported = total;
            }
        }
    }
    file.flush().map_err(|e| format!("flush: {e}"))?;
    drop(file);
    if let Some(cb) = progress.as_mut() {
        cb(total, content_len);
    }

    let actual = hex_upper(&hasher.finalize());
    if actual != expected_sha.to_uppercase() {
        let _ = fs::remove_file(dest);
        return Err(format!(
            "SHA256 mismatch for {name}: expected {expected_sha} got {actual}"
        ));
    }
    Ok(())
}

/// Put a verified archive where the entry says it belongs.
fn install_from_archive(
    entry: &Entry,
    pe: &PlatformEntry,
    archive_path: &Path,
    resources_dir: &Path,
) -> Result<(), String> {
    // Raw-file mode — the download *is* the artefact (an Xdebug DLL, a
    // composer.phar, a MailHog binary).
    if let Some(raw) = raw_file_target(entry, Some(pe)) {
        let target = resources_dir.join(raw);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(archive_path, &target).map_err(|e| e.to_string())?;
        make_executable(&target)?;
        return Ok(());
    }

    let extract_to = entry
        .extract_to
        .as_deref()
        .ok_or("neither extract_to nor raw_file set for this entry")?;
    let target = resources_dir.join(extract_to);
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|e| format!("clear target: {e}"))?;
    }
    fs::create_dir_all(&target).map_err(|e| e.to_string())?;

    let format = detect_format(&pe.filename)?;
    match format {
        ArchiveFormat::Zip => extract_zip(archive_path, &target, pe.strip_root_dir.as_deref()),
        ArchiveFormat::Tar | ArchiveFormat::TarGz | ArchiveFormat::TarXz => {
            extract_tar(archive_path, &target, format, pe.strip_root_dir.as_deref())
        }
    }
}

/// Mark a file executable on Unix. No-op on Windows, where the extension
/// decides. Raw-file downloads arrive as plain bytes with no mode attached,
/// so without this a downloaded MailHog binary can't be spawned at all.
fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
        let mode = perms.mode();
        perms.set_mode(mode | 0o755);
        fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Drop the leading `prefix/` from an archive path, or return None when the
/// entry lives outside it (those are skipped, which is what `strip_root_dir`
/// means: unwrap this one directory).
fn strip_prefix<'a>(entry_name: &'a str, prefix: Option<&str>) -> Option<&'a str> {
    match prefix {
        Some(p) => entry_name
            .strip_prefix(p)
            .and_then(|rest| rest.strip_prefix('/')),
        None => Some(entry_name),
    }
}

/// Reject anything that would escape the extraction directory. Our own
/// archives are well-behaved, but an upstream we don't control shouldn't be
/// able to write wherever it likes.
fn is_safe_relative(rel: &str) -> bool {
    !rel.is_empty()
        && rel != "."
        && !Path::new(rel).is_absolute()
        && !Path::new(rel)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

fn extract_zip(archive: &Path, target: &Path, strip: Option<&str>) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| format!("open archive: {e}"))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| format!("open zip: {e}"))?;

    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| format!("zip entry {i}: {e}"))?;
        let entry_name = file
            .enclosed_name()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| file.name().to_string());

        let Some(rel) = strip_prefix(&entry_name, strip) else {
            continue;
        };
        let rel = rel.trim_end_matches('/');
        if !is_safe_relative(rel) {
            continue;
        }
        let out_path = target.join(rel);

        // macOS frameworks and .dylib version chains are held together by
        // symlinks; writing them out as regular files quietly breaks the
        // library at load time.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
                let mut link_target = String::new();
                file.read_to_string(&mut link_target)
                    .map_err(|e| e.to_string())?;
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let _ = fs::remove_file(&out_path);
                std::os::unix::fs::symlink(&link_target, &out_path).map_err(|e| e.to_string())?;
                continue;
            }
        }

        if file.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = fs::File::create(&out_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
        drop(out);

        // The zip format carries the Unix mode; the crate does not apply it.
        // Skipping this is why a Unix binary extracted from a zip came out
        // 0644 and refused to run.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                let _ = fs::set_permissions(&out_path, fs::Permissions::from_mode(mode));
            }
        }
    }
    Ok(())
}

fn extract_tar(
    archive: &Path,
    target: &Path,
    format: ArchiveFormat,
    strip: Option<&str>,
) -> Result<(), String> {
    // xz has no streaming decoder available here, so it is expanded to a
    // sibling file rather than into memory — the whole point of the rewrite is
    // to stop holding a 900 MB payload in RAM.
    let mut xz_tmp: Option<PathBuf> = None;
    let stream: Box<dyn Read> = match format {
        ArchiveFormat::Tar => Box::new(std::io::BufReader::new(
            fs::File::open(archive).map_err(|e| format!("open archive: {e}"))?,
        )),
        ArchiveFormat::TarGz => Box::new(flate2::read::GzDecoder::new(std::io::BufReader::new(
            fs::File::open(archive).map_err(|e| format!("open archive: {e}"))?,
        ))),
        ArchiveFormat::TarXz => {
            let tmp = archive.with_extension("untar");
            let mut input = std::io::BufReader::new(
                fs::File::open(archive).map_err(|e| format!("open archive: {e}"))?,
            );
            let mut out = std::io::BufWriter::new(
                fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?,
            );
            lzma_rs::xz_decompress(&mut input, &mut out)
                .map_err(|e| format!("xz decompress: {e}"))?;
            out.flush().map_err(|e| e.to_string())?;
            drop(out);
            xz_tmp = Some(tmp.clone());
            Box::new(std::io::BufReader::new(
                fs::File::open(&tmp).map_err(|e| e.to_string())?,
            ))
        }
        ArchiveFormat::Zip => unreachable!("zip is handled by extract_zip"),
    };

    let result = unpack_tar_entries(stream, target, strip);
    if let Some(tmp) = xz_tmp {
        let _ = fs::remove_file(tmp);
    }
    result
}

fn unpack_tar_entries(
    stream: Box<dyn Read>,
    target: &Path,
    strip: Option<&str>,
) -> Result<(), String> {
    let mut tar = tar::Archive::new(stream);
    // The tar crate applies the stored mode and recreates symlinks for us,
    // which is most of why Unix archives are less work than zips.
    tar.set_preserve_permissions(true);
    tar.set_unpack_xattrs(false);

    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let name = path.to_string_lossy().replace('\\', "/");
        let name = name.trim_end_matches('/');

        let Some(rel) = strip_prefix(name, strip) else {
            continue;
        };
        if !is_safe_relative(rel) {
            continue;
        }

        let out_path = target.join(rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        entry
            .unpack(&out_path)
            .map_err(|e| format!("unpack {}: {e}", out_path.display()))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_detected_from_the_filename() {
        assert_eq!(detect_format("httpd-2.4.66.zip").unwrap(), ArchiveFormat::Zip);
        assert_eq!(
            detect_format("mysql-8.0.46-macos15-arm64.tar.gz").unwrap(),
            ArchiveFormat::TarGz
        );
        assert_eq!(
            detect_format("mysql-8.0.46-linux-glibc2.28-x86_64.tar.xz").unwrap(),
            ArchiveFormat::TarXz
        );
        assert_eq!(detect_format("thing.TGZ").unwrap(), ArchiveFormat::TarGz);
        assert_eq!(detect_format("plain.tar").unwrap(), ArchiveFormat::Tar);
        // A bare binary is raw_file mode and never reaches the extractor, so
        // getting here at all means the manifest entry is misconfigured.
        assert!(detect_format("MailHog_linux_amd64").is_err());
        assert!(detect_format("installer.dmg").is_err());
    }

    #[test]
    fn strip_prefix_unwraps_exactly_one_root() {
        assert_eq!(strip_prefix("nginx-1.30.0/conf/nginx.conf", Some("nginx-1.30.0")), Some("conf/nginx.conf"));
        assert_eq!(strip_prefix("conf/nginx.conf", None), Some("conf/nginx.conf"));
        // Entries outside the declared root are skipped rather than dumped at
        // the top level.
        assert_eq!(strip_prefix("other/file", Some("nginx-1.30.0")), None);
        // A sibling whose name merely starts with the prefix must not match.
        assert_eq!(strip_prefix("nginx-1.30.0-extra/f", Some("nginx-1.30.0")), None);
    }

    /// Unique scratch directory. No `tempfile` dependency for two tests.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lamp-bench-test-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A tar holding `pkg-1.0/bin/tool` and `pkg-1.0/conf/app.conf`, i.e. the
    /// single-root layout every upstream tarball uses and that
    /// `strip_root_dir` exists to unwrap.
    fn sample_tar() -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, body) in [
            ("pkg-1.0/bin/tool", &b"#!/bin/sh\necho hi\n"[..]),
            ("pkg-1.0/conf/app.conf", &b"key = value\n"[..]),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            // 0o755 so the permission assertion below is meaningful.
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, body).unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn assert_extracted(target: &Path) {
        let tool = target.join("bin/tool");
        let conf = target.join("conf/app.conf");
        assert!(tool.is_file(), "missing {}", tool.display());
        assert_eq!(fs::read_to_string(&conf).unwrap(), "key = value\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&tool).unwrap().permissions().mode();
            // The whole reason Unix support was blocked: an extracted binary
            // that comes out 0644 cannot be spawned.
            assert!(mode & 0o111 != 0, "extracted tool is not executable: {mode:o}");
        }
    }

    #[test]
    fn plain_tar_round_trips_with_root_stripped() {
        let dir = scratch("tar");
        let archive = dir.join("pkg.tar");
        fs::write(&archive, sample_tar()).unwrap();
        let target = dir.join("out");
        fs::create_dir_all(&target).unwrap();

        extract_tar(&archive, &target, ArchiveFormat::Tar, Some("pkg-1.0")).unwrap();
        assert_extracted(&target);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tar_gz_round_trips() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let dir = scratch("targz");
        let archive = dir.join("pkg.tar.gz");
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&sample_tar()).unwrap();
        fs::write(&archive, enc.finish().unwrap()).unwrap();
        let target = dir.join("out");
        fs::create_dir_all(&target).unwrap();

        extract_tar(&archive, &target, ArchiveFormat::TarGz, Some("pkg-1.0")).unwrap();
        assert_extracted(&target);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tar_xz_round_trips() {
        let dir = scratch("tarxz");
        let archive = dir.join("pkg.tar.xz");
        let mut compressed = Vec::new();
        lzma_rs::xz_compress(&mut std::io::Cursor::new(sample_tar()), &mut compressed).unwrap();
        fs::write(&archive, &compressed).unwrap();
        let target = dir.join("out");
        fs::create_dir_all(&target).unwrap();

        // MySQL's Linux build is the reason this path exists at all.
        extract_tar(&archive, &target, ArchiveFormat::TarXz, Some("pkg-1.0")).unwrap();
        assert_extracted(&target);
        // The intermediate decompressed tar must not be left lying around.
        assert!(!dir.join("pkg.tar.untar").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extraction_without_a_root_to_strip_keeps_the_layout() {
        let dir = scratch("noroot");
        let archive = dir.join("pkg.tar");
        fs::write(&archive, sample_tar()).unwrap();
        let target = dir.join("out");
        fs::create_dir_all(&target).unwrap();

        extract_tar(&archive, &target, ArchiveFormat::Tar, None).unwrap();
        assert!(target.join("pkg-1.0/bin/tool").is_file());
        fs::remove_dir_all(&dir).ok();
    }

    /// Hits the network, so it stays out of the default run and out of CI.
    /// Exercise it deliberately with:
    ///   cargo test --lib -- --ignored real_tarball
    #[test]
    #[ignore = "network"]
    fn real_upstream_tarball_extracts() {
        let dir = scratch("real");
        let archive = dir.join("nginx-1.30.0.tar.gz");
        // nginx publishes only source for Unix, but it is a genuine upstream
        // gzip tarball with the usual single root directory — exactly the
        // shape the extractor has to cope with.
        fetch_to_file(
            "https://nginx.org/download/nginx-1.30.0.tar.gz",
            &archive,
            "058188C64BF22BAECAA72B809A6318A4F9BA623889C554FEAB03F7CB853AB31B",
            "nginx-src",
            None,
        )
        .expect("download + sha");

        let target = dir.join("out");
        fs::create_dir_all(&target).unwrap();
        extract_tar(&archive, &target, ArchiveFormat::TarGz, Some("nginx-1.30.0")).unwrap();

        assert!(target.join("conf/nginx.conf").is_file());
        assert!(target.join("src/core/nginx.c").is_file());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn traversal_and_absolute_paths_are_refused() {
        assert!(is_safe_relative("bin/mysqld"));
        assert!(!is_safe_relative(""));
        assert!(!is_safe_relative("."));
        assert!(!is_safe_relative("../escape"));
        assert!(!is_safe_relative("bin/../../escape"));
        #[cfg(windows)]
        assert!(!is_safe_relative("C:/Windows/System32/evil.dll"));
        #[cfg(unix)]
        assert!(!is_safe_relative("/etc/passwd"));
    }
}
