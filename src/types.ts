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

export type SectionId = "hosts" | "tools" | "editor" | "logs";

export type LogName = "apache" | "mysql" | "nginx";

export type ServiceName = "apache" | "mysql" | "nginx" | "redis";
