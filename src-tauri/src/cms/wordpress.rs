use super::{install_files_and_db, sanitize_db_name};
use rand::Rng;
use std::fs;
use std::path::{Path, PathBuf};

pub struct InstallRequest {
    pub site_name: String,
    pub hostname: String,
    pub parent_dir: PathBuf,
    pub mysql_dir: PathBuf,
    pub mysql_port: u16,
    pub source_dir: PathBuf,
}

pub struct InstallResult {
    pub docroot: PathBuf,
    #[allow(dead_code)]
    pub db_name: String,
}

pub fn install(req: &InstallRequest) -> Result<InstallResult, String> {
    let target = req.parent_dir.join(&req.site_name);
    let db_name = sanitize_db_name("wp", &req.site_name);

    install_files_and_db(
        &req.source_dir,
        &target,
        &req.mysql_dir,
        req.mysql_port,
        &db_name,
    )?;
    write_wp_config(&target, &db_name, req.mysql_port)?;

    Ok(InstallResult {
        docroot: target,
        db_name,
    })
}

fn write_wp_config(target: &Path, db_name: &str, mysql_port: u16) -> Result<(), String> {
    let sample = target.join("wp-config-sample.php");
    let config = target.join("wp-config.php");
    let mut content = fs::read_to_string(&sample)
        .map_err(|e| format!("read wp-config-sample.php: {e}"))?;

    content = content
        .replace("database_name_here", db_name)
        .replace("username_here", "root")
        .replace("password_here", "")
        .replace("localhost", &format!("127.0.0.1:{mysql_port}"));

    for _ in 0..8 {
        let salt = random_salt();
        content = content.replacen(
            "'put your unique phrase here'",
            &format!("'{salt}'"),
            1,
        );
    }

    fs::write(&config, content).map_err(|e| format!("write wp-config.php: {e}"))?;
    Ok(())
}

fn random_salt() -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()_+-=[]{}|:,.<>?";
    let mut rng = rand::thread_rng();
    (0..64)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect()
}
