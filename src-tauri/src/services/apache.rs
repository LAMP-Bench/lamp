use super::{bin_path, kill_tree, posix, Service, ServiceStatus};
use crate::hosts::Host;
use crate::ssl::{LocalCa, SSL_PORT};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

pub const DEFAULT_PORT: u16 = 8080;

#[derive(Debug, Clone)]
pub struct PhpInstall {
    pub version: String,
    pub dir: PathBuf,
}

pub struct ApacheService {
    apache_dir: PathBuf,
    pma_dir: PathBuf,
    php_installs: Vec<PhpInstall>,
    default_php: String,
    runtime_dir: PathBuf,
    ca: LocalCa,
    ssl_dir: PathBuf,
    /// User-facing htdocs root. Default vhost serves from here, CMS one-click
    /// installs land here. Lives next to runtime state, not under
    /// `resources/apache/` (which is read-only after install).
    htdocs_dir: PathBuf,
    port: u16,
    hosts: Vec<Host>,
    child: Option<Child>,
}

impl ApacheService {
    pub fn new(
        apache_dir: PathBuf,
        pma_dir: PathBuf,
        php_installs: Vec<PhpInstall>,
        default_php: String,
        ca: LocalCa,
        ssl_dir: PathBuf,
        runtime_dir: PathBuf,
        htdocs_dir: PathBuf,
    ) -> Self {
        Self {
            apache_dir,
            pma_dir,
            php_installs,
            default_php,
            ca,
            ssl_dir,
            runtime_dir,
            htdocs_dir,
            port: DEFAULT_PORT,
            hosts: Vec::new(),
            child: None,
        }
    }

    pub fn set_hosts(&mut self, hosts: Vec<Host>) {
        self.hosts = hosts;
    }

    /// Refresh the list of available PHP versions from disk. Callers do this
    /// right before `start()` / `reload()` so newly-downloaded PHP versions
    /// (via the on-demand runtime fetch) get picked up without restarting
    /// the whole app.
    pub fn set_php_installs(&mut self, installs: Vec<PhpInstall>) {
        if !installs.is_empty() {
            self.php_installs = installs;
        }
    }

    pub fn available_php_versions(&self) -> Vec<String> {
        self.php_installs.iter().map(|p| p.version.clone()).collect()
    }

    pub fn reload(&mut self) -> Result<(), String> {
        if self.child.is_some() {
            self.stop()?;
            self.start()?;
        }
        Ok(())
    }

    fn php_install(&self, version: &str) -> &PhpInstall {
        self.php_installs
            .iter()
            .find(|p| p.version == version)
            .or_else(|| {
                self.php_installs
                    .iter()
                    .find(|p| p.version == self.default_php)
            })
            .expect("at least one PHP install configured")
    }

    fn ensure_all_php_inis(&self) -> Result<(), String> {
        for p in &self.php_installs {
            let ini = p.dir.join("php.ini");
            if ini.exists() {
                continue;
            }
            let template = p.dir.join("php.ini-development");
            let mut content = fs::read_to_string(&template)
                .map_err(|e| format!("read {}: {e}", template.display()))?;
            content.push_str(&format!(
                "\n\
                 ; --- Lamp Bench overrides ---\n\
                 extension_dir = \"{ext}\"\n\
                 extension=mysqli\n\
                 extension=pdo_mysql\n\
                 extension=curl\n\
                 extension=mbstring\n\
                 extension=openssl\n\
                 extension=gd\n\
                 extension=intl\n\
                 extension=zip\n\
                 extension=fileinfo\n\
                 extension=exif\n\
                 \n\
                 ; OPcache (Zend extension on Windows)\n\
                 zend_extension=opcache\n\
                 opcache.enable=1\n\
                 opcache.enable_cli=0\n\
                 \n\
                 ; Xdebug 3 — develop mode is always on (pretty errors),\n\
                 ; debugger only attaches when the request carries an\n\
                 ; XDEBUG_TRIGGER cookie/GET/POST. Use the IDE's \"Listen for\n\
                 ; Xdebug\" button + a browser extension to step through code.\n\
                 zend_extension=xdebug\n\
                 xdebug.mode=develop,debug\n\
                 xdebug.start_with_request=trigger\n\
                 xdebug.client_host=127.0.0.1\n\
                 xdebug.client_port=9003\n\
                 xdebug.discover_client_host=0\n\
                 \n\
                 ; Mail capture via MailHog — listening on 127.0.0.1:1025\n\
                 ; when the Mailhog service is running. PHP's mail() routes\n\
                 ; here so the user can inspect emails in the MailHog web UI\n\
                 ; at http://localhost:8025 instead of trying to deliver.\n\
                 [mail function]\n\
                 SMTP = 127.0.0.1\n\
                 smtp_port = 1025\n\
                 sendmail_from = noreply@localhost\n",
                ext = posix(&p.dir.join("ext"))
            ));
            fs::write(&ini, content).map_err(|e| format!("write {}: {e}", ini.display()))?;
        }
        Ok(())
    }

