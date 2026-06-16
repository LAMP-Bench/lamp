use super::{hidden_command, kill_tree, posix, Service, ServiceStatus};
use crate::hosts::Host;
use crate::services::apache::PhpInstall;
use crate::ssl::LocalCa;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;

pub const HTTP_PORT: u16 = 8081;
pub const SSL_PORT: u16 = 8444;
/// First port of the php-cgi pool range. Each PHP version gets the next port
/// (8.3 → 9183, 8.4 → 9184, …).
pub const PHP_BASE_PORT: u16 = 9183;

pub struct NginxService {
    nginx_dir: PathBuf,
    runtime_dir: PathBuf,
    ssl_dir: PathBuf,
    ca: LocalCa,
    php_installs: Vec<PhpInstall>,
    default_php: String,
    hosts: Vec<Host>,
    nginx_child: Option<Child>,
    php_pools: Vec<Child>,
}

impl NginxService {
    pub fn new(
        nginx_dir: PathBuf,
        runtime_dir: PathBuf,
        ssl_dir: PathBuf,
        ca: LocalCa,
        php_installs: Vec<PhpInstall>,
        default_php: String,
    ) -> Self {
        Self {
            nginx_dir,
            runtime_dir,
            ssl_dir,
            ca,
            php_installs,
            default_php,
            hosts: Vec::new(),
            nginx_child: None,
            php_pools: Vec::new(),
        }
    }

    pub fn set_hosts(&mut self, hosts: Vec<Host>) {
        self.hosts = hosts;
    }

    pub fn set_php_installs(&mut self, installs: Vec<PhpInstall>) {
        if !installs.is_empty() {
            self.php_installs = installs;
        }
    }

    pub fn reload(&mut self) -> Result<(), String> {
        if self.nginx_child.is_some() {
            self.stop()?;
            self.start()?;
        }
        Ok(())
    }

    fn php_port(&self, version: &str) -> u16 {
        for (i, p) in self.php_installs.iter().enumerate() {
            if p.version == version {
                return PHP_BASE_PORT + (i as u16);
            }
        }
        for (i, p) in self.php_installs.iter().enumerate() {
            if p.version == self.default_php {
                return PHP_BASE_PORT + (i as u16);
            }
        }
        PHP_BASE_PORT
    }

    fn default_php_port(&self) -> u16 {
        self.php_port(&self.default_php)
    }

    fn ensure_ssl(&self) -> Result<(), String> {
        self.ca.ensure()?;
        let _ = crate::ssl::trust::ensure_trusted(&self.ca.cert_path());
        self.ca.issue_leaf("localhost", &self.ssl_dir)?;
        for h in &self.hosts {
            self.ca.issue_leaf(&h.name, &self.ssl_dir)?;
        }
        Ok(())
    }

    fn ensure_conf(&self) -> Result<PathBuf, String> {
        let conf_dir = self.runtime_dir.join("nginx");
        let logs_dir = conf_dir.join("logs");
        let temp_dir = conf_dir.join("temp");
        fs::create_dir_all(&logs_dir).map_err(|e| e.to_string())?;
        fs::create_dir_all(temp_dir.join("client_body")).map_err(|e| e.to_string())?;
        fs::create_dir_all(temp_dir.join("fastcgi")).map_err(|e| e.to_string())?;
        fs::create_dir_all(temp_dir.join("proxy")).map_err(|e| e.to_string())?;
        fs::create_dir_all(temp_dir.join("uwsgi")).map_err(|e| e.to_string())?;
        fs::create_dir_all(temp_dir.join("scgi")).map_err(|e| e.to_string())?;
        let _ = self.ensure_ssl();

        let conf_path = conf_dir.join("nginx.conf");
        let conf = build_conf(
            &self.nginx_dir,
            &conf_dir,
            &self.ssl_dir,
            &self.hosts,
            self.default_php_port(),
            |v| self.php_port(v),
        );
        fs::write(&conf_path, conf).map_err(|e| e.to_string())?;
        Ok(conf_path)
    }

    fn spawn_php_pools(&mut self) -> Result<(), String> {
        for (i, p) in self.php_installs.iter().enumerate() {
            let port = PHP_BASE_PORT + (i as u16);
            let php_cgi = p.dir.join("php-cgi.exe");
            if !php_cgi.exists() {
                return Err(format!(
                    "php-cgi not found at {} (PHP {})",
                    php_cgi.display(),
                    p.version
                ));
            }
            let child = hidden_command(&php_cgi)
                .arg("-b")
                .arg(format!("127.0.0.1:{port}"))
                .spawn()
                .map_err(|e| format!("spawn php-cgi {}: {e}", p.version))?;
            self.php_pools.push(child);
        }
        Ok(())
    }
}

