mod cloud;
mod cms;
mod config_gen;
mod db;
mod deploy;
mod hosts;
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
use services::mysql::{MysqlInstall, MysqlService};
use services::nginx::NginxService;
use services::redis::RedisService;
use services::{Service, ServiceStatus};
use ssl::LocalCa;
use std::process::Command;
use tauri::Manager;

struct AppState {
    db: Mutex<Connection>,
    apache: Mutex<ApacheService>,
    mysql: Mutex<MysqlService>,
    nginx: Mutex<NginxService>,
    redis: Mutex<RedisService>,
    php_installs: Vec<PhpInstall>,
    default_php: String,
    resources_dir: PathBuf,
    runtime_dir: PathBuf,
}

/// Where bundled binaries live at runtime.
///
/// In debug builds we read from the repo's `resources/` next to `Cargo.toml`,
/// so `pnpm tauri dev` doesn't need a reinstall. In release builds we pull
/// them from the location Tauri's bundler dropped them into, next to the
/// installed binary.
fn resources_root(app: &tauri::AppHandle) -> PathBuf {
    #[cfg(debug_assertions)]
    {
        let _ = app;
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri parent")
            .join("resources")
    }
    #[cfg(not(debug_assertions))]
    {
        app.path()
            .resource_dir()
            .expect("Tauri resource dir")
            .join("resources")
    }
}

/// Where Lamp Bench keeps its writable state: SQLite DB, generated configs,
/// per-host certs, MySQL data dirs, log files.
///
/// In debug builds: `.lamp-bench/` at the repo root — convenient to inspect.
/// In release builds: the OS app-data dir, e.g.
/// `%APPDATA%\com.lampbench.app\` on Windows.
fn runtime_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    {
        let _ = app;
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri parent")
            .join(".lamp-bench"))
    }
    #[cfg(not(debug_assertions))]
    {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("app_data_dir: {e}"))?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(dir)
    }
}

#[derive(Serialize)]
struct CommandResult {
    success: bool,
    stdout: String,
    stderr: String,
    exit_code: i32,
}

fn run_capture(cmd: &mut Command) -> Result<CommandResult, String> {
    let output = cmd.output().map_err(|e| e.to_string())?;
    Ok(CommandResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

fn php_exe(state: &AppState, version: Option<&str>) -> Result<PathBuf, String> {
    let v = version.unwrap_or(&state.default_php);
    let install = state
        .php_installs
        .iter()
        .find(|p| p.version == v)
        .or_else(|| state.php_installs.iter().find(|p| p.version == state.default_php))
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

fn load_hosts(state: &AppState) -> Result<Vec<Host>, String> {
    let conn = state.db.lock().unwrap();
    hosts::list(&conn)
}

#[tauri::command]
fn service_start(name: &str, state: tauri::State<AppState>) -> Result<(), String> {
    match name {
        "apache" => {
            let hosts = load_hosts(&state)?;
            let mut apache = state.apache.lock().unwrap();
            apache.set_hosts(hosts);
            apache.start()
        }
        "nginx" => {
            let hosts = load_hosts(&state)?;
            let mut nginx = state.nginx.lock().unwrap();
            nginx.set_hosts(hosts);
            nginx.start()
        }
        "mysql" => state.mysql.lock().unwrap().start(),
        "redis" => state.redis.lock().unwrap().start(),
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
        other => Err(format!("unknown service: {other}")),
    }
}

#[tauri::command]
fn service_status(name: &str, state: tauri::State<AppState>) -> Result<ServiceStatus, String> {
    match name {
        "apache" => Ok(state.apache.lock().unwrap().status()),
        "nginx" => Ok(state.nginx.lock().unwrap().status()),
        "mysql" => Ok(state.mysql.lock().unwrap().status()),
        "redis" => Ok(state.redis.lock().unwrap().status()),
        other => Err(format!("unknown service: {other}")),
    }
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
fn host_delete(id: i64, state: tauri::State<AppState>) -> Result<(), String> {
    {
        let conn = state.db.lock().unwrap();
        hosts::delete(&conn, id)?;
    }
    apply_host_changes(&state)?;
    Ok(())
}

fn apply_host_changes(state: &AppState) -> Result<(), String> {
    let all = load_hosts(state)?;
    hosts::apply_to_system(&all)?;
    {
        let mut apache = state.apache.lock().unwrap();
        apache.set_hosts(all.clone());
        apache.reload()?;
    }
    {
        let mut nginx = state.nginx.lock().unwrap();
        nginx.set_hosts(all);
        nginx.reload()?;
    }
    Ok(())
}

#[tauri::command]
fn git_available() -> bool {
    Command::new("git")
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
    run_capture(Command::new("git").arg("init").current_dir(&path))
}

#[tauri::command]
fn composer_version(state: tauri::State<AppState>) -> Result<CommandResult, String> {
    let php = php_exe(&state, None)?;
    let phar = composer_phar(&state);
    run_capture(Command::new(&php).arg(phar).arg("--version"))
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
        Command::new(&php)
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

#[tauri::command]
fn file_read(path: String) -> Result<String, String> {
    let raw = std::fs::read(&path).map_err(|e| format!("read {path}: {e}"))?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

#[tauri::command]
fn file_write(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("write {path}: {e}"))
}

fn mysql_active(state: &AppState) -> (PathBuf, u16) {
    let mysql = state.mysql.lock().unwrap();
    let active = mysql.active_version();
    (
        state.resources_dir.join(format!("mysql-{active}")),
        3306u16,
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
    run_capture(Command::new(&php).arg("-l").arg(&path))
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
        "mysql" => runtime.join("mysql").join("mysql.log"),
        other => return Err(format!("unknown log: {other}")),
    };
    if !path.exists() {
        return Ok(String::new());
    }
    // Read as bytes and decode lossily — Apache + php-cgi mix UTF-8 with the
    // local codepage (Windows-1252 for the user) when they write OS error
    // strings. `read_to_string` would refuse the whole file on the first
    // invalid byte; `from_utf8_lossy` swaps invalid sequences for U+FFFD.
    let raw = std::fs::read(&path).map_err(|e| e.to_string())?;
    let content = String::from_utf8_lossy(&raw);
    let collected: Vec<&str> = content.lines().collect();
    let start = collected.len().saturating_sub(lines);
    Ok(collected[start..].join("\n"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let resources = resources_root(&app.handle());
            let runtime = runtime_root(&app.handle())
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            std::fs::create_dir_all(&runtime).ok();

            let conn = db::open(&runtime.join("lamp.db"))?;

            let php_installs = vec![
                PhpInstall {
                    version: "8.3".into(),
                    dir: resources.join("php-8.3"),
                },
                PhpInstall {
                    version: "8.4".into(),
                    dir: resources.join("php-8.4"),
                },
            ];
            let default_php = "8.4".to_string();

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
                php_installs,
                default_php,
                resources_dir: resources,
                runtime_dir: runtime,
            };

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            service_start,
            service_stop,
            service_status,
            php_versions,
            mysql_versions,
            mysql_active_version,
            mysql_set_version,
            host_list,
            host_create,
            host_update,
            host_delete,
            read_log,
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}