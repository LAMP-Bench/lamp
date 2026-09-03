//! PHP extension management: list and toggle the `extension=` lines in a
//! version's `php.ini`.
//!
//! Windows PHP ships every bundled extension as a commented-out
//! `;extension=name` line in `php.ini-development`. Enabling one is just
//! uncommenting it; disabling is re-commenting. We never touch
//! `zend_extension` lines here — those (OPcache, Xdebug, ionCube) have load
//! ordering constraints and are managed elsewhere.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct PhpExtension {
    pub name: String,
    pub enabled: bool,
}

/// Extensions Lamp Bench switches on the first time it materialises a
/// `php.ini`. They're uncommented **in the template's own body**, not
/// appended, so the Versions panel sees one line per extension and the user
/// stays in charge of them afterwards.
const DEFAULT_EXTENSIONS: [&str; 10] = [
    "mysqli",
    "pdo_mysql",
    "curl",
    "mbstring",
    "openssl",
    "gd",
    "intl",
    "zip",
    "fileinfo",
    "exif",
];

const BLOCK_BEGIN: &str = "; === Lamp Bench managed — do not edit between these markers ===";
const BLOCK_END: &str = "; === Lamp Bench end ===";

/// Header of the pre-marker block Lamp Bench used to append. It was written
/// once and never refreshed, and it carried its own `extension=` lines that
/// fought with the Versions panel. Recognised here purely so upgrading
/// installs can have it removed.
const LEGACY_BLOCK_HEADER: &str = "; --- Lamp Bench overrides ---";

/// Returns the php.ini path for a version, seeding it from
/// `php.ini-development` on first access so toggles have something to edit.
fn ensure_ini(php_dir: &Path) -> Result<PathBuf, String> {
    let ini = php_dir.join("php.ini");
    if !ini.exists() {
        let template = php_dir.join("php.ini-development");
        if !template.exists() {
            return Err(format!(
                "no php.ini or php.ini-development in {}",
                php_dir.display()
            ));
        }
        let seeded = enable_extensions(
            &fs::read_to_string(&template).map_err(|e| format!("read php.ini-development: {e}"))?,
            &DEFAULT_EXTENSIONS,
        );
        fs::write(&ini, seeded).map_err(|e| format!("seed php.ini: {e}"))?;
    }
    Ok(ini)
}

/// Materialise `php.ini` if missing and bring the Lamp Bench block up to date.
///
/// Two bugs live here historically, both fixed by making this the single
/// entry point:
///
/// * the settings block was only ever written when the file was created, so
///   changing MailHog's SMTP port left `php.ini` pointing at the old one and
///   no existing install ever picked up new defaults;
/// * opening Versions → extensions before Apache had started created a bare
///   `php.ini`, after which the block was never added at all — that PHP
///   version silently lost `extension_dir`, mysqli and Xdebug forever.
///
/// The block is delimited and rewritten every call, so it is self-healing
/// regardless of which code path gets there first. Everything outside the
/// markers — including the user's extension toggles — is left alone.
///
/// A PHP version listed but not on disk is skipped rather than failing:
/// making one absent optional version abort the whole Apache start is what
/// bricked the v0.1.0 installer's service toggles.
pub fn ensure_managed_ini(php_dir: &Path, smtp_port: u16) -> Result<(), String> {
    let ini = php_dir.join("php.ini");
    if !ini.exists() && !php_dir.join("php.ini-development").exists() {
        return Ok(());
    }
    let ini = ensure_ini(php_dir)?;

    let current = fs::read_to_string(&ini).map_err(|e| format!("read php.ini: {e}"))?;
    let updated = apply_managed_block(&current, &managed_block(php_dir, smtp_port));
    if updated != current {
        fs::write(&ini, updated).map_err(|e| format!("write php.ini: {e}"))?;
    }
    Ok(())
}

/// The settings Lamp Bench owns. Deliberately no `extension=` lines: those
/// belong to the user via the Versions panel, and a block that rewrites
/// itself on every service start would undo their toggles.
fn managed_block(php_dir: &Path, smtp_port: u16) -> String {
    let ext_dir = php_dir.join("ext").to_string_lossy().replace('\\', "/");
    format!(
        "{BLOCK_BEGIN}\n\
         extension_dir = \"{ext_dir}\"\n\
         \n\
         ; OPcache (a Zend extension, not a regular one)\n\
         zend_extension=opcache\n\
         opcache.enable=1\n\
         opcache.enable_cli=0\n\
         \n\
         ; Xdebug 3 — develop mode is always on (pretty errors); the debugger\n\
         ; only attaches when the request carries an XDEBUG_TRIGGER\n\
         ; cookie/GET/POST. Use the IDE's \"Listen for Xdebug\" button plus a\n\
         ; browser extension to step through code.\n\
         zend_extension=xdebug\n\
         xdebug.mode=develop,debug\n\
         xdebug.start_with_request=trigger\n\
         xdebug.client_host=127.0.0.1\n\
         xdebug.client_port=9003\n\
         xdebug.discover_client_host=0\n\
         \n\
         ; Route mail() at MailHog instead of trying to deliver, so the user\n\
         ; can read what their app sent in the MailHog web UI.\n\
         [mail function]\n\
         SMTP = 127.0.0.1\n\
         smtp_port = {smtp_port}\n\
         sendmail_from = noreply@localhost\n\
         {BLOCK_END}\n"
    )
}

