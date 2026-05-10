//! Local Certificate Authority + per-host leaf certs.
//!
//! Phase 2.2: rcgen creates a self-signed root CA (10-year) on first run,
//! then issues per-host leaf certs (1-year) signed by it. Browsers trust the
//! whole chain once the CA is installed in the OS root store — see
//! `ssl::trust` for that.

pub mod trust;

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use std::fs;
use std::path::{Path, PathBuf};

pub const SSL_PORT: u16 = 8443;

pub struct LocalCa {
    pub dir: PathBuf,
}

pub struct LeafCert {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

impl LocalCa {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn cert_path(&self) -> PathBuf {
        self.dir.join("ca.crt")
    }

    pub fn key_path(&self) -> PathBuf {
        self.dir.join("ca.key")
    }

    /// Generate the CA on disk if it doesn't already exist.
    pub fn ensure(&self) -> Result<(), String> {
        if self.cert_path().exists() && self.key_path().exists() {
            return Ok(());
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;

        let kp = KeyPair::generate().map_err(|e| format!("CA keypair: {e}"))?;
        let mut params = CertificateParams::new(Vec::<String>::new())
            .map_err(|e| format!("CA params: {e}"))?;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, "Lamp Bench Local CA");
        params
            .distinguished_name
            .push(DnType::OrganizationName, "Lamp Bench");
        params.not_before = time::OffsetDateTime::now_utc();
        params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(3650);
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        let cert = params
            .self_signed(&kp)
            .map_err(|e| format!("CA self-sign: {e}"))?;

        fs::write(self.cert_path(), cert.pem()).map_err(|e| e.to_string())?;
        fs::write(self.key_path(), kp.serialize_pem()).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Issue (or reuse) a leaf cert for `hostname` under `out_dir`. Stable —
    /// same hostname keeps the same cert across restarts unless deleted.
    pub fn issue_leaf(&self, hostname: &str, out_dir: &Path) -> Result<LeafCert, String> {
        fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
        let cert_path = out_dir.join(format!("{hostname}.crt"));
        let key_path = out_dir.join(format!("{hostname}.key"));
        if cert_path.exists() && key_path.exists() {
            return Ok(LeafCert {
                cert_path,
                key_path,
            });
        }

        let ca_cert_pem = fs::read_to_string(self.cert_path())
            .map_err(|e| format!("read CA cert: {e}"))?;
        let ca_key_pem = fs::read_to_string(self.key_path())
            .map_err(|e| format!("read CA key: {e}"))?;
        let ca_kp = KeyPair::from_pem(&ca_key_pem).map_err(|e| format!("parse CA key: {e}"))?;
        let ca_params = CertificateParams::from_ca_cert_pem(&ca_cert_pem)
            .map_err(|e| format!("parse CA cert: {e}"))?;
        let ca_cert = ca_params
            .self_signed(&ca_kp)
            .map_err(|e| format!("rebuild CA cert: {e}"))?;

        let leaf_kp = KeyPair::generate().map_err(|e| format!("leaf keypair: {e}"))?;
        let mut params = CertificateParams::new(vec![hostname.to_string()])
            .map_err(|e| format!("leaf params: {e}"))?;
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, hostname);
        params.not_before = time::OffsetDateTime::now_utc();
        params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(365);
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

        let leaf = params
            .signed_by(&leaf_kp, &ca_cert, &ca_kp)
            .map_err(|e| format!("leaf sign: {e}"))?;

        fs::write(&cert_path, leaf.pem()).map_err(|e| e.to_string())?;
        fs::write(&key_path, leaf_kp.serialize_pem()).map_err(|e| e.to_string())?;

        Ok(LeafCert {
            cert_path,
            key_path,
        })
    }
}
