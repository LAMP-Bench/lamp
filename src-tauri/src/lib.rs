mod cloud;
mod cms;
mod config_gen;
mod db;
mod deploy;
mod downloads;
mod dyndns;
mod ioncube;
mod hosts;
mod images;
mod php;
mod services;
mod snapshots;
pub mod ssl;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use hosts::Host;
use rusqlite::Connection;
use serde::Serialize;
use services::apache::{ApacheService, PhpInstall};
use services::mailhog::MailhogService;
use services::mysql::{MysqlInstall, MysqlService};
use services::nginx::NginxService;
use services::redis::RedisService;
use services::{hidden_command, Service, ServiceStatus};
use ssl::LocalCa;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};

struct AppState {
    db: Mutex<Connection>,
    apache: Mutex<ApacheService>,
    mysql: Mutex<MysqlService>,
    nginx: Mutex<NginxService>,
    redis: Mutex<RedisService>,
    mailhog: Mutex<MailhogService>,
    php_installs: Vec<PhpInstall>,
    default_php: String,
    resources_dir: PathBuf,
    runtime_dir: PathBuf,
    htdocs_dir: PathBuf,
}

/// Lamp Bench is a self-contained install: bundled binaries, runtime state
/// and user-facing `htdocs/` all live under one root. In production that root
/// is the install dir (derived from `current_exe`). In dev it's the repo
/// root, so the binaries fetched by `pnpm scripts:fetch-binaries` line up
/// with the layout the prod build expects.
fn install_dir(_app: &tauri::AppHandle) -> PathBuf {
    #[cfg(debug_assertions)]
    {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri parent")
            .to_path_buf()
    }
    #[cfg(not(debug_assertions))]
    {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

fn resources_root(app: &tauri::AppHandle) -> PathBuf {
    install_dir(app).join("resources")
}

/// Writable state: generated configs, per-host SSL certs, MySQL data dirs,
/// log files, the SQLite app DB. Lives next to bundled resources so the
/// whole app folder can be moved or backed up as a single unit. In debug
/// it's `.lamp-bench/` next to the repo (gitignored); in production it's
/// `<install>/runtime/` (writable thanks to the NSIS post-install ACL hook).
fn runtime_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = if cfg!(debug_assertions) {
        install_dir(app).join(".lamp-bench")
    } else {
        install_dir(app).join("runtime")
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// User-facing `htdocs/`. Default Apache vhost serves from here, CMS
/// installers default to it. Sits at the top of the install dir in
/// production (`C:\<install>\htdocs\`) so users can find their projects
/// without digging through hidden folders.
fn htdocs_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = if cfg!(debug_assertions) {
        install_dir(app).join(".lamp-bench").join("htdocs")
    } else {
        install_dir(app).join("htdocs")
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[derive(Serialize)]
struct CommandResult {
    success: bool,
    stdout: String,
    stderr: String,
    exit_code: i32,
}

fn seed_default_index(htdocs: &Path) {
    let index = htdocs.join("index.html");
    if index.exists() {
        return;
    }
    let _ = std::fs::write(
        &index,
        "<!doctype html>\n\
         <html><head><meta charset=\"utf-8\"><title>Lamp Bench</title></head>\n\
         <body style=\"font-family:system-ui;max-width:42rem;margin:4rem auto;padding:0 1rem;color:#222\">\n\
         <h1>Lamp Bench</h1>\n\
         <p>This is the default <code>htdocs</code> directory. Drop project folders here, or use <strong>Tools → CMS Extras</strong> to install one with a click.</p>\n\
         <p>Apache serves this page at <code>http://localhost:8080/</code>. Per-host virtual hosts live alongside it on the same port.</p>\n\
         </body></html>\n",
    );
}

fn run_capture(cmd: &mut std::process::Command) -> Result<CommandResult, String> {
    let output = cmd.output().map_err(|e| e.to_string())?;
    Ok(CommandResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

fn php_exe(state: &AppState, version: Option<&str>) -> Result<PathBuf, String> {
    let fallback = effective_default_php(state);
    let v = version.unwrap_or(&fallback);
    let install = state
        .php_installs
        .iter()
        .find(|p| p.version == v)
        .or_else(|| state.php_installs.iter().find(|p| p.version == fallback))
        .ok_or_else(|| format!("PHP {v} not installed"))?;
    Ok(install
        .dir
        .join(format!("php{}", std::env::consts::EXE_SUFFIX)))
}

fn composer_phar(state: &AppState) -> PathBuf {
    state.resources_dir.join("composer").join("composer.phar")
}

#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Serialize)]
struct BuildInfo {
    version: &'static str,
    git_sha: &'static str,
    build_epoch: u64,
}

#[tauri::command]
fn build_info() -> BuildInfo {
    BuildInfo {
        version: env!("CARGO_PKG_VERSION"),
        git_sha: env!("LAMP_BENCH_GIT_SHA"),
        build_epoch: env!("LAMP_BENCH_BUILD_EPOCH").parse().unwrap_or(0),
    }
}

fn load_hosts(state: &AppState) -> Result<Vec<Host>, String> {
    let conn = state.db.lock().unwrap();
    hosts::list(&conn)
}

const SETTING_DEFAULT_PHP: &str = "default_php";

fn setting_get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get(0),
    )
    .ok()
}

fn setting_set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// The PHP version the default vhost, phpMyAdmin and any host whose own
/// version has gone missing all fall back to.
///
/// Read fresh from the database rather than from the value discovered at
/// startup — the Settings picker used to change nothing at all, because the
/// only copy of this lived in a field fixed when the app booted.
fn effective_default_php(state: &AppState) -> String {
    let stored = {
        let conn = state.db.lock().unwrap();
        setting_get(&conn, SETTING_DEFAULT_PHP)
    };
    match stored {
        // Ignore a stored version that has since been removed from disk.
        Some(v) if state.php_installs.iter().any(|p| p.version == v) => v,
        _ => state.default_php.clone(),
    }
}

#[tauri::command]
fn php_default_get(state: tauri::State<AppState>) -> String {
    effective_default_php(&state)
}

#[tauri::command]
fn php_default_set(version: String, state: tauri::State<AppState>) -> Result<(), String> {
    let installs = downloads::discover_php_installs(&state.resources_dir);
    if !installs.iter().any(|p| p.version == version) {
        return Err(format!("PHP {version} is not installed"));
    }
    let conn = state.db.lock().unwrap();
    setting_set(&conn, SETTING_DEFAULT_PHP, &version)
}

/// Compiled-in default (port, port2) for a service. port2 is 0 when the
/// service has a single port.
fn default_ports(service: &str) -> (u16, u16) {
    match service {
        "apache" => (8080, 8443),
        "nginx" => (8081, 8444),
        "mysql" => (3306, 0),
        "redis" => (6379, 0),
        "mailhog" => (8025, 1025), // UI, SMTP
        _ => (0, 0),
    }
}

/// Read a service's configured (port, port2), falling back to the default
/// when the user hasn't overridden it.
fn ports_for(conn: &Connection, service: &str) -> (u16, u16) {
    let (dp, dp2) = default_ports(service);
    conn.query_row(
        "SELECT port, port2 FROM service_config WHERE service = ?1",
        rusqlite::params![service],
        |row| Ok((row.get::<_, i64>(0)? as u16, row.get::<_, i64>(1)? as u16)),
    )
    .unwrap_or((dp, dp2))
}

#[derive(Serialize)]
struct ServicePortConfig {
    service: String,
    port: u16,
    port2: u16,
    has_secondary: bool,
}

#[tauri::command]
fn service_ports_get(name: String, state: tauri::State<AppState>) -> ServicePortConfig {
    let conn = state.db.lock().unwrap();
    let (port, port2) = ports_for(&conn, &name);
    let (_, default_p2) = default_ports(&name);
    ServicePortConfig {
        has_secondary: default_p2 != 0,
        service: name,
        port,
        port2,
    }
}

/// All ports in use by services OTHER than `exclude`, as (port, owner) pairs.
/// Uses each service's effective port (configured or default).
fn used_ports_excluding(conn: &Connection, exclude: &str) -> Vec<(u16, String)> {
    const SERVICES: [&str; 5] = ["apache", "nginx", "mysql", "redis", "mailhog"];
    let mut out = Vec::new();
    for svc in SERVICES {
        if svc == exclude {
            continue;
        }
        let (p, p2) = ports_for(conn, svc);
        if p != 0 {
            out.push((p, svc.to_string()));
        }
        if p2 != 0 {
            out.push((p2, svc.to_string()));
        }
    }
    out
}

#[tauri::command]
fn service_ports_set(
    name: String,
    port: u16,
    port2: u16,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    let (_, default_p2) = default_ports(&name);
    let has_secondary = default_p2 != 0;
    let effective_p2 = if has_secondary { port2 } else { 0 };

    if port == 0 || (has_secondary && effective_p2 == 0) {
        return Err("ports must be between 1 and 65535".into());
    }
    // A service can't collide with its own two ports either.
    if has_secondary && port == effective_p2 {
        return Err(format!(
            "the two ports must differ — both are set to {port}"
        ));
    }

    let conn = state.db.lock().unwrap();
    // Collision check against every other service's effective ports BEFORE
    // persisting, so a clash is rejected up front rather than surfacing as a
    // cryptic bind failure when the service later tries to start.
    let used = used_ports_excluding(&conn, &name);
    for new_port in [Some(port), if has_secondary { Some(effective_p2) } else { None }]
        .into_iter()
        .flatten()
    {
        if let Some((_, owner)) = used.iter().find(|(p, _)| *p == new_port) {
            return Err(format!(
                "port {new_port} is already used by {owner}"
            ));
        }
    }

    conn.execute(
        "INSERT INTO service_config (service, port, port2) VALUES (?1, ?2, ?3) \
         ON CONFLICT(service) DO UPDATE SET port=excluded.port, port2=excluded.port2",
        rusqlite::params![name, port as i64, effective_p2 as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn service_start(name: &str, state: tauri::State<AppState>) -> Result<(), String> {
    // Snapshot all configured ports up front (one DB lock) so cross-service
    // references (phpMyAdmin→MySQL, php.ini→MailHog SMTP) stay coherent.
    let (apache_p, apache_p2, mysql_p, _mysql_p2, mailhog_ui, mailhog_smtp, nginx_p, nginx_p2, redis_p) = {
        let conn = state.db.lock().unwrap();
        let (ap, ap2) = ports_for(&conn, "apache");
        let (mp, mp2) = ports_for(&conn, "mysql");
        let (mu, ms) = ports_for(&conn, "mailhog");
        let (np, np2) = ports_for(&conn, "nginx");
        let (rp, _) = ports_for(&conn, "redis");
        (ap, ap2, mp, mp2, mu, ms, np, np2, rp)
    };
    match name {
        "apache" => {
            let hosts = load_hosts(&state)?;
            let installs = downloads::discover_php_installs(&state.resources_dir);
            let default_php = effective_default_php(&state);
            let mut apache = state.apache.lock().unwrap();
            apache.set_php_installs(installs);
            apache.set_default_php(default_php);
            apache.set_hosts(hosts);
            apache.set_ports(apache_p, apache_p2, mysql_p, mailhog_smtp);
            apache.start()
        }
        "nginx" => {
            let hosts = load_hosts(&state)?;
            let installs = downloads::discover_php_installs(&state.resources_dir);
            let default_php = effective_default_php(&state);
            let mut nginx = state.nginx.lock().unwrap();
            nginx.set_php_installs(installs);
            nginx.set_default_php(default_php);
            nginx.set_hosts(hosts);
            nginx.set_ports(nginx_p, nginx_p2, mailhog_smtp);
            nginx.start()
        }
        "mysql" => {
            let mut mysql = state.mysql.lock().unwrap();
            mysql.set_port(mysql_p);
            mysql.start()
        }
        "redis" => {
            let mut redis = state.redis.lock().unwrap();
            redis.set_port(redis_p);
            redis.start()
        }
        "mailhog" => {
            let mut mailhog = state.mailhog.lock().unwrap();
            mailhog.set_ports(mailhog_ui, mailhog_smtp);
            mailhog.start()
        }
        other => Err(format!("unknown service: {other}")),
    }
}

#[tauri::command]
fn service_stop(name: &str, state: tauri::State<AppState>) -> Result<(), String> {
    match name {
        "apache" => state.apache.lock().unwrap().stop(),
        "nginx" => state.nginx.lock().unwrap().stop(),
        "mysql" => state.mysql.lock().unwrap().stop(),
        "redis" => state.redis.lock().unwrap().stop(),
        "mailhog" => state.mailhog.lock().unwrap().stop(),
        other => Err(format!("unknown service: {other}")),
    }
}

fn status_of(name: &str, state: &AppState) -> Result<ServiceStatus, String> {
    match name {
        "apache" => Ok(state.apache.lock().unwrap().status()),
        "nginx" => Ok(state.nginx.lock().unwrap().status()),
        "mysql" => Ok(state.mysql.lock().unwrap().status()),
        "redis" => Ok(state.redis.lock().unwrap().status()),
        "mailhog" => Ok(state.mailhog.lock().unwrap().status()),
        other => Err(format!("unknown service: {other}")),
    }
}

#[tauri::command]
fn service_status(name: &str, state: tauri::State<AppState>) -> Result<ServiceStatus, String> {
    status_of(name, &state)
}

/// Services that hold files inside a manifest entry's install directory.
///
/// Replacing or deleting those files while the process has them open silently
/// loses data on Windows — that is how MySQL 8.0's `bin/` was emptied during
/// an earlier smoke test, with a leaked mysqld still holding the DLLs.
fn services_using_binary(name: &str) -> &'static [&'static str] {
    match name {
        "apache" | "mod_fcgid" => &["apache"],
        "nginx" => &["nginx"],
        "redis" => &["redis"],
        "mailhog" => &["mailhog"],
        n if n.starts_with("mysql-") => &["mysql"],
        // PHP is loaded by Apache's mod_fcgid children and by the php-cgi
        // pools Nginx drives, so either one pins the files.
        n if n.starts_with("php-") || n.starts_with("xdebug-") => &["apache", "nginx"],
        _ => &[],
    }
}

fn ensure_not_in_use(name: &str, state: &AppState) -> Result<(), String> {
    for svc in services_using_binary(name) {
        if matches!(status_of(svc, state)?, ServiceStatus::Running { .. }) {
            return Err(format!(
                "Stop {svc} first — replacing files it currently has open loses them."
            ));
        }
    }
    Ok(())
}

#[tauri::command]
fn mysql_versions(state: tauri::State<AppState>) -> Vec<String> {
    state.mysql.lock().unwrap().versions()
}

#[tauri::command]
fn mysql_active_version(state: tauri::State<AppState>) -> String {
    state.mysql.lock().unwrap().active_version()
}

#[tauri::command]
fn mysql_set_version(version: String, state: tauri::State<AppState>) -> Result<(), String> {
    state.mysql.lock().unwrap().set_active(version)
}

#[tauri::command]
fn php_versions(state: tauri::State<AppState>) -> Vec<String> {
    state.apache.lock().unwrap().available_php_versions()
}

#[tauri::command]
fn host_list(state: tauri::State<AppState>) -> Result<Vec<Host>, String> {
    load_hosts(&state)
}

#[tauri::command]
fn host_create(
    name: String,
    docroot: String,
    php_version: String,
    state: tauri::State<AppState>,
) -> Result<Host, String> {
    let host = {
        let conn = state.db.lock().unwrap();
        hosts::create(&conn, &name, &docroot, &php_version)?
    };
    apply_host_changes(&state)?;
    Ok(host)
}

#[tauri::command]
fn host_update(
    id: i64,
    name: String,
    docroot: String,
    php_version: String,
    apache_extra: String,
    nginx_extra: String,
    state: tauri::State<AppState>,
) -> Result<Host, String> {
    let host = {
        let conn = state.db.lock().unwrap();
        hosts::update(
            &conn,
            id,
            &name,
            &docroot,
            &php_version,
            &apache_extra,
            &nginx_extra,
        )?
    };
    apply_host_changes(&state)?;
    Ok(host)
}

#[tauri::command]
fn snapshot_list(
    host_id: i64,
    state: tauri::State<AppState>,
) -> Result<Vec<snapshots::Snapshot>, String> {
    let conn = state.db.lock().unwrap();
    snapshots::list_for_host(&conn, host_id)
}

#[tauri::command]
fn snapshot_create(
    host_id: i64,
    label: String,
    db_name: Option<String>,
    state: tauri::State<AppState>,
) -> Result<snapshots::Snapshot, String> {
    let (mysql_dir, mysql_port, mysql_version) = mysql_active_full(&state);

    // Look the host up, then let go. The capture below runs mysqldump and
    // compresses the whole docroot, and the single SQLite connection sits
    // behind one mutex — holding it for the duration froze every other
    // command in the app until the snapshot finished.
    let host = {
        let conn = state.db.lock().unwrap();
        hosts::list(&conn)?
            .into_iter()
            .find(|h| h.id == host_id)
            .ok_or_else(|| format!("host {host_id} not found"))?
    };

    let trimmed = db_name.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let db_capture = trimmed.map(|name| snapshots::DbCapture {
        mysql_dir: &mysql_dir,
        port: mysql_port,
        db_name: name,
        version: &mysql_version,
    });
    let captured = snapshots::capture(&host, &state.runtime_dir, db_capture)?;

    let conn = state.db.lock().unwrap();
    snapshots::record(&conn, host.id, &label, captured)
}

#[tauri::command]
fn snapshot_restore(id: i64, state: tauri::State<AppState>) -> Result<(), String> {
    let (mysql_dir, mysql_port) = mysql_active(&state);
    let conn = state.db.lock().unwrap();
    let host_id: i64 = conn
        .query_row(
            "SELECT host_id FROM snapshots WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .map_err(|e| format!("snapshot lookup: {e}"))?;
    let host = hosts::list(&conn)?
        .into_iter()
        .find(|h| h.id == host_id)
        .ok_or_else(|| format!("host {host_id} not found"))?;
    snapshots::restore(&conn, id, &host, &mysql_dir, mysql_port)
}

#[tauri::command]
fn snapshot_delete(id: i64, state: tauri::State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    snapshots::delete(&conn, id)
}

#[tauri::command]
fn host_delete(id: i64, state: tauri::State<AppState>) -> Result<(), String> {
    {
        let conn = state.db.lock().unwrap();
        hosts::delete(&conn, id, &state.runtime_dir)?;
    }
    apply_host_changes(&state)?;
    Ok(())
}

fn apply_host_changes(state: &AppState) -> Result<(), String> {
    let all = load_hosts(state)?;
    hosts::apply_to_system(&all, &state.runtime_dir)?;
    let installs = downloads::discover_php_installs(&state.resources_dir);
    {
        let mut apache = state.apache.lock().unwrap();
        apache.set_php_installs(installs.clone());
        apache.set_hosts(all.clone());
        apache.reload()?;
    }
    {
        let mut nginx = state.nginx.lock().unwrap();
        nginx.set_php_installs(installs);
        nginx.set_hosts(all);
        nginx.reload()?;
    }
    Ok(())
}

#[tauri::command]
fn git_available() -> bool {
    hidden_command("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tauri::command]
fn git_init(path: String) -> Result<CommandResult, String> {
    if !std::path::Path::new(&path).exists() {
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    }
    run_capture(hidden_command("git").arg("init").current_dir(&path))
}

#[tauri::command]
fn composer_version(state: tauri::State<AppState>) -> Result<CommandResult, String> {
    let php = php_exe(&state, None)?;
    let phar = composer_phar(&state);
    run_capture(hidden_command(&php).arg(phar).arg("--version"))
}

#[tauri::command]
fn laravel_create(
    name: String,
    parent_dir: String,
    php_version: Option<String>,
    state: tauri::State<AppState>,
) -> Result<String, String> {
    let parent = PathBuf::from(&parent_dir);
    if !parent.exists() {
        return Err(format!("parent dir does not exist: {parent_dir}"));
    }
    let project_dir = parent.join(name.trim());
    if project_dir.exists() {
        return Err(format!(
            "{} already exists",
            project_dir.display()
        ));
    }

    let php = php_exe(&state, php_version.as_deref())?;
    let phar = composer_phar(&state);
    let res = run_capture(
        hidden_command(&php)
            .arg(phar)
            .arg("create-project")
            .arg("laravel/laravel")
            .arg(&project_dir)
            .arg("--no-interaction")
            .arg("--prefer-dist"),
    )?;
    if !res.success {
        return Err(format!(
            "composer create-project failed (exit {}):\n{}",
            res.exit_code, res.stderr
        ));
    }
    // Laravel apps are served from the `public/` subfolder.
    let public_dir = project_dir.join("public");
    Ok(public_dir.to_string_lossy().replace('\\', "/"))
}

#[derive(Serialize)]
struct FileContents {
    content: String,
    /// False when the bytes on disk were not valid UTF-8. The editor opens
    /// those read-only.
    utf8: bool,
}

/// Read a file for the editor, reporting whether it decoded cleanly.
///
/// The lossy decode is deliberate — Apache logs and older PHP sources mix
/// UTF-8 with the local codepage, and refusing to show them is worse than
/// showing them. But the replacement characters it produces are real
/// characters, so *saving* one of those buffers used to write `EF BF BD` over
/// every byte the decoder didn't recognise: silent, permanent corruption in a
/// tool people point at their source files.
#[tauri::command]
fn file_read(path: String) -> Result<FileContents, String> {
    let raw = std::fs::read(&path).map_err(|e| format!("read {path}: {e}"))?;
    match String::from_utf8(raw) {
        Ok(content) => Ok(FileContents {
            content,
            utf8: true,
        }),
        Err(e) => Ok(FileContents {
            content: String::from_utf8_lossy(e.as_bytes()).into_owned(),
            utf8: false,
        }),
    }
}

#[tauri::command]
fn file_write(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("write {path}: {e}"))
}

fn mysql_active(state: &AppState) -> (PathBuf, u16) {
    let (dir, port, _version) = mysql_active_full(state);
    (dir, port)
}

/// Like `mysql_active` but also returns the active version label so snapshot
/// capture can record which MySQL produced the dump.
///
/// The port is read from `service_config`, not assumed. Everything that talks
/// to MySQL over TCP goes through here — `mysqldump` for snapshots, the
/// `mysql` client for restores, and `CREATE DATABASE` for the CMS installers
/// — so hardcoding 3306 meant all of them failed with "can't connect" the
/// moment someone moved MySQL off its default port.
fn mysql_active_full(state: &AppState) -> (PathBuf, u16, String) {
    // Deliberately two separate scopes: never hold the service lock while
    // taking the DB lock, so this can't invert the order `service_start`
    // uses (DB first, then service) and deadlock.
    let active = state.mysql.lock().unwrap().active_version();
    let port = {
        let conn = state.db.lock().unwrap();
        ports_for(&conn, "mysql").0
    };
    (
        state.resources_dir.join(format!("mysql-{active}")),
        port,
        active,
    )
}

fn register_host_and_apply(
    state: &AppState,
    hostname: &str,
    docroot: &Path,
    php_version: &str,
) -> Result<String, String> {
    let docroot_str = docroot.to_string_lossy().replace('\\', "/");
    {
        let conn = state.db.lock().unwrap();
        hosts::create(&conn, hostname, &docroot_str, php_version)?;
    }
    apply_host_changes(state)?;
    Ok(docroot_str)
}

#[tauri::command]
fn wordpress_install(
    site_name: String,
    hostname: String,
    parent_dir: String,
    php_version: String,
    state: tauri::State<AppState>,
) -> Result<String, String> {
    let (mysql_dir, mysql_port) = mysql_active(&state);
    let req = cms::wordpress::InstallRequest {
        site_name: site_name.trim().to_string(),
        hostname: hostname.trim().to_string(),
        parent_dir: PathBuf::from(parent_dir.trim()),
        mysql_dir,
        mysql_port,
        source_dir: state.resources_dir.join("wordpress"),
    };
    let result = cms::wordpress::install(&req)?;

    // Hostname is optional. When empty the site is reachable through the
    // default vhost as `localhost:8080/<site_name>/`.
    if req.hostname.is_empty() {
        return Ok(result.docroot.to_string_lossy().replace('\\', "/"));
    }
    register_host_and_apply(&state, &req.hostname, &result.docroot, php_version.trim())
}

fn cms_install_generic(
    state: &AppState,
    source_subdir: &str,
    db_prefix: &str,
    site_name: &str,
    hostname: &str,
    parent_dir: &str,
    php_version: &str,
) -> Result<String, String> {
    let parent = PathBuf::from(parent_dir.trim());
    let target = parent.join(site_name.trim());
    let (mysql_dir, mysql_port) = mysql_active(state);
    let db_name = cms::sanitize_db_name(db_prefix, site_name.trim());

    let docroot = cms::install_files_and_db(
        &state.resources_dir.join(source_subdir),
        &target,
        &mysql_dir,
        mysql_port,
        &db_name,
    )?;
    if hostname.trim().is_empty() {
        return Ok(docroot.to_string_lossy().replace('\\', "/"));
    }
    register_host_and_apply(state, hostname.trim(), &docroot, php_version.trim())
}

#[tauri::command]
fn joomla_install(
    site_name: String,
    hostname: String,
    parent_dir: String,
    php_version: String,
    state: tauri::State<AppState>,
) -> Result<String, String> {
    cms_install_generic(
        &state,
        "joomla",
        "joomla",
        &site_name,
        &hostname,
        &parent_dir,
        &php_version,
    )
}

#[tauri::command]
fn drupal_install(
    site_name: String,
    hostname: String,
    parent_dir: String,
    php_version: String,
    state: tauri::State<AppState>,
) -> Result<String, String> {
    cms_install_generic(
        &state,
        "drupal",
        "drupal",
        &site_name,
        &hostname,
        &parent_dir,
        &php_version,
    )
}

#[tauri::command]
fn mediawiki_install(
    site_name: String,
    hostname: String,
    parent_dir: String,
    php_version: String,
    state: tauri::State<AppState>,
) -> Result<String, String> {
    cms_install_generic(
        &state,
        "mediawiki",
        "mw",
        &site_name,
        &hostname,
        &parent_dir,
        &php_version,
    )
}

#[tauri::command]
fn php_lint(
    path: String,
    php_version: Option<String>,
    state: tauri::State<AppState>,
) -> Result<CommandResult, String> {
    let php = php_exe(&state, php_version.as_deref())?;
    run_capture(hidden_command(&php).arg("-l").arg(&path))
}

#[tauri::command]
fn binary_installed(name: &str, state: tauri::State<AppState>) -> bool {
    downloads::is_installed(name, &state.resources_dir)
}

#[tauri::command]
fn binary_download(
    name: String,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    ensure_not_in_use(&name, &state)?;
    // Streaming progress is forwarded as Tauri events. Frontend subscribes
    // to `binary-download-progress` and filters on the `name` field.
    let resources = state.resources_dir.clone();
    let emit_name = name.clone();
    let mut cb = move |downloaded: u64, total: Option<u64>| {
        let _ = app.emit(
            "binary-download-progress",
            serde_json::json!({
                "name": emit_name,
                "downloaded": downloaded,
                "total": total,
            }),
        );
    };
    downloads::download(&name, &resources, Some(&mut cb))
}

#[tauri::command]
fn binary_remove(name: &str, state: tauri::State<AppState>) -> Result<(), String> {
    ensure_not_in_use(name, &state)?;
    downloads::remove(name, &state.resources_dir)
}

#[tauri::command]
fn binary_list() -> Vec<String> {
    downloads::list_manifest_entries()
}

#[tauri::command]
fn php_catalog(state: tauri::State<AppState>) -> Vec<downloads::PhpCatalogEntry> {
    downloads::php_catalog(&state.resources_dir)
}

#[tauri::command]
fn php_install(version: String, state: tauri::State<AppState>) -> Result<(), String> {
    downloads::install_php_with_xdebug(&version, &state.resources_dir)
}

#[tauri::command]
fn htdocs_path(state: tauri::State<AppState>) -> String {
    state.htdocs_dir.to_string_lossy().replace('\\', "/")
}

/// Writable state root: generated configs, MySQL data dirs, certs, logs.
///
/// The UI used to reconstruct this by string-surgery on `htdocs_path()`,
/// which happened to work in dev (`<repo>/.lamp-bench/htdocs` → strip
/// `/htdocs`) and was wrong in every real install, where htdocs and runtime
/// are siblings rather than nested (`C:/LAMP/htdocs` vs `C:/LAMP/runtime`).
#[tauri::command]
fn runtime_path(state: tauri::State<AppState>) -> String {
    state.runtime_dir.to_string_lossy().replace('\\', "/")
}

/// Where the downloaded service binaries live (`php-8.4/`, `apache/`, …).
#[tauri::command]
fn resources_path(state: tauri::State<AppState>) -> String {
    state.resources_dir.to_string_lossy().replace('\\', "/")
}

/// LAN-routable IPv4 of this machine, for the "open on my phone" QR code.
/// Uses the classic UDP-connect trick: bind ephemeral, connect to a public
/// address (no packet actually sent), read the OS-selected local IP. That
/// avoids pulling a network-interface crate just to enumerate adapters.
#[tauri::command]
fn ftp_upload(
    host: String,
    port: u16,
    user: String,
    password: String,
    remote_dir: String,
    local_dir: String,
    protocol: Option<String>,
) -> Result<deploy::DeployReport, String> {
    // Guard against silently downgrading an FTPS/SFTP request to plaintext —
    // that would leak credentials over the wire. Encrypted transports are
    // not wired yet (suppaftp's TLS type-split + no SFTP runtime), so reject
    // them explicitly rather than send the password in the clear.
    match protocol.as_deref() {
        None | Some("ftp") => {}
        Some(other) => {
            return Err(format!(
                "{} transport is not supported yet — only plain FTP is available. \
                 Encrypted transports (FTPS/SFTP) are coming in a later release.",
                other.to_uppercase()
            ))
        }
    }
    deploy::ftp_upload_folder(
        host.trim(),
        port,
        user.trim(),
        &password,
        remote_dir.trim(),
        Path::new(&local_dir),
    )
}

#[tauri::command]
fn ioncube_install(version: String, state: tauri::State<AppState>) -> Result<(), String> {
    let php_dir = state.resources_dir.join(format!("php-{version}"));
    ioncube::install(&version, &php_dir, downloads::current_platform())
}

/// Locate a PHP install and make sure its `php.ini` is fully formed before
/// the extensions panel reads or edits it. Opening that panel is one of two
/// ways a `php.ini` can come into existence, and it used to produce a bare
/// copy of the template that never received the settings block.
fn php_dir_with_ini(version: &str, state: &AppState) -> Result<PathBuf, String> {
    let dir = state.resources_dir.join(format!("php-{version}"));
    let smtp = {
        let conn = state.db.lock().unwrap();
        ports_for(&conn, "mailhog").1
    };
    php::ensure_managed_ini(&dir, smtp)?;
    Ok(dir)
}

#[tauri::command]
fn php_extensions(
    version: String,
    state: tauri::State<AppState>,
) -> Result<Vec<php::PhpExtension>, String> {
    php::list_extensions(&php_dir_with_ini(&version, &state)?)
}

#[tauri::command]
fn php_extension_toggle(
    version: String,
    name: String,
    enable: bool,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    php::toggle_extension(&php_dir_with_ini(&version, &state)?, &name, enable)
}

#[tauri::command]
fn dyndns_update(
    provider: String,
    hostname: String,
    user: String,
    password: String,
) -> Result<dyndns::DynDnsResult, String> {
    dyndns::update(&provider, &hostname, &user, &password)
}

#[tauri::command]
fn deploy_profile_get(
    host_id: i64,
    state: tauri::State<AppState>,
) -> Result<Option<deploy::DeployProfile>, String> {
    let conn = state.db.lock().unwrap();
    deploy::get_profile(&conn, host_id)
}

#[tauri::command]
fn deploy_profile_save(
    profile: deploy::DeployProfile,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    deploy::save_profile(&conn, &profile)
}

#[tauri::command]
fn compress_images(
    folder: String,
    jpeg_quality: u8,
    include_png: bool,
    include_jpg: bool,
) -> Result<images::CompressReport, String> {
    let q = jpeg_quality.clamp(40, 100);
    images::compress_folder(Path::new(&folder), q, include_png, include_jpg)
}

/// Identifier of the current OS+arch as written in `scripts/binaries.json`.
/// Used by the React setup wizard to short-circuit on platforms that
/// don't have bundled-service binaries pinned yet.
#[tauri::command]
fn current_platform() -> &'static str {
    downloads::current_platform()
}

#[tauri::command]
fn lan_ip() -> Option<String> {
    use std::net::UdpSocket;
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    sock.local_addr().ok().map(|a| a.ip().to_string())
}

#[tauri::command]
fn editor_open(path: String, app: tauri::AppHandle) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    let label = format!(
        "editor-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    // We hand the path off through the URL hash. App.tsx checks it on mount
    // and renders a full-screen EditorSection when present.
    let encoded = path.replace('#', "%23");
    let url = format!("index.html#editor={encoded}");
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
        .title(format!("Lamp Bench — {}", path))
        .inner_size(1100.0, 720.0)
        .min_inner_size(600.0, 400.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn read_log(
    service: &str,
    lines: usize,
    state: tauri::State<AppState>,
) -> Result<String, String> {
    let runtime = &state.runtime_dir;
    let path = match service {
        "apache" => runtime.join("apache").join("logs").join("error.log"),
        "nginx" => runtime.join("nginx").join("logs").join("error.log"),
        "mysql" => {
            // The data + log dir is per active version (mysql-5.7 / mysql-8.0),
            // not a flat `mysql/` dir. Read whichever version is live.
            let active = state.mysql.lock().unwrap().active_version();
            runtime.join(format!("mysql-{active}")).join("mysql.log")
        }
        "redis" => runtime.join("redis").join("redis.log"),
        "mailhog" => runtime.join("mailhog").join("mailhog.log"),
        other => return Err(format!("unknown log: {other}")),
    };
    if !path.exists() {
        return Ok(String::new());
    }
    read_tail(&path, lines)
}

/// Read the last `lines` lines of a file without slurping the whole thing
/// into memory. Seeks from the end in 64 KB chunks until enough newlines are
/// collected, then decodes lossily — Apache + php-cgi mix UTF-8 with the
/// local codepage (Windows-1252) for OS error strings, so a strict UTF-8
/// decode would abort on the first invalid byte.
fn read_tail(path: &Path, lines: usize) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    if len == 0 || lines == 0 {
        return Ok(String::new());
    }
    const CHUNK: u64 = 65536;
    let mut buf: Vec<u8> = Vec::new();
    let mut pos = len;
    let mut newlines = 0usize;
    // +1 because the tail of the file usually has no trailing newline for the
    // final line, so we need one extra boundary to keep `lines` whole lines.
    while pos > 0 && newlines <= lines {
        let read_size = CHUNK.min(pos);
        pos -= read_size;
        file.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
        let mut chunk = vec![0u8; read_size as usize];
        file.read_exact(&mut chunk).map_err(|e| e.to_string())?;
        newlines += chunk.iter().filter(|&&b| b == b'\n').count();
        let mut combined = chunk;
        combined.append(&mut buf);
        buf = combined;
    }
    let content = String::from_utf8_lossy(&buf);
    let collected: Vec<&str> = content.lines().collect();
    let start = collected.len().saturating_sub(lines);
    Ok(collected[start..].join("\n"))
}

/// Tear down every supervised service before the process goes away.
///
/// Without this, quitting from the tray (`app.exit(0)`) left httpd, mysqld,
/// nginx, the php-cgi pools, redis-server and MailHog running as orphans:
/// Windows has no job object tying them to us and Unix has no PDEATHSIG. The
/// next launch then reported "stopped" for everything (the `Child` handles
/// died with the old process) while the ports were still held and MySQL's
/// data dir was still locked — an unrecoverable-looking state with no error
/// message anywhere.
///
/// Best-effort by design: this runs on the way out, so a poisoned mutex or a
/// stubborn child must not stop the remaining services from being cleaned up.
fn stop_all_services(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        stop_services(state.inner());
    }
}

fn stop_services(state: &AppState) {
    // MySQL first, and on its own: it's the only one that loses data to a
    // hard kill, and `stop()` gives it up to 20s to flush. Apache/Nginx are
    // stopped after so they aren't left serving requests against a database
    // that's already halfway through shutdown.
    if let Ok(mut mysql) = state.mysql.lock() {
        let _ = mysql.stop();
    }
    if let Ok(mut apache) = state.apache.lock() {
        let _ = apache.stop();
    }
    if let Ok(mut nginx) = state.nginx.lock() {
        let _ = nginx.stop();
    }
    if let Ok(mut redis) = state.redis.lock() {
        let _ = redis.stop();
    }
    if let Ok(mut mailhog) = state.mailhog.lock() {
        let _ = mailhog.stop();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // MUST be the first plugin registered. When a second copy of Lamp
        // Bench is launched while one is already running, this callback
        // fires in the EXISTING instance (restoring + focusing its window)
        // and the second process exits immediately — you can't have two
        // instances fighting over the same ports, MySQL data dir and
        // generated configs.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // Persists main-window size + position to a JSON file under the OS
        // app-data dir, restoring it on next launch. Editor child windows
        // (label `editor-*`) opt out via the per-window builder when needed.
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .setup(|app| {
            let resources = resources_root(app.handle());
            let runtime = runtime_root(app.handle())
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            let htdocs = htdocs_root(app.handle())
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            seed_default_index(&htdocs);

            let conn = db::open(&runtime.join("lamp.db"))?;

            // Discover what's actually on disk instead of hardcoding the list.
            // The installer only ships php-8.4; other versions are on-demand.
            // Hardcoding them caused Apache to try to seed a php.ini for a
            // version that didn't exist and blow up the whole service start.
            let mut php_installs = downloads::discover_php_installs(&resources);
            if php_installs.is_empty() {
                // Bundled 8.4 should always exist; this is just a belt-and-
                // suspenders fallback so the app still boots if discovery
                // returns empty on a weird filesystem.
                php_installs.push(PhpInstall {
                    version: "8.4".into(),
                    dir: resources.join("php-8.4"),
                });
            }
            let default_php = if php_installs.iter().any(|p| p.version == "8.4") {
                "8.4".to_string()
            } else {
                php_installs[0].version.clone()
            };

            let ca_dir = runtime.join("ca");
            let ssl_dir = runtime.join("ssl");

            let mysql_installs = vec![
                MysqlInstall {
                    version: "5.7".into(),
                    dir: resources.join("mysql-5.7"),
                },
                MysqlInstall {
                    version: "8.0".into(),
                    dir: resources.join("mysql-8.0"),
                },
            ];
            let default_mysql = "8.0".to_string();

            let state = AppState {
                db: Mutex::new(conn),
                apache: Mutex::new(ApacheService::new(
                    resources.join("apache"),
                    resources.join("phpmyadmin"),
                    php_installs.clone(),
                    default_php.clone(),
                    LocalCa::new(ca_dir.clone()),
                    ssl_dir.clone(),
                    runtime.clone(),
                    htdocs.clone(),
                )),
                nginx: Mutex::new(NginxService::new(
                    resources.join("nginx"),
                    runtime.clone(),
                    ssl_dir,
                    LocalCa::new(ca_dir),
                    php_installs.clone(),
                    default_php.clone(),
                )),
                mysql: Mutex::new(MysqlService::new(
                    mysql_installs,
                    default_mysql,
                    runtime.clone(),
                )),
                redis: Mutex::new(RedisService::new(
                    resources.join("redis"),
                    runtime.clone(),
                )),
                mailhog: Mutex::new(MailhogService::new(
                    resources.join("mailhog"),
                    runtime.clone(),
                )),
                php_installs,
                default_php,
                resources_dir: resources,
                runtime_dir: runtime,
                htdocs_dir: htdocs,
            };

            app.manage(state);

            // System tray + close-to-tray (Discord-style). Clicking the
            // window's X hides instead of quitting; the user re-opens by
            // left-clicking the tray icon, or quits cleanly from the menu.
            let show_item = MenuItem::with_id(app, "show", "Show Lamp Bench", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let tray_icon = app
                .default_window_icon()
                .cloned()
                .ok_or_else(|| -> Box<dyn std::error::Error> {
                    "no default window icon to use for tray".into()
                })?;
            TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .tooltip("Lamp Bench")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                    // Belt and braces: `RunEvent::Exit` reaps the children
                    // too, but this is the path a user actually takes, so
                    // stop them here where we know the runtime is still
                    // healthy. Both are idempotent (`child.take()`).
                    "quit" => {
                        stop_all_services(app);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Only the main window minimises to the tray. Editor windows
                // (label `editor-*`) close for real.
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            build_info,
            service_start,
            service_stop,
            service_status,
            service_ports_get,
            service_ports_set,
            php_versions,
            php_default_get,
            php_default_set,
            mysql_versions,
            mysql_active_version,
            mysql_set_version,
            host_list,
            host_create,
            host_update,
            host_delete,
            snapshot_list,
            snapshot_create,
            snapshot_restore,
            snapshot_delete,
            read_log,
            htdocs_path,
            runtime_path,
            resources_path,
            lan_ip,
            compress_images,
            current_platform,
            ftp_upload,
            deploy_profile_get,
            deploy_profile_save,
            dyndns_update,
            ioncube_install,
            php_extensions,
            php_extension_toggle,
            editor_open,
            binary_installed,
            binary_download,
            binary_remove,
            binary_list,
            php_catalog,
            php_install,
            git_available,
            git_init,
            composer_version,
            laravel_create,
            wordpress_install,
            joomla_install,
            drupal_install,
            mediawiki_install,
            file_read,
            file_write,
            php_lint,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        // `run` with a callback instead of the plain `run()` so we get the
        // exit events. `Exit` is the last thing Tauri emits before the
        // process goes away — the only reliable place to reap the service
        // children we spawned. See `stop_all_services`.
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                stop_all_services(app);
            }
        });
}