//! ionCube Loader installer.
//!
//! ionCube is a proprietary PHP encoder loader (XenForo, WHMCS, many
//! commercial scripts ship ionCube-encoded files). It's a `zend_extension`,
//! not a regular `extension`, and the correct loader `.dll`/`.so` depends on
//! the OS, CPU arch, PHP major.minor AND the thread-safety mode.
//!
//! **SHA pinning exception.** Every other binary in Lamp Bench is pinned with
//! a SHA256 in `binaries.json`. ionCube can't be: its download URLs are
//! unversioned ("latest") and it publishes no stable checksums. We fetch over
//! HTTPS from the official `downloads.ioncube.com` host, which is the best
//! integrity guarantee available for this dependency. It's opt-in and only
//! runs when the user explicitly asks for it.
//!
//! Lamp Bench uses NTS PHP (mod_fcgid / php-cgi), so we pick the `_nonts`
//! loader on Windows.

use std::fs;
use std::io::Read;
use std::path::Path;

/// ionCube Windows package suffix (Visual C runtime) for a PHP major.minor.
/// VC15 = PHP 7.2–7.4, VC16 = 8.0–8.3, VC17 = 8.4+. Unknown/newer versions
/// default to the latest known (vc17).
fn vc_tag_for(php_version: &str) -> &'static str {
    let mut parts = php_version.split('.');
    let major: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(8);
    let minor: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(4);
    match (major, minor) {
        (7, _) => "vc15",
        (8, 0..=3) => "vc16",
        _ => "vc17",
    }
}

/// Install the ionCube loader for `php_version` (e.g. "8.4") into the given
/// PHP install dir, and register it as a `zend_extension` in that version's
/// `php.ini`. Idempotent — re-running overwrites the loader and won't add a
/// duplicate php.ini line.
pub fn install(php_version: &str, php_dir: &Path, platform: &str) -> Result<(), String> {
    if !php_dir.is_dir() {
        return Err(format!(
            "PHP {php_version} is not installed (missing {})",
            php_dir.display()
        ));
    }

    let (url, loader_match) = match platform {
        "windows-x64" => {
            // ionCube packages Windows loaders by the Visual C runtime the PHP
            // build used: VC15 (PHP 7.2–7.4), VC16 (8.1–8.3), VC17 (8.4+).
            // The single `ioncube_loader_win_X.Y.dll` in each package is the
            // NTS build (what we run via php-cgi / mod_fcgid) — modern ionCube
            // dropped the `_nonts` suffix.
            let vc = vc_tag_for(php_version);
            (
                format!(
                    "https://downloads.ioncube.com/loader_downloads/ioncube_loaders_win_{vc}_x86-64.zip"
                ),
                format!("ioncube_loader_win_{php_version}.dll"),
            )
        }
        other => {
            return Err(format!(
                "ionCube auto-install is only wired for Windows right now (got {other}). \
                 Linux/macOS PHP binaries aren't bundled yet."
            ))
        }
    };

    // Download (HTTPS, ureq/rustls). No SHA — see module docs. A browser-ish
    // User-Agent matters: ionCube's CDN serves an HTML interstitial (with a
    // 200!) to the default `ureq/x.y` agent, which then fails to parse as a
    // zip with a cryptic "Could not find EOCD".
    let resp = ureq::get(&url)
        .set(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) LampBench/1.0",
        )
        .call()
        .map_err(|e| format!("download ionCube: {e}"))?;
    if resp.status() != 200 {
        return Err(format!("ionCube download HTTP {}", resp.status()));
    }
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| format!("read ionCube zip: {e}"))?;

    // Sanity-check the magic bytes before handing to the zip reader, so a
    // server that returns an HTML error/redirect page (status 200) yields a
    // clear message instead of "Could not find EOCD".
    if buf.len() < 4 || &buf[..2] != b"PK" {
        let preview: String = buf
            .iter()
            .take(120)
            .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' })
            .collect();
        return Err(format!(
            "ionCube download wasn't a zip ({} bytes). The server likely returned \
             an error page. First bytes: {preview}",
            buf.len()
        ));
    }

    // Pull just the one loader matching this PHP version out of the archive.
    let cursor = std::io::Cursor::new(buf);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("open ionCube zip: {e}"))?;
    let ext_dir = php_dir.join("ext");
    fs::create_dir_all(&ext_dir).map_err(|e| e.to_string())?;

    let mut installed_path: Option<std::path::PathBuf> = None;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("zip entry {i}: {e}"))?;
        let name = file.name().replace('\\', "/");
        if name.ends_with(&loader_match) {
            let target = ext_dir.join(&loader_match);
            let mut out = fs::File::create(&target).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
            installed_path = Some(target);
            break;
        }
    }

    let loader_path = installed_path.ok_or_else(|| {
        format!(
            "no ionCube loader for PHP {php_version} in the package \
             (looked for {loader_match}) — that PHP version may not be supported yet"
        )
    })?;

    register_zend_extension(php_dir, &loader_path)?;
    Ok(())
}

/// Ensure the version's php.ini loads the ionCube loader as a zend_extension.
/// Creates php.ini from php.ini-development if it doesn't exist yet.
fn register_zend_extension(php_dir: &Path, loader_path: &Path) -> Result<(), String> {
    let ini = php_dir.join("php.ini");
    if !ini.exists() {
        let template = php_dir.join("php.ini-development");
        if template.exists() {
            fs::copy(&template, &ini).map_err(|e| format!("seed php.ini: {e}"))?;
        } else {
            fs::write(&ini, "").map_err(|e| e.to_string())?;
        }
    }

    let current = fs::read_to_string(&ini).map_err(|e| format!("read php.ini: {e}"))?;
    // ionCube wants a forward-slash path; backslashes in php.ini are escapes.
    let path_str = loader_path.display().to_string().replace('\\', "/");

    // Idempotency: skip if any ioncube zend_extension line is already present.
    if current
        .lines()
        .any(|l| l.contains("ioncube") && l.contains("zend_extension"))
    {
        return Ok(());
    }

    // ionCube must be the FIRST zend_extension (before OPcache). Prepend it.
    let block = format!(
        "; --- Lamp Bench: ionCube loader (must precede other zend_extensions) ---\n\
         zend_extension = \"{path_str}\"\n\n"
    );
    let updated = format!("{block}{current}");
    fs::write(&ini, updated).map_err(|e| format!("write php.ini: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vc_tag_mapping() {
        assert_eq!(vc_tag_for("7.4"), "vc15");
        assert_eq!(vc_tag_for("8.1"), "vc16");
        assert_eq!(vc_tag_for("8.3"), "vc16");
        assert_eq!(vc_tag_for("8.4"), "vc17");
        assert_eq!(vc_tag_for("8.5"), "vc17");
        // unknown/newer falls back to the latest known package
        assert_eq!(vc_tag_for("9.0"), "vc17");
    }
}
