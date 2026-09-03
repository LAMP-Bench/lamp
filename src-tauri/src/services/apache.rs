use super::{bin_path, kill_tree, posix, service_command, Service, ServiceStatus};
use crate::hosts::Host;
use crate::ssl::{LocalCa, SSL_PORT};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;

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
    ssl_port: u16,
    /// MySQL port baked into the generated phpMyAdmin config.
    mysql_port: u16,
    /// MailHog SMTP port baked into each php.ini's mail function.
    mailhog_smtp_port: u16,
    hosts: Vec<Host>,
    child: Option<Child>,
}

impl ApacheService {
    #[allow(clippy::too_many_arguments)]
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
            ssl_port: SSL_PORT,
            mysql_port: 3306,
            mailhog_smtp_port: 1025,
            hosts: Vec::new(),
            child: None,
        }
    }

    pub fn set_hosts(&mut self, hosts: Vec<Host>) {
        self.hosts = hosts;
    }

    /// The version the default vhost and phpMyAdmin run on, and the fallback
    /// for a host pointing at a PHP that is no longer installed. Applied per
    /// start so the Settings picker takes effect on the next restart rather
    /// than never.
    pub fn set_default_php(&mut self, version: String) {
        if self.php_installs.iter().any(|p| p.version == version) {
            self.default_php = version;
        }
    }

    /// Apply listen ports + dependency ports read from `service_config`.
    /// `mysql_port`/`mailhog_smtp` keep the generated phpMyAdmin and php.ini
    /// configs in sync when the user moves those services off their defaults.
    pub fn set_ports(&mut self, http: u16, https: u16, mysql_port: u16, mailhog_smtp: u16) {
        self.port = http;
        self.ssl_port = https;
        self.mysql_port = mysql_port;
        self.mailhog_smtp_port = mailhog_smtp;
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

    /// Keep every installed PHP version's `php.ini` current. Delegated to
    /// `php::ensure_managed_ini` so this and the Versions panel can't disagree
    /// about who owns the file — see that function for the history.
    fn ensure_all_php_inis(&self) -> Result<(), String> {
        for p in &self.php_installs {
            crate::php::ensure_managed_ini(&p.dir, self.mailhog_smtp_port)?;
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
        let cfg = build_pma_config(&tmp, self.mysql_port, &self.pma_secret()?);
        fs::write(&cfg_path, cfg).map_err(|e| format!("write phpMyAdmin config: {e}"))?;
        Ok(())
    }

    /// phpMyAdmin's cookie secret. Generated once per install and kept in the
    /// runtime dir — it used to be a constant compiled into every copy of the
    /// app. It only protects phpMyAdmin's own cookie (auth_type is `config`,
    /// so there's no password to steal), but a shared secret is still worse
    /// than a local one for no benefit.
    fn pma_secret(&self) -> Result<String, String> {
        let path = self.runtime_dir.join("phpmyadmin").join("blowfish.secret");
        if let Ok(existing) = fs::read_to_string(&path) {
            let existing = existing.trim().to_string();
            if existing.len() >= 32 {
                return Ok(existing);
            }
        }
        use rand::Rng;
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::thread_rng();
        let secret: String = (0..32)
            .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
            .collect();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&path, &secret).map_err(|e| format!("write pma secret: {e}"))?;
        Ok(secret)
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
        // Not swallowed: the generated conf references these certificates by
        // path, so failing here means Apache won't start and the only clue
        // would be a line in error.log.
        self.ensure_ssl()
            .map_err(|e| format!("could not prepare SSL certificates: {e}"))?;

        let conf_path = conf_dir.join("httpd.conf");
        let conf = build_conf(
            &self.apache_dir,
            &self.pma_dir,
            &self.php_install(&self.default_php).dir,
            &conf_dir,
            self.port,
            self.ssl_port,
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
        let mut cmd = service_command(&httpd);
        cmd.arg("-f").arg(&conf).arg("-d").arg(&self.apache_dir);
        // Unix httpd forks a daemon and the process we spawned exits at once.
        // That loses us the server entirely: status() reports Stopped while
        // Apache is serving, stop() has nothing to kill, and the listening
        // socket stays held. -DFOREGROUND keeps the real server as our direct
        // child. Windows httpd runs in the foreground and doesn't take it.
        #[cfg(unix)]
        cmd.arg("-DFOREGROUND");
        let child = cmd
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

/// `php-cgi` with the platform's executable suffix, forward-slashed for the
/// config file. Hardcoding `.exe` here meant the generated vhosts pointed at
/// a binary that cannot exist off Windows.
fn php_cgi_path(php_dir: &Path) -> String {
    posix(&php_dir.join(format!("php-cgi{}", std::env::consts::EXE_SUFFIX)))
}

/// Where `mod_fcgid.so` sits relative to ServerRoot. The Windows build comes
/// from ApacheLounge as a separate download that we drop into
/// `modules-extra/`; a distro package puts it alongside every other module.
#[cfg(windows)]
const FCGID_MODULE: &str = "modules-extra/mod_fcgid.so";
#[cfg(not(windows))]
const FCGID_MODULE: &str = "modules/mod_fcgid.so";

/// PATH handed to the php-cgi children — they inherit nothing useful
/// otherwise, and anything PHP shells out to needs it. The Windows value was
/// previously baked in unconditionally, so on Unix the CGI processes were
/// handed a path that does not exist.
#[cfg(windows)]
const CGI_PATH: &str = "C:/Windows/System32";
#[cfg(not(windows))]
const CGI_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

#[allow(clippy::too_many_arguments)]
fn build_conf(
    apache_dir: &Path,
    pma_dir: &Path,
    default_php_dir: &Path,
    runtime_dir: &Path,
    port: u16,
    ssl_port: u16,
    hosts: &[Host],
    ssl_dir: &Path,
    htdocs_dir: &Path,
    php_dir_for: impl Fn(&str) -> PathBuf,
) -> String {
    let server_root = posix(apache_dir);
    let runtime = posix(runtime_dir);
    let pma = posix(pma_dir);
    let default_cgi = php_cgi_path(default_php_dir);
    let default_php_root = posix(default_php_dir);

    // Unix httpd needs an MPM and mod_unixd loaded as DSOs; on Windows both
    // are compiled into the server and a LoadModule line for them is a hard
    // startup error. mod_unixd in particular has to come before mod_fcgid,
    // which links against ap_unixd_setup_child, without it httpd refuses to
    // start with "undefined symbol: ap_unixd_setup_child". Guarded with
    // <IfFile> like the optional modules below, so a distro Apache that
    // compiles either of them in still starts.
    let platform_modules = if cfg!(windows) {
        String::new()
    } else {
        "<IfFile \"modules/mod_mpm_event.so\">\n\
         \x20   LoadModule mpm_event_module modules/mod_mpm_event.so\n\
         </IfFile>\n\
         <IfFile \"modules/mod_unixd.so\">\n\
         \x20   LoadModule unixd_module modules/mod_unixd.so\n\
         </IfFile>\n"
            .to_string()
    };
    let default_vhost_env = format!("\x20   FcgidInitialEnv PHPRC \"{default_php_root}\"\n");
    let ssl = posix(ssl_dir);
    let htdocs = posix(htdocs_dir);

    let mut out = format!(
        "# Generated by Lamp Bench. Do not edit by hand.\n\
         ServerRoot \"{server_root}\"\n\
         PidFile \"{runtime}/logs/httpd.pid\"\n\
         ServerName localhost\n\
         Listen {port}\n\
         Listen {ssl_port}\n\
         \n\
         {platform_modules}\
         LoadModule authn_core_module modules/mod_authn_core.so\n\
         LoadModule authz_core_module modules/mod_authz_core.so\n\
         LoadModule authz_host_module modules/mod_authz_host.so\n\
         LoadModule log_config_module modules/mod_log_config.so\n\
         LoadModule mime_module modules/mod_mime.so\n\
         LoadModule dir_module modules/mod_dir.so\n\
         LoadModule alias_module modules/mod_alias.so\n\
         LoadModule rewrite_module modules/mod_rewrite.so\n\
         LoadModule actions_module modules/mod_actions.so\n\
         LoadModule fcgid_module {FCGID_MODULE}\n\
         LoadModule socache_shmcb_module modules/mod_socache_shmcb.so\n\
         LoadModule ssl_module modules/mod_ssl.so\n\
         # Optional, but stock .htaccess files assume them: an unguarded\n\
         # `Header set` or `ExpiresByType` is a hard 500 without these, and\n\
         # `Options Indexes` below does nothing unless autoindex is loaded —\n\
         # a directory with no index returns 403 instead of a listing.\n\
         # <IfFile> (2.4.34+) keeps a build that lacks one from refusing to\n\
         # start at all.\n\
         <IfFile \"modules/mod_headers.so\">\n\
         \x20   LoadModule headers_module modules/mod_headers.so\n\
         </IfFile>\n\
         <IfFile \"modules/mod_expires.so\">\n\
         \x20   LoadModule expires_module modules/mod_expires.so\n\
         </IfFile>\n\
         <IfFile \"modules/mod_deflate.so\">\n\
         \x20   LoadModule deflate_module modules/mod_deflate.so\n\
         </IfFile>\n\
         <IfFile \"modules/mod_env.so\">\n\
         \x20   LoadModule env_module modules/mod_env.so\n\
         </IfFile>\n\
         <IfFile \"modules/mod_setenvif.so\">\n\
         \x20   LoadModule setenvif_module modules/mod_setenvif.so\n\
         </IfFile>\n\
         <IfFile \"modules/mod_autoindex.so\">\n\
         \x20   LoadModule autoindex_module modules/mod_autoindex.so\n\
         </IfFile>\n\
         <IfFile \"modules/mod_auth_basic.so\">\n\
         \x20   LoadModule auth_basic_module modules/mod_auth_basic.so\n\
         </IfFile>\n\
         <IfFile \"modules/mod_authn_file.so\">\n\
         \x20   LoadModule authn_file_module modules/mod_authn_file.so\n\
         </IfFile>\n\
         <IfFile \"modules/mod_authz_user.so\">\n\
         \x20   LoadModule authz_user_module modules/mod_authz_user.so\n\
         </IfFile>\n\
         \n\
         FcgidInitialEnv PATH \"{CGI_PATH}\"\n\
         # PHPRC is where php-cgi looks for php.ini. Windows PHP also checks
         # beside the binary, but Linux and macOS builds use a compiled-in
         # path, so without this they start fine and run with no php.ini at
         # all, no extensions, no Xdebug, no mail routed to MailHog. It is
         # set per vhost below as well, since each vhost pins its own PHP
         # version; this server-level one covers the phpMyAdmin alias.
         # FcgidInitialEnv is only valid in server and virtual-host context,
         # inside <Directory> Apache refuses to start.\n\
         FcgidInitialEnv PHPRC \"{default_php_root}\"\n\
         # php-cgi retires itself after PHP_FCGI_MAX_REQUESTS (default 500)\n\
         # requests. That is lower than FcgidMaxRequestsPerProcess below, so\n\
         # the process would vanish while mod_fcgid still believed it was\n\
         # alive — surfacing as intermittent 500s and \"exit(communication\n\
         # error)\" in the error log. Hand the recycling to mod_fcgid alone.\n\
         FcgidInitialEnv PHP_FCGI_MAX_REQUESTS 0\n\
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
         {default_vhost_env}\
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
         {default_vhost_env}\
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
        let php_root_path = php_dir_for(&host.php_version);
        let cgi = php_cgi_path(&php_root_path);
        let host_env = format!(
            "\x20   FcgidInitialEnv PHPRC \"{}\"\n",
            posix(&php_root_path)
        );
        let extras = render_extras(&host.apache_extra);
        let host_inner = format!(
            "{host_env}\
             \x20   ServerName {name}\n\
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

fn build_pma_config(tmp_dir: &Path, mysql_port: u16, secret: &str) -> String {
    let tmp = posix(tmp_dir);
    format!(
        "<?php\n\
         // Generated by Lamp Bench. Do not edit by hand.\n\
         $cfg['blowfish_secret'] = '{secret}';\n\
         $i = 0;\n\
         $i++;\n\
         $cfg['Servers'][$i]['auth_type'] = 'config';\n\
         $cfg['Servers'][$i]['host'] = '127.0.0.1';\n\
         $cfg['Servers'][$i]['port'] = '{mysql_port}';\n\
         $cfg['Servers'][$i]['user'] = 'root';\n\
         $cfg['Servers'][$i]['password'] = '';\n\
         $cfg['Servers'][$i]['AllowNoPassword'] = true;\n\
         $cfg['TempDir'] = '{tmp}';\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_conf() -> String {
        let host = Host {
            id: 1,
            name: "test.local".into(),
            docroot: "C:/sites/test".into(),
            php_version: "8.4".into(),
            apache_extra: "Header set X-Test \"1\"".into(),
            nginx_extra: String::new(),
        };
        build_conf(
            Path::new("C:/LAMP/resources/apache"),
            Path::new("C:/LAMP/resources/phpmyadmin"),
            Path::new("C:/LAMP/resources/php-8.4"),
            Path::new("C:/LAMP/runtime/apache"),
            8080,
            8443,
            &[host],
            Path::new("C:/LAMP/runtime/ssl"),
            Path::new("C:/LAMP/htdocs"),
            |_| PathBuf::from("C:/LAMP/resources/php-8.4"),
        )
    }

    /// The conf is one large `format!` with hand-escaped quotes and `\x20`
    /// indentation. A mistake there produces a file Apache rejects at
    /// startup, with the reason buried in error.log — cheap to catch here.
    #[test]
    fn generated_conf_is_well_formed() {
        let conf = sample_conf();

        // No unsubstituted placeholders, and no escape sequence that should
        // have been resolved surviving as literal text. Raw strings here on
        // purpose: `"\x20"` would just be a space.
        assert!(!conf.contains('{'), "unsubstituted placeholder in the conf");
        assert!(!conf.contains(r"\x20"), r"literal \x20 leaked into the conf");
        assert!(!conf.contains(r"\n"), r"literal \n leaked into the conf");

        // Every block that opens must close.
        assert_eq!(
            conf.matches("<IfFile ").count(),
            conf.matches("</IfFile>").count(),
            "unbalanced <IfFile> blocks"
        );
        assert_eq!(
            conf.matches("<VirtualHost ").count(),
            conf.matches("</VirtualHost>").count(),
            "unbalanced <VirtualHost> blocks"
        );
        assert_eq!(
            conf.matches("<Directory ").count(),
            conf.matches("</Directory>").count(),
            "unbalanced <Directory> blocks"
        );

        // Quotes are escaped in pairs, so an odd count means one got lost.
        assert_eq!(conf.matches('"').count() % 2, 0, "unbalanced quotes");
    }

    #[test]
    fn conf_carries_the_settings_that_matter() {
        let conf = sample_conf();
        assert!(conf.contains("ServerRoot \"C:/LAMP/resources/apache\""));
        assert!(conf.contains("Listen 8080") && conf.contains("Listen 8443"));
        // The autoindex guard is what makes `Options Indexes` mean anything.
        assert!(conf.contains("LoadModule autoindex_module"));
        // php-cgi resolved through EXE_SUFFIX, not a hardcoded .exe.
        assert!(conf.contains(&format!(
            "php-cgi{}",
            std::env::consts::EXE_SUFFIX
        )));
        // Recycling belongs to mod_fcgid, not to php-cgi's own counter.
        assert!(conf.contains("FcgidInitialEnv PHP_FCGI_MAX_REQUESTS 0"));
        // Per-host extras land inside the vhost, verbatim.
        assert!(conf.contains("Header set X-Test \"1\""));
        assert!(conf.contains("ServerName test.local"));
        // Both the HTTP and HTTPS vhost for the host, plus the two defaults.
        assert_eq!(conf.matches("<VirtualHost ").count(), 4);
    }
}
