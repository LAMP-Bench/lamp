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

export type SectionId = "home" | "hosts" | "tools" | "config" | "logs";

export type LogName = "apache" | "mysql" | "nginx";

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

export type Snapshot = {
  id: number;
  host_id: number;
  label: string;
  path: string;
  size_bytes: number;
  created_at: string;
};
