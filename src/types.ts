export type ServiceStatus =
  | { kind: "stopped" }
  | { kind: "running"; pid: number }
  | { kind: "error"; message: string };

export type Host = {
  id: number;
  name: string;
  docroot: string;
  php_version: string;
  apache_extra: string;
  nginx_extra: string;
};

export type SectionId =
  | "home"
  | "hosts"
  | "tools"
  | "config"
  | "logs"
  | "settings";

export type PhpExtension = {
  name: string;
  enabled: boolean;
};

export type LogName = "apache" | "mysql" | "nginx" | "redis" | "mailhog";

/// Why a start/stop attempt failed. `backend` carries the message the Rust
/// side returned; `exited` means the spawn itself succeeded but the process
/// was gone moments later — almost always a busy port or a bad config, and
/// the component translates it rather than showing a raw string.
export type ServiceError =
  | { kind: "backend"; message: string }
  | { kind: "exited" };

export type ServiceName =
  | "apache"
  | "mysql"
  | "nginx"
  | "redis"
  | "mailhog";

export type PhpCatalogEntry = {
  version: string;
  installed: boolean;
};

export type DeployProfile = {
  host_id: number;
  protocol: string;
  ftp_host: string;
  ftp_port: number;
  ftp_user: string;
  ftp_password: string;
  remote_dir: string;
};

export type DeployReport = {
  files_uploaded: number;
  bytes_uploaded: number;
  errors: string[];
};

export type Snapshot = {
  id: number;
  host_id: number;
  label: string;
  path: string;
  size_bytes: number;
  created_at: string;
  has_db: boolean;
  mysql_version: string;
};
