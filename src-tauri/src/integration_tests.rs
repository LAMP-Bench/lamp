//! Opt-in integration tests that exercise the real path end to end: download
//! from the live upstream, verify the SHA256, extract, spawn the service,
//! confirm it's listening, then stop it and confirm nothing is left behind.
//!
//! These are `#[ignore]`d because they hit the network and take minutes. They
//! exist because the unit tests elsewhere only cover pure functions. They say
//! nothing about whether an extracted binary is actually runnable on this OS,
//! or whether `kill_tree`'s SIGTERM-to-the-process-group really reaps a
//! service. Those are precisely the things that were broken.
//!
//! Run with:
//!   cargo test --lib -- --ignored --test-threads=1 integration

use crate::downloads;
use crate::services::mailhog::MailhogService;
use crate::services::mysql::{MysqlInstall, MysqlService};
use crate::services::{Service, ServiceStatus};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Ports deliberately off the defaults: this machine may well have a distro
/// MySQL/MariaDB on 3306, and binding the real port would either fail or,
/// worse, appear to succeed against someone else's server.
const TEST_MYSQL_PORT: u16 = 13306;
const TEST_MAILHOG_UI_PORT: u16 = 18025;
const TEST_MAILHOG_SMTP_PORT: u16 = 11025;

/// `php-cgi` with the platform's executable suffix. The production copy of
/// this lives in services::apache as a private helper that returns a
/// config-ready string; the tests want a plain filename to join.
fn php_cgi_file() -> String {
    format!("php-cgi{}", std::env::consts::EXE_SUFFIX)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("lamp-bench-integration").join(name);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Poll until something accepts a TCP connection on the port, or give up.
fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn port_is_free(port: u16) -> bool {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err()
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

/// MailHog: a `raw_file` entry. Proves the download writes the
/// platform-specific filename (`MailHog`, not `MailHog.exe`) and chmods it,
/// without which the service could never spawn.
#[test]
#[ignore]
fn integration_mailhog_downloads_and_runs() {
    let resources = scratch("mailhog-resources");
    let runtime = scratch("mailhog-runtime");

    downloads::download("mailhog", &resources, None, None).expect("download mailhog");

    let bin = resources
        .join("mailhog")
        .join(format!("MailHog{}", std::env::consts::EXE_SUFFIX));
    assert!(bin.is_file(), "expected {} on disk", bin.display());
    assert!(
        is_executable(&bin),
        "{} is not executable, the service would fail to spawn",
        bin.display()
    );

    assert!(
        port_is_free(TEST_MAILHOG_UI_PORT),
        "port {TEST_MAILHOG_UI_PORT} is already in use; can't run this test cleanly"
    );

    let mut svc = MailhogService::new(resources.join("mailhog"), runtime);
    svc.set_ports(TEST_MAILHOG_UI_PORT, TEST_MAILHOG_SMTP_PORT);
    svc.start().expect("start mailhog");

    assert!(
        wait_for_port(TEST_MAILHOG_UI_PORT, Duration::from_secs(20)),
        "MailHog never started listening on {TEST_MAILHOG_UI_PORT}"
    );
    assert!(matches!(svc.status(), ServiceStatus::Running { .. }));

    svc.stop().expect("stop mailhog");
    assert!(matches!(svc.status(), ServiceStatus::Stopped));
    assert!(
        wait_for_free(TEST_MAILHOG_UI_PORT, Duration::from_secs(15)),
        "port {TEST_MAILHOG_UI_PORT} still held after stop, the process leaked"
    );
}

/// MySQL: an archive entry, and the demanding one. It covers tar.xz
/// decompression of a 59 MB upstream tarball, `strip_root_dir`, the executable
/// bit, symlinked shared libraries, `mysqld --initialize-insecure`, the
/// generated my.cnf (including the Unix socket path), and finally whether
/// `kill_tree` shuts a forking server down cleanly rather than orphaning it.
#[test]
#[ignore]
fn integration_mysql_downloads_and_runs() {
    let resources = scratch("mysql-resources");
    let runtime = scratch("mysql-runtime");

    downloads::download("mysql-8.0", &resources, None, None).expect("download mysql-8.0");

    let mysqld = crate::services::bin_path(&resources.join("mysql-8.0"), "mysqld");
    assert!(mysqld.is_file(), "expected {} on disk", mysqld.display());
    assert!(is_executable(&mysqld), "{} is not executable", mysqld.display());

    assert!(
        port_is_free(TEST_MYSQL_PORT),
        "port {TEST_MYSQL_PORT} is already in use; can't run this test cleanly"
    );

    let mut svc = MysqlService::new(
        vec![MysqlInstall {
            version: "8.0".into(),
            dir: resources.join("mysql-8.0"),
        }],
        "8.0".into(),
        runtime.clone(),
    );
    svc.set_port(TEST_MYSQL_PORT);

    // First start also runs --initialize-insecure, which takes a while.
    svc.start().expect("start mysqld");
    let listening = wait_for_port(TEST_MYSQL_PORT, Duration::from_secs(120));
    if !listening {
        // The server's own log is far more informative than "port closed".
        let log = runtime.join("mysql-8.0").join("mysql.log");
        let tail = std::fs::read_to_string(&log).unwrap_or_default();
        panic!(
            "mysqld never listened on {TEST_MYSQL_PORT}. Log tail:\n{}",
            tail.lines().rev().take(30).collect::<Vec<_>>().join("\n")
        );
    }
    assert!(matches!(svc.status(), ServiceStatus::Running { .. }));

    // The socket has to live in our runtime dir, not /tmp, or it would
    // collide with a distro MySQL/MariaDB on the same machine.
    let socket = runtime.join("mysql-8.0").join("mysql.sock");
    assert!(
        socket.exists(),
        "expected the Unix socket at {}, the my.cnf override didn't take",
        socket.display()
    );

    svc.stop().expect("stop mysqld");
    assert!(
        wait_for_free(TEST_MYSQL_PORT, Duration::from_secs(30)),
        "port {TEST_MYSQL_PORT} still held after stop, mysqld leaked"
    );
}

/// The one that matters: Apache serving a PHP page through mod_fcgid, using
/// the generated httpd.conf, on Linux. Covers the MPM and unixd DSOs loading,
/// mod_fcgid being found in modules-extra, php-cgi spawning with PHPRC
/// pointing at the php.ini we wrote, and PHP actually executing.
///
/// Needs the Linux service binaries in the repo's `resources/`, which don't
/// exist upstream, build them first (see the build-linux-binaries workflow).
#[test]
#[ignore]
fn integration_apache_serves_php() {
    use crate::services::apache::{ApacheService, PhpInstall};
    use crate::ssl::LocalCa;

    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .to_path_buf();
    let resources = repo.join("resources");
    let apache_dir = resources.join("apache");
    let php_dir = resources.join("php-8.4");

    let httpd = crate::services::bin_path(&apache_dir, "httpd");
    if !httpd.exists() || !php_dir.join(php_cgi_file()).exists() {
        panic!(
            "needs {} and {}/{}, build the Linux binaries first",
            httpd.display(),
            php_dir.display(),
            php_cgi_file()
        );
    }

    // A polkit prompt mid-test would stall forever with nobody to answer it.
    std::env::set_var("LAMP_BENCH_SKIP_CA_TRUST", "1");

    let runtime = scratch("apache-runtime");
    let htdocs = scratch("apache-htdocs");
    // ensure_pma_config writes config.inc.php into this dir, so it has to be
    // real even though this test never loads phpMyAdmin.
    let pma = scratch("apache-pma");
    // Reports the effective extension_dir. That value comes from the managed
    // block php::ensure_managed_ini writes, so seeing our own path proves
    // php-cgi actually read the php.ini we generated, the thing PHPRC is
    // there to guarantee. Checking for a specific extension wouldn't work:
    // the managed block deliberately enables none, leaving that to the user
    // via the Versions panel.
    std::fs::write(
        htdocs.join("probe.php"),
        "<?php echo 'lamp-bench-ok:' . PHP_VERSION . ':' . ini_get('extension_dir');",
    )
    .expect("write probe.php");

    const HTTP_PORT: u16 = 18080;
    const HTTPS_PORT: u16 = 18443;
    assert!(
        port_is_free(HTTP_PORT),
        "port {HTTP_PORT} is already in use; can't run this test cleanly"
    );

    let php_dir_for_assert = php_dir.clone();
    let mut svc = ApacheService::new(
        apache_dir,
        pma,
        vec![PhpInstall {
            version: "8.4".into(),
            dir: php_dir,
        }],
        "8.4".into(),
        LocalCa::new(runtime.join("ca")),
        runtime.join("ssl"),
        runtime.clone(),
        htdocs,
    );
    svc.set_ports(HTTP_PORT, HTTPS_PORT, 3306, 1025);
    svc.start().expect("start httpd");

    if !wait_for_port(HTTP_PORT, Duration::from_secs(30)) {
        let log = runtime.join("apache").join("logs").join("error.log");
        let tail = std::fs::read_to_string(&log).unwrap_or_default();
        let _ = svc.stop();
        panic!(
            "httpd never listened on {HTTP_PORT}. error.log tail:\n{}",
            tail.lines().rev().take(30).collect::<Vec<_>>().join("\n")
        );
    }

    // Kept as two steps rather than chained: mapping through a Result whose
    // error variant is ureq::Error trips clippy::result_large_err.
    let fail = |what: &str| -> ! {
        let log = runtime.join("apache").join("logs").join("error.log");
        let tail = std::fs::read_to_string(&log).unwrap_or_default();
        panic!(
            "{what}\nerror.log tail:\n{}",
            tail.lines().rev().take(30).collect::<Vec<_>>().join("\n")
        )
    };
    let response = match ureq::get(&format!("http://127.0.0.1:{HTTP_PORT}/probe.php")).call() {
        Ok(r) => r,
        Err(e) => fail(&format!("GET /probe.php failed: {e}")),
    };
    let body = match response.into_string() {
        Ok(b) => b,
        Err(e) => fail(&format!("reading the response body failed: {e}")),
    };

    svc.stop().expect("stop httpd");

    assert!(
        body.starts_with("lamp-bench-ok:"),
        "PHP did not execute, got: {body}"
    );
    let expected_ext_dir = php_dir_for_assert.join("ext");
    assert!(
        body.contains(&expected_ext_dir.to_string_lossy().replace('\\', "/")),
        "php.ini wasn't picked up, extension_dir should be {}, got: {body}",
        expected_ext_dir.display()
    );
    assert!(
        wait_for_free(HTTP_PORT, Duration::from_secs(20)),
        "port {HTTP_PORT} still held after stop, httpd workers leaked, which is \
         exactly what the old single-PID SIGKILL did"
    );
}

/// nginx serving PHP through a php-cgi pool. Separate from the Apache test
/// because the failure modes are different: nginx daemonises on Unix (so the
/// supervisor loses it without `daemon off`), and the pool is spawned by us
/// rather than by the web server.
#[test]
#[ignore]
fn integration_nginx_serves_php() {
    use crate::services::apache::PhpInstall;
    use crate::services::nginx::NginxService;
    use crate::ssl::LocalCa;

    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .to_path_buf();
    let resources = repo.join("resources");
    let nginx_dir = resources.join("nginx");
    let php_dir = resources.join("php-8.4");

    let nginx_bin = nginx_dir.join(format!("nginx{}", std::env::consts::EXE_SUFFIX));
    if !nginx_bin.exists() || !php_dir.join(php_cgi_file()).exists() {
        panic!(
            "needs {} and PHP in {}, build the Linux binaries first",
            nginx_bin.display(),
            php_dir.display()
        );
    }

    std::env::set_var("LAMP_BENCH_SKIP_CA_TRUST", "1");

    // The generated default server block roots at <nginx>/html.
    let docroot = nginx_dir.join("html");
    std::fs::write(
        docroot.join("probe.php"),
        "<?php echo 'nginx-ok:' . PHP_VERSION;",
    )
    .expect("write probe.php");

    const HTTP_PORT: u16 = 18081;
    const HTTPS_PORT: u16 = 18444;
    assert!(
        port_is_free(HTTP_PORT),
        "port {HTTP_PORT} is already in use; can't run this test cleanly"
    );

    let runtime = scratch("nginx-runtime");
    let mut svc = NginxService::new(
        nginx_dir,
        runtime.clone(),
        runtime.join("ssl"),
        docroot.clone(),
        LocalCa::new(runtime.join("ca")),
        vec![PhpInstall {
            version: "8.4".into(),
            dir: php_dir,
        }],
        "8.4".into(),
    );
    svc.set_ports(HTTP_PORT, HTTPS_PORT, 1025);
    svc.start().expect("start nginx");

    if !wait_for_port(HTTP_PORT, Duration::from_secs(30)) {
        let log = runtime.join("nginx").join("logs").join("error.log");
        let tail = std::fs::read_to_string(&log).unwrap_or_default();
        let _ = svc.stop();
        panic!(
            "nginx never listened on {HTTP_PORT}. error.log tail:\n{}",
            tail.lines().rev().take(30).collect::<Vec<_>>().join("\n")
        );
    }

    let body = match ureq::get(&format!("http://127.0.0.1:{HTTP_PORT}/probe.php")).call() {
        Ok(r) => r.into_string().expect("read body"),
        Err(e) => {
            let _ = svc.stop();
            panic!("GET /probe.php failed: {e}");
        }
    };

    svc.stop().expect("stop nginx");
    let _ = std::fs::remove_file(docroot.join("probe.php"));

    assert!(
        body.starts_with("nginx-ok:"),
        "PHP did not execute through the php-cgi pool, got: {body}"
    );
    assert!(
        wait_for_free(HTTP_PORT, Duration::from_secs(20)),
        "port {HTTP_PORT} still held after stop, nginx daemonised out from under us"
    );
}

/// Inverse of `wait_for_port`: poll until nothing answers any more.
fn wait_for_free(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if port_is_free(port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}
