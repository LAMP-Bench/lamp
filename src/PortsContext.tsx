import {
  createContext,
  ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ServiceName } from "./types";

export type ServicePorts = { port: number; port2: number };

const SERVICES: ServiceName[] = ["apache", "nginx", "mysql", "redis", "mailhog"];

/// Mirrors `default_ports()` in lib.rs. Rendered until the real config
/// arrives, so a link is never briefly built from a zero.
const DEFAULTS: Record<ServiceName, ServicePorts> = {
  apache: { port: 8080, port2: 8443 },
  nginx: { port: 8081, port2: 8444 },
  mysql: { port: 3306, port2: 0 },
  redis: { port: 6379, port2: 0 },
  // port is the web UI, port2 the SMTP listener.
  mailhog: { port: 8025, port2: 1025 },
};

type Ctx = {
  ports: Record<ServiceName, ServicePorts>;
  /// Re-read from the backend. Call after the sidebar saves a port so the
  /// links elsewhere in the UI stop pointing at the old one.
  refresh: () => Promise<void>;
  /// `http://<host>:<apache http port><path>`
  siteUrl: (host?: string, path?: string) => string;
  /// `https://<host>:<apache https port><path>`
  secureSiteUrl: (host?: string, path?: string) => string;
  /// MailHog's web inbox.
  mailhogUrl: () => string;
};

const PortsCtx = createContext<Ctx | null>(null);

/// Single source of truth for "which port is X on right now".
///
/// Ports became user-configurable without the UI following: nineteen places
/// still wrote `:8080`, `:3306` and friends as literals, so moving Apache
/// off its default left the WebStart button, the phpMyAdmin shortcut, every
/// per-host link and the phone QR code all pointing somewhere dead.
export function PortsProvider({ children }: { children: ReactNode }) {
  const [ports, setPorts] = useState<Record<ServiceName, ServicePorts>>(DEFAULTS);

  const refresh = useCallback(async () => {
    const results = await Promise.all(
      SERVICES.map((name) =>
        invoke<ServicePorts>("service_ports_get", { name })
          .then((p) => [name, { port: p.port, port2: p.port2 }] as const)
          // A single failed lookup shouldn't blank the others out.
          .catch(() => [name, DEFAULTS[name]] as const),
      ),
    );
    setPorts(Object.fromEntries(results) as Record<ServiceName, ServicePorts>);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const value = useMemo<Ctx>(() => {
    const url = (scheme: string, port: number, host: string, path: string) =>
      `${scheme}://${host}:${port}${path}`;
    return {
      ports,
      refresh,
      siteUrl: (host = "localhost", path = "/") =>
        url("http", ports.apache.port, host, path),
      secureSiteUrl: (host = "localhost", path = "/") =>
        url("https", ports.apache.port2, host, path),
      mailhogUrl: () => url("http", ports.mailhog.port, "localhost", "/"),
    };
  }, [ports, refresh]);

  return <PortsCtx.Provider value={value}>{children}</PortsCtx.Provider>;
}

export function usePorts(): Ctx {
  const ctx = useContext(PortsCtx);
  if (!ctx) throw new Error("usePorts must be used inside a PortsProvider");
  return ctx;
}