impl Service for NginxService {
    fn start(&mut self) -> Result<(), String> {
        if self.nginx_child.is_some() {
            return Ok(());
        }
        // php-cgi pools first so nginx workers can talk to them on the very
        // first request.
        self.spawn_php_pools()?;

        let conf = self.ensure_conf()?;
        let nginx = self.nginx_dir.join("nginx.exe");
        if !nginx.exists() {
            // Best-effort: tear down the pools we just spawned.
            for mut c in self.php_pools.drain(..) {
                kill_tree(&mut c);
            }
            return Err(format!("nginx.exe not found at {}", nginx.display()));
        }
        let child = hidden_command(&nginx)
            .arg("-p")
            .arg(&self.nginx_dir)
            .arg("-c")
            .arg(&conf)
            .spawn()
            .map_err(|e| format!("spawn nginx: {e}"))?;
        self.nginx_child = Some(child);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        if let Some(mut c) = self.nginx_child.take() {
            kill_tree(&mut c);
        }
        for mut c in self.php_pools.drain(..) {
            kill_tree(&mut c);
        }
        Ok(())
    }

    fn status(&mut self) -> ServiceStatus {
        match self.nginx_child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(None) => ServiceStatus::Running { pid: child.id() },
                Ok(Some(_)) => {
                    self.nginx_child = None;
                    for mut c in self.php_pools.drain(..) {
                        kill_tree(&mut c);
                    }
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
    nginx_dir: &Path,
    runtime_dir: &Path,
    ssl_dir: &Path,
    hosts: &[Host],
    default_php_port: u16,
    php_port_for: impl Fn(&str) -> u16,
) -> String {
    let nginx_conf = posix(&nginx_dir.join("conf"));
    let runtime = posix(runtime_dir);
    let ssl = posix(ssl_dir);
    let default_root = posix(&nginx_dir.join("html"));

    let mut out = format!(
        "# Generated by Lamp Bench. Do not edit by hand.\n\
         worker_processes 1;\n\
         error_log \"{runtime}/logs/error.log\" warn;\n\
         pid \"{runtime}/logs/nginx.pid\";\n\
         \n\
         events {{\n\
         \x20   worker_connections 1024;\n\
         }}\n\
         \n\
         http {{\n\
         \x20   include \"{nginx_conf}/mime.types\";\n\
         \x20   default_type application/octet-stream;\n\
         \x20   client_body_temp_path \"{runtime}/temp/client_body\";\n\
         \x20   fastcgi_temp_path \"{runtime}/temp/fastcgi\";\n\
         \x20   proxy_temp_path \"{runtime}/temp/proxy\";\n\
         \x20   uwsgi_temp_path \"{runtime}/temp/uwsgi\";\n\
         \x20   scgi_temp_path \"{runtime}/temp/scgi\";\n\
         \x20   sendfile on;\n\
         \x20   server_tokens off;\n\
         \n",
        runtime = runtime,
        nginx_conf = nginx_conf,
    );

    // Default vhost (HTTP + HTTPS) — uses default PHP version
    out.push_str(&format!(
        "\x20   server {{\n\
         \x20       listen {HTTP_PORT};\n\
         \x20       listen {SSL_PORT} ssl;\n\
         \x20       ssl_certificate \"{ssl}/localhost.crt\";\n\
         \x20       ssl_certificate_key \"{ssl}/localhost.key\";\n\
         \x20       server_name localhost;\n\
         \x20       root \"{default_root}\";\n\
         \x20       index index.php index.html;\n\
         \x20       location ~ \\.php$ {{\n\
         \x20           fastcgi_pass 127.0.0.1:{default_php_port};\n\
         \x20           fastcgi_index index.php;\n\
         \x20           fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;\n\
         \x20           include \"{nginx_conf}/fastcgi_params\";\n\
         \x20       }}\n\
         \x20   }}\n\
         \n",
    ));

    // Per-host vhosts (HTTP + HTTPS combined into one server block)
    for host in hosts {
        let docroot = posix(Path::new(&host.docroot));
        let port = php_port_for(&host.php_version);
        let name = &host.name;
        let extras = render_nginx_extras(&host.nginx_extra);
        out.push_str(&format!(
            "\x20   server {{\n\
             \x20       listen {HTTP_PORT};\n\
             \x20       listen {SSL_PORT} ssl;\n\
             \x20       ssl_certificate \"{ssl}/{name}.crt\";\n\
             \x20       ssl_certificate_key \"{ssl}/{name}.key\";\n\
             \x20       server_name {name};\n\
             \x20       root \"{docroot}\";\n\
             \x20       index index.php index.html;\n\
             \x20       location ~ \\.php$ {{\n\
             \x20           fastcgi_pass 127.0.0.1:{port};\n\
             \x20           fastcgi_index index.php;\n\
             \x20           fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;\n\
             \x20           include \"{nginx_conf}/fastcgi_params\";\n\
             \x20       }}\n\
             {extras}\
             \x20   }}\n\
             \n",
        ));
    }

    out.push_str("}\n");
    out
}

fn render_nginx_extras(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for line in trimmed.lines() {
        out.push_str("        ");
        out.push_str(line);
        out.push('\n');
    }
    out
}
