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
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct PhpExtension {
    pub name: String,
    pub enabled: bool,
}

/// Returns the php.ini path for a version, seeding it from
/// `php.ini-development` on first access so toggles have something to edit.
fn ensure_ini(php_dir: &Path) -> Result<std::path::PathBuf, String> {
    let ini = php_dir.join("php.ini");
    if !ini.exists() {
        let template = php_dir.join("php.ini-development");
        if template.exists() {
            fs::copy(&template, &ini).map_err(|e| format!("seed php.ini: {e}"))?;
        } else {
            return Err(format!("no php.ini or php.ini-development in {}", php_dir.display()));
        }
    }
    Ok(ini)
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
    // Normalise php_curl.dll / curl → curl for display + matching.
    let name = value
        .trim_matches('"')
        .trim_start_matches("php_")
        .trim_end_matches(".dll")
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
    }

    #[test]
    fn ignores_unrelated_lines() {
        assert_eq!(parse_extension_line("; just a comment"), None);
        assert_eq!(parse_extension_line("memory_limit = 128M"), None);
        assert_eq!(parse_extension_line("zend_extension=opcache"), None);
    }
}
