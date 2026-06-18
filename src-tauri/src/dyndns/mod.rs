//! Dynamic DNS updates over the `dyndns2` protocol.
//!
//! No-IP, dyn.com, DNS-O-Matic, easyDNS and spDYN all speak the same simple
//! HTTP scheme: a GET to `<host>/nic/update?hostname=<name>` with HTTP Basic
//! auth. Omitting `myip` makes the provider use the connecting (public) IP,
//! so we don't have to discover our own external address. The response body
//! is a short status code (`good`, `nochg`, `badauth`, `nohost`, …).
//!
//! No OAuth, no SDK — just `ureq` (rustls). Cloud-storage sync (Drive /
//! OneDrive / Dropbox) is a separate, heavier effort and lives in `cloud`.

use serde::Serialize;

/// Maps a provider id (as sent from the UI dropdown) to its dyndns2 update
/// host. Unknown ids are rejected so a typo can't silently no-op.
fn update_host(provider: &str) -> Result<&'static str, String> {
    Ok(match provider {
        "noip" => "dynupdate.no-ip.com",
        "dyn" => "members.dyndns.org",
        "dnsomatic" => "updates.dnsomatic.com",
        "easydns" => "api.cp.easydns.com",
        "spdyn" => "update.spdyn.de",
        other => return Err(format!("unknown DynDNS provider: {other}")),
    })
}

#[derive(Debug, Serialize)]
pub struct DynDnsResult {
    /// Raw status line returned by the provider (e.g. "good 1.2.3.4").
    pub status: String,
    /// Whether the status indicates success (`good` or `nochg`).
    pub ok: bool,
}

pub fn update(
    provider: &str,
    hostname: &str,
    user: &str,
    password: &str,
) -> Result<DynDnsResult, String> {
    let hostname = hostname.trim();
    let user = user.trim();
    if hostname.is_empty() {
        return Err("hostname is required".into());
    }
    if user.is_empty() {
        return Err("username is required".into());
    }
    let host = update_host(provider)?;
    let url = format!("https://{host}/nic/update?hostname={hostname}");

    // dyndns2 mandates a descriptive User-Agent; providers reject the default
    // ureq one. Basic auth carries the credentials.
    let auth = base64_basic(user, password);
    let resp = ureq::get(&url)
        .set("Authorization", &format!("Basic {auth}"))
        .set("User-Agent", "LampBench/1.0 dyndns")
        .call()
        .map_err(|e| format!("dyndns request failed: {e}"))?;

    let body = resp
        .into_string()
        .map_err(|e| format!("read response: {e}"))?;
    let status = body.trim().to_string();
    let ok = status.starts_with("good") || status.starts_with("nochg");
    Ok(DynDnsResult { status, ok })
}

/// Minimal base64 for the Basic auth header. Avoids pulling a base64 crate
/// just for one header — the input is short (user:pass).
fn base64_basic(user: &str, password: &str) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = format!("{user}:{password}");
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        // "user:pass" -> dXNlcjpwYXNz
        assert_eq!(base64_basic("user", "pass"), "dXNlcjpwYXNz");
        // "a:b" -> YTpi
        assert_eq!(base64_basic("a", "b"), "YTpi");
    }

    #[test]
    fn unknown_provider_rejected() {
        assert!(update_host("bogus").is_err());
        assert!(update_host("noip").is_ok());
    }
}