    fn ensure_pma_config(&self) -> Result<(), String> {
        let pma_runtime = self.runtime_dir.join("phpmyadmin");
        let tmp = pma_runtime.join("tmp");
        let twig = tmp.join("twig");
        if twig.exists() {
            let _ = fs::remove_dir_all(&twig);
        }
        fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

        let cfg_path = self.pma_dir.join("config.inc.php");
        let cfg = build_pma_config(&tmp);
        fs::write(&cfg_path, cfg).map_err(|e| format!("write phpMyAdmin config: {e}"))?;
        Ok(())
    }

    /// Bring CA + per-host leaf certs into existence. Best-effort trust store
    /// install — non-fatal if it fails (user can `Trust CA` from the UI later).
    fn ensure_ssl(&self) -> Result<Vec<(String, PathBuf, PathBuf)>, String> {
        self.ca.ensure()?;
        let _ = crate::ssl::trust::ensure_trusted(&self.ca.cert_path());

        // localhost leaf for the default vhost
        let mut out = Vec::new();
        let localhost = self.ca.issue_leaf("localhost", &self.ssl_dir)?;
        out.push(("localhost".to_string(), localhost.cert_path, localhost.key_path));

        for h in &self.hosts {
            let leaf = self.ca.issue_leaf(&h.name, &self.ssl_dir)?;
            out.push((h.name.clone(), leaf.cert_path, leaf.key_path));
        }
        Ok(out)
    }

    fn ensure_conf(&self) -> Result<PathBuf, String> {
        let conf_dir = self.runtime_dir.join("apache");
        fs::create_dir_all(conf_dir.join("logs")).map_err(|e| e.to_string())?;
        self.ensure_all_php_inis()?;
        self.ensure_pma_config()?;
        let _ = self.ensure_ssl();

        let conf_path = conf_dir.join("httpd.conf");
        let conf = build_conf(
            &self.apache_dir,
            &self.pma_dir,
            &self.php_install(&self.default_php).dir,
            &conf_dir,
            self.port,
            &self.hosts,
            &self.ssl_dir,
            &self.htdocs_dir,
            |version| self.php_install(version).dir.clone(),
        );
        fs::write(&conf_path, conf).map_err(|e| e.to_string())?;
        Ok(conf_path)
    }
}

impl Service for ApacheService {
    fn start(&mut self) -> Result<(), String> {
        if self.child.is_some() {
            return Ok(());
        }
        let conf = self.ensure_conf()?;
        let httpd = bin_path(&self.apache_dir, "httpd");
        if !httpd.exists() {
            return Err(format!("httpd binary not found at {}", httpd.display()));
        }
        let child = Command::new(&httpd)
            .arg("-f")
            .arg(&conf)
            .arg("-d")
            .arg(&self.apache_dir)
            .spawn()
            .map_err(|e| format!("failed to spawn httpd: {e}"))?;
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        if let Some(mut child) = self.child.take() {
            kill_tree(&mut child);
        }
        Ok(())
    }