/// Swap the managed block for `block`, appending it when there isn't one yet.
/// Also strips the legacy pre-marker block so upgrading installs don't end up
/// loading Xdebug and OPcache twice.
fn apply_managed_block(current: &str, block: &str) -> String {
    let cleaned = strip_legacy_block(current);

    match (cleaned.find(BLOCK_BEGIN), cleaned.find(BLOCK_END)) {
        (Some(begin), Some(end)) if end > begin => {
            let before = &cleaned[..begin];
            let mut tail = &cleaned[end + BLOCK_END.len()..];
            tail = tail.strip_prefix("\r\n").unwrap_or(tail);
            tail = tail.strip_prefix('\n').unwrap_or(tail);
            format!("{before}{block}{tail}")
        }
        _ => {
            let mut out = cleaned;
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
            out.push_str(block);
            out
        }
    }
}

/// The legacy block ran from its header to the end of the file — it was
/// always the last thing appended.
fn strip_legacy_block(current: &str) -> String {
    match current.find(LEGACY_BLOCK_HEADER) {
        Some(at) => current[..at].trim_end().to_string(),
        None => current.to_string(),
    }
}

/// Uncomment `wanted` extensions where the template already lists them. Lines
/// that aren't present are left alone — we don't invent `extension=` entries
/// for DLLs the build doesn't ship.
fn enable_extensions(content: &str, wanted: &[&str]) -> String {
    let lines: Vec<String> = content
        .lines()
        .map(|line| match parse_extension_line(line) {
            Some((name, false)) if wanted.contains(&name.as_str()) => line
                .trim_start()
                .trim_start_matches(';')
                .trim_start()
                .to_string(),
            _ => line.to_string(),
        })
        .collect();
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Parse a line into (extension_name, enabled) if it's an `extension=` line
/// (commented or not). Returns None for unrelated lines. Handles both
/// `extension=curl` and `extension=php_curl.dll` forms, and leading `;`.
fn parse_extension_line(raw: &str) -> Option<(String, bool)> {
    let trimmed = raw.trim();
    let (enabled, body) = match trimmed.strip_prefix(';') {
        Some(rest) => (false, rest.trim_start()),
        None => (true, trimmed),
    };
    let rest = body.strip_prefix("extension")?.trim_start();
    let value = rest.strip_prefix('=')?.trim();
    if value.is_empty() {
        return None;
    }
    // Normalise php_curl.dll / curl.so / curl → curl for display + matching.
    let name = value
        .trim_matches('"')
        .trim_start_matches("php_")
        .trim_end_matches(".dll")
        .trim_end_matches(".so")
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some((name, enabled))
    }
}

