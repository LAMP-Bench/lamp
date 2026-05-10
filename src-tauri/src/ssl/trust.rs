//! Install the Lamp Bench CA into the platform's user trust store.
//!
//! On Windows we install to `CurrentUser\Root` — no UAC needed and it is the
//! store Chrome/Edge consult. Firefox keeps its own NSS database and must
//! either flip `security.enterprise_roots.enabled = true` or import the CA
//! manually. We surface that as a heads-up in the UI rather than try to
//! manipulate Firefox's profile.

#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
use std::process::Command;

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
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("spawn powershell: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).contains("INSTALLED"))
}

#[cfg(not(windows))]
pub fn ensure_trusted(_path: &std::path::Path) -> Result<bool, String> {
    Err("trust store install on this OS is implemented in Phase 9".into())
}