    fn status(&mut self) -> ServiceStatus {
        match self.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(None) => ServiceStatus::Running { pid: child.id() },
                Ok(Some(_)) => {
                    self.child = None;
                    ServiceStatus::Stopped
                }
                Err(e) => ServiceStatus::Error {
                    message: e.to_string(),
                },
            },
            None => ServiceStatus::Stopped,
        }
    }
}

fn build_conf(
    apache_dir: &Path,
    pma_dir: &Path,
    default_php_dir: &Path,
    runtime_dir: &Path,
    port: u16,
    hosts: &[Host],
    ssl_dir: &Path,
    htdocs_dir: &Path,
    php_dir_for: impl Fn(&str) -> PathBuf,
) -> String {
    let server_root = posix(apache_dir);
    let runtime = posix(runtime_dir);
    let pma = posix(pma_dir);
    let default_cgi = posix(&default_php_dir.join("php-cgi.exe"));
    let ssl = posix(ssl_dir);
    let htdocs = posix(htdocs_dir);
    let ssl_port = SSL_PORT;

    let mut out = format!(
        "# Generated by Lamp Bench. Do not edit by hand.\n\
         ServerRoot \"{server_root}\"\n\
         PidFile \"{runtime}/logs/httpd.pid\"\n\
         ServerName localhost\n\
         Listen {port}\n\
         Listen {ssl_port}\n\
         \n\
         LoadModule authn_core_module modules/mod_authn_core.so\n\
         LoadModule authz_core_module modules/mod_authz_core.so\n\
         LoadModule authz_host_module modules/mod_authz_host.so\n\
         LoadModule log_config_module modules/mod_log_config.so\n\
         LoadModule mime_module modules/mod_mime.so\n\
         LoadModule dir_module modules/mod_dir.so\n\
         LoadModule alias_module modules/mod_alias.so\n\
         LoadModule rewrite_module modules/mod_rewrite.so\n\
         LoadModule actions_module modules/mod_actions.so\n\
         LoadModule fcgid_module modules-extra/mod_fcgid.so\n\
         LoadModule socache_shmcb_module modules/mod_socache_shmcb.so\n\
         LoadModule ssl_module modules/mod_ssl.so\n\
         \n\
         FcgidInitialEnv PATH \"C:/Windows/System32\"\n\
         FcgidIOTimeout 60\n\
         FcgidIdleTimeout 300\n\
         FcgidMaxRequestsPerProcess 1000\n\
         \n\
         SSLSessionCache \"shmcb:{runtime}/logs/ssl_scache(512000)\"\n\
         SSLSessionCacheTimeout 300\n\
         SSLProtocol all -SSLv3 -TLSv1 -TLSv1.1\n\
         \n\
         DirectoryIndex index.php index.html\n\
         \n\
         ErrorLog \"{runtime}/logs/error.log\"\n\
         LogLevel warn\n\
         \n\
         Alias /phpmyadmin \"{pma}\"\n\
         <Directory \"{pma}\">\n\
         \x20   Options Indexes FollowSymLinks ExecCGI\n\
         \x20   AllowOverride All\n\
         \x20   Require all granted\n\
         \x20   <FilesMatch \\.php$>\n\
         \x20       SetHandler fcgid-script\n\
         \x20   </FilesMatch>\n\
         \x20   FcgidWrapper \"{default_cgi}\" .php\n\
         </Directory>\n\
         \n"
    );

    // Default catch-all vhost (HTTP + HTTPS) — serves from the user-facing
    // htdocs dir, NOT the bundled Apache welcome page. CMS one-click installs
    // and stray project folders both live there, accessible as
    // `localhost:8080/<project>/`.
    out.push_str(&format!(
        "<VirtualHost *:{port}>\n\
         \x20   DocumentRoot \"{htdocs}\"\n\
         \x20   <Directory \"{htdocs}\">\n\
         \x20       Options Indexes FollowSymLinks ExecCGI\n\
         \x20       AllowOverride All\n\
         \x20       Require all granted\n\
         \x20       <FilesMatch \\.php$>\n\
         \x20           SetHandler fcgid-script\n\
         \x20       </FilesMatch>\n\
         \x20       FcgidWrapper \"{default_cgi}\" .php\n\
         \x20   </Directory>\n\
         </VirtualHost>\n\n"
    ));

    out.push_str(&format!(
        "<VirtualHost *:{ssl_port}>\n\
         \x20   DocumentRoot \"{htdocs}\"\n\
         \x20   SSLEngine on\n\
         \x20   SSLCertificateFile \"{ssl}/localhost.crt\"\n\
         \x20   SSLCertificateKeyFile \"{ssl}/localhost.key\"\n\
         \x20   <Directory \"{htdocs}\">\n\
         \x20       Options Indexes FollowSymLinks ExecCGI\n\
         \x20       AllowOverride All\n\
         \x20       Require all granted\n\
         \x20       <FilesMatch \\.php$>\n\
         \x20           SetHandler fcgid-script\n\
         \x20       </FilesMatch>\n\
         \x20       FcgidWrapper \"{default_cgi}\" .php\n\
         \x20   </Directory>\n\
         </VirtualHost>\n\n"
    ));

    for host in hosts {
        let docroot = posix(Path::new(&host.docroot));
        let cgi = posix(&php_dir_for(&host.php_version).join("php-cgi.exe"));
        let extras = render_extras(&host.apache_extra);
        let host_inner = format!(
            "\x20   ServerName {name}\n\
             \x20   DocumentRoot \"{docroot}\"\n\
             \x20   <Directory \"{docroot}\">\n\
             \x20       Options Indexes FollowSymLinks ExecCGI\n\
             \x20       AllowOverride All\n\
             \x20       Require all granted\n\
             \x20       <FilesMatch \\.php$>\n\
             \x20           SetHandler fcgid-script\n\
             \x20       </FilesMatch>\n\
             \x20       FcgidWrapper \"{cgi}\" .php\n\
             \x20   </Directory>\n\
             {extras}",
            name = host.name
        );

        // HTTP
        out.push_str(&format!(
            "<VirtualHost *:{port}>\n{host_inner}</VirtualHost>\n\n"
        ));

        // HTTPS
        let name = &host.name;
        out.push_str(&format!(
            "<VirtualHost *:{ssl_port}>\n\
             \x20   SSLEngine on\n\
             \x20   SSLCertificateFile \"{ssl}/{name}.crt\"\n\
             \x20   SSLCertificateKeyFile \"{ssl}/{name}.key\"\n\
             {host_inner}\
             </VirtualHost>\n\n"
        ));
    }

    out
}

/// Indent each line of the user-supplied per-host extras to match the
/// surrounding vhost block, and ensure trailing newline.
fn render_extras(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for line in trimmed.lines() {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn build_pma_config(tmp_dir: &Path) -> String {
    let tmp = posix(tmp_dir);
    format!(
        "<?php\n\
         // Generated by Lamp Bench. Do not edit by hand.\n\
         $cfg['blowfish_secret'] = 'lamp-bench-local-dev-blowfish-32ch-pad';\n\
         $i = 0;\n\
         $i++;\n\
         $cfg['Servers'][$i]['auth_type'] = 'config';\n\
         $cfg['Servers'][$i]['host'] = '127.0.0.1';\n\
         $cfg['Servers'][$i]['port'] = '3306';\n\
         $cfg['Servers'][$i]['user'] = 'root';\n\
         $cfg['Servers'][$i]['password'] = '';\n\
         $cfg['Servers'][$i]['AllowNoPassword'] = true;\n\
         $cfg['TempDir'] = '{tmp}';\n"
    )
}