pub fn list_extensions(php_dir: &Path) -> Result<Vec<PhpExtension>, String> {
    let ini = ensure_ini(php_dir)?;
    let content = fs::read_to_string(&ini).map_err(|e| format!("read php.ini: {e}"))?;
    let mut out: Vec<PhpExtension> = Vec::new();
    for line in content.lines() {
        if let Some((name, enabled)) = parse_extension_line(line) {
            // Last occurrence wins, but de-dupe by name keeping enabled-OR.
            if let Some(existing) = out.iter_mut().find(|e| e.name == name) {
                existing.enabled = existing.enabled || enabled;
            } else {
                out.push(PhpExtension { name, enabled });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Enable or disable an extension by (un)commenting its line(s). Returns an
/// error if no matching line exists — we don't invent extension lines for
/// DLLs that aren't shipped.
pub fn toggle_extension(php_dir: &Path, name: &str, enable: bool) -> Result<(), String> {
    let ini = ensure_ini(php_dir)?;
    let content = fs::read_to_string(&ini).map_err(|e| format!("read php.ini: {e}"))?;
    let mut changed = false;
    let new_lines: Vec<String> = content
        .lines()
        .map(|line| match parse_extension_line(line) {
            Some((ext, currently_enabled)) if ext == name => {
                changed = true;
                if enable && !currently_enabled {
                    // Drop the leading ';' (and any space after it).
                    line.trim_start()
                        .trim_start_matches(';')
                        .trim_start()
                        .to_string()
                } else if !enable && currently_enabled {
                    format!(";{}", line.trim_start())
                } else {
                    line.to_string()
                }
            }
            _ => line.to_string(),
        })
        .collect();
    if !changed {
        return Err(format!("extension '{name}' not found in php.ini"));
    }
    fs::write(&ini, new_lines.join("\n")).map_err(|e| format!("write php.ini: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commented_and_active() {
        assert_eq!(parse_extension_line("extension=curl"), Some(("curl".into(), true)));
        assert_eq!(parse_extension_line(";extension=gd"), Some(("gd".into(), false)));
        assert_eq!(
            parse_extension_line("extension=php_mbstring.dll"),
            Some(("mbstring".into(), true))
        );
        assert_eq!(parse_extension_line(";  extension = \"intl\""), Some(("intl".into(), false)));
        // Linux/macOS spelling, same extension, different suffix.
        assert_eq!(
            parse_extension_line("extension=curl.so"),
            Some(("curl".into(), true))
        );
    }

    #[test]
    fn block_is_appended_then_replaced_in_place() {
        let base = "[PHP]\nmemory_limit = 128M\n";
        let once = apply_managed_block(base, &managed_block(Path::new("C:/php-8.4"), 1025));
        assert!(once.starts_with("[PHP]\nmemory_limit = 128M\n"));
        assert!(once.contains("smtp_port = 1025"));

        // A second pass with a different port must swap the block, not stack
        // another one on top of it.
        let twice = apply_managed_block(&once, &managed_block(Path::new("C:/php-8.4"), 2525));
        assert_eq!(twice.matches(BLOCK_BEGIN).count(), 1);
        assert_eq!(twice.matches("zend_extension=xdebug").count(), 1);
        assert!(twice.contains("smtp_port = 2525"));
        assert!(!twice.contains("smtp_port = 1025"));
        assert!(twice.starts_with("[PHP]\nmemory_limit = 128M\n"));
    }

    #[test]
    fn block_rewrite_is_idempotent() {
        let block = managed_block(Path::new("C:/php-8.4"), 1025);
        let once = apply_managed_block("[PHP]\n", &block);
        assert_eq!(once, apply_managed_block(&once, &block));
    }

    #[test]
    fn legacy_block_is_migrated_not_duplicated() {
        // What installs before the markers existed look like on disk.
        let legacy = "[PHP]\nmemory_limit = 128M\n\n\
                      ; --- Lamp Bench overrides ---\n\
                      extension_dir = \"C:/php-8.4/ext\"\n\
                      extension=mysqli\n\
                      zend_extension=opcache\n\
                      zend_extension=xdebug\n\
                      smtp_port = 1025\n";
        let out = apply_managed_block(legacy, &managed_block(Path::new("C:/php-8.4"), 1025));
        // Loading Xdebug twice is a startup warning, so the old copy has to go.
        assert_eq!(out.matches("zend_extension=xdebug").count(), 1);
        assert_eq!(out.matches("zend_extension=opcache").count(), 1);
        assert!(!out.contains(LEGACY_BLOCK_HEADER));
        // The user's own settings survive.
        assert!(out.contains("memory_limit = 128M"));
    }

    /// Whether `out` has an exact line equal to `line` — string `contains`
    /// gives false positives here, since `extension=gd` is a substring of
    /// `;extension=gd`.
    fn has_line(out: &str, line: &str) -> bool {
        out.lines().any(|l| l == line)
    }

    #[test]
    fn user_toggles_outside_the_block_survive_a_rewrite() {
        let block = managed_block(Path::new("C:/php-8.4"), 1025);
        let seeded = apply_managed_block(";extension=gd\nextension=mysqli\n", &block);
        // User turns gd on and mysqli off in the Versions panel.
        let edited = seeded
            .replace(";extension=gd", "extension=gd")
            .replace("extension=mysqli", ";extension=mysqli");
        let after = apply_managed_block(&edited, &block);
        assert!(has_line(&after, "extension=gd"));
        assert!(has_line(&after, ";extension=mysqli"));
    }

    #[test]
    fn seeding_uncomments_only_the_defaults_present() {
        let template = ";extension=gd\n;extension=mysqli\n;extension=snmp\nmemory_limit = 128M\n";
        let out = enable_extensions(template, &DEFAULT_EXTENSIONS);
        assert!(has_line(&out, "extension=gd"));
        assert!(has_line(&out, "extension=mysqli"));
        // Not in our default set — left exactly as the template had it.
        assert!(has_line(&out, ";extension=snmp"));
        assert!(has_line(&out, "memory_limit = 128M"));
    }

    #[test]
    fn ignores_unrelated_lines() {
        assert_eq!(parse_extension_line("; just a comment"), None);
        assert_eq!(parse_extension_line("memory_limit = 128M"), None);
        assert_eq!(parse_extension_line("zend_extension=opcache"), None);
    }
}
