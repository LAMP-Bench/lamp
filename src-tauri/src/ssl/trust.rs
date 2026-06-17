//! Install the Lamp Bench CA into the platform's user trust store.
//!
//! Windows: `CurrentUser\Root` via PowerShell. No UAC. Chrome/Edge use it.
//! macOS: `login.keychain-db` via `security add-trusted-cert`. No admin needed.
//! Linux: best-effort copy to `/usr/local/share/ca-certificates/` + refresh.
//!        Needs pkexec/sudo because the directory is root-owned.
//!
//! Firefox keeps its own NSS database on every platform and either needs
//! `security.enterprise_roots.enabled = true` flipped in `about:config`
//! or a manual import of `ca.crt`. The UI surfaces that as a heads-up
//! rather than try to manipulate Firefox's profile from here.

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
use crate::services::hidden_command;
use std::path::Path;

/// Idempotent. Returns `Ok(true)` when a new install happened, `Ok(false)`
/// when the cert was already present, `Err` on failure.
#[cfg(windows)]
pub fn ensure_trusted(ca_cert_path: &Path) -> Result<bool, String> {
    let cert_path_arg = ca_cert_path
        .display()
        .to_string()
        .replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference = 'Stop'\r\n\
         $cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2 -ArgumentList '{cert_path_arg}'\r\n\
         $store = New-Object System.Security.Cryptography.X509Certificates.X509Store('Root','CurrentUser')\r\n\
         $store.Open('ReadWrite')\r\n\
         $existing = $store.Certificates | Where-Object Thumbprint -eq $cert.Thumbprint\r\n\
         if (-not $existing) {{\r\n\
         \x20   $store.Add($cert)\r\n\
         \x20   Write-Output 'INSTALLED'\r\n\
         }} else {{\r\n\
         \x20   Write-Output 'PRESENT'\r\n\
         }}\r\n\
         $store.Close()"
    );
    let output = hidden_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("spawn powershell: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).contains("INSTALLED"))
}

#[cfg(target_os = "macos")]
pub fn ensure_trusted(ca_cert_path: &Path) -> Result<bool, String> {
    // `security verify-cert` exits non-zero if the cert isn't trusted yet —
    // we use that as the idempotency check instead of parsing keychain dumps.
    let already = hidden_command("security")
        .args(["verify-cert", "-c"])
        .arg(ca_cert_path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if already {
        return Ok(false);
    }
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let keychain = format!("{home}/Library/Keychains/login.keychain-db");
    let output = hidden_command("security")
        .args(["add-trusted-cert", "-r", "trustRoot", "-k", &keychain])
        .arg(ca_cert_path)
        .output()
        .map_err(|e| format!("spawn security: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "security add-trusted-cert failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(true)
}

#[cfg(target_os = "linux")]
pub fn ensure_trusted(ca_cert_path: &Path) -> Result<bool, String> {
    // /etc/ssl/certs/ca-certificates.crt is the concatenated trust bundle on
    // Debian/Ubuntu. update-ca-certificates rebuilds it from .crt files in
    // /usr/local/share/ca-certificates/. Fedora/RHEL use a different path
    // (/etc/pki/ca-trust/source/anchors + update-ca-trust); we try both.
    let target_paths: &[(&str, &str)] = &[
        ("/usr/local/share/ca-certificates/lamp-bench.crt", "update-ca-certificates"),
        ("/etc/pki/ca-trust/source/anchors/lamp-bench.crt", "update-ca-trust"),
    ];

    let mut last_err = String::from("no supported trust store path found");
    for (dest, refresh_cmd) in target_paths {
        // Skip distros that don't have the parent dir to avoid touching paths
        // owned by some other package manager unexpectedly.
        let parent = std::path::Path::new(dest).parent().unwrap();
        if !parent.exists() {
            continue;
        }
        // Already present + same content? Done.
        if let (Ok(a), Ok(b)) = (std::fs::read(dest), std::fs::read(ca_cert_path)) {
            if a == b {
                return Ok(false);
            }
        }
        let runner = pick_elevation()?;
        let mut cp = hidden_command(&runner);
        if runner == "sudo" { cp.arg("-n"); }
        cp.arg("cp").arg(ca_cert_path).arg(dest);
        let out = cp.output().map_err(|e| format!("spawn {runner}: {e}"))?;
        if !out.status.success() {
            last_err = format!(
                "{runner} cp failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            continue;
        }
        let mut refresh = hidden_command(&runner);
        if runner == "sudo" { refresh.arg("-n"); }
        refresh.arg(refresh_cmd);
        let _ = refresh.output();
        return Ok(true);
    }
    Err(format!("trust store install failed: {last_err}"))
}

#[cfg(target_os = "linux")]
fn pick_elevation() -> Result<String, String> {
    for runner in ["pkexec", "sudo"] {
        if hidden_command("which")
            .arg(runner)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Ok(runner.to_string());
        }
    }
    Err("need pkexec or sudo to install the CA".into())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn ensure_trusted(_path: &Path) -> Result<bool, String> {
    Err("trust store install is not supported on this OS".into())
}
