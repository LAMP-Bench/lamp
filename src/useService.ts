import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ServiceError, ServiceName, ServiceStatus } from "./types";

/// How long to wait after a successful spawn before deciding the service is
/// actually up. httpd/mysqld/nginx all exit within a couple hundred ms when
/// their port is taken or the generated config is invalid, and by then
/// `service_start` has long since returned Ok.
const SETTLE_MS = 900;

export function useService(name: ServiceName) {
  const [status, setStatus] = useState<ServiceStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ServiceError | null>(null);

  async function refresh() {
    try {
      const next = await invoke<ServiceStatus>("service_status", { name });
      setStatus(next);
      return next;
    } catch (e) {
      setError({ kind: "backend", message: String(e) });
      return null;
    }
  }

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 1500);
    return () => clearInterval(id);
  }, [name]);

  /// Flip the service and report what went wrong, if anything.
  ///
  /// Returns `null` on success and a `ServiceError` otherwise. Callers MUST
  /// surface it: a toggle that springs back to "off" with no explanation is
  /// the failure mode that made the v0.1.0 installer look bricked, and it's
  /// the entire experience on a platform with no service binaries pinned.
  async function toggle(): Promise<ServiceError | null> {
    if (busy) return null;
    const starting = status?.kind !== "running";
    setBusy(true);
    setError(null);
    try {
      await invoke(starting ? "service_start" : "service_stop", { name });

      if (!starting) {
        await refresh();
        return null;
      }

      // A clean spawn is not a clean start. Give the process a moment to
      // fall over on its own before calling it running.
      await new Promise((r) => setTimeout(r, SETTLE_MS));
      const settled = await refresh();
      if (settled && settled.kind !== "running") {
        const failure: ServiceError =
          settled.kind === "error"
            ? { kind: "backend", message: settled.message }
            : { kind: "exited" };
        setError(failure);
        return failure;
      }
      return null;
    } catch (e) {
      const failure: ServiceError = { kind: "backend", message: String(e) };
      setError(failure);
      return failure;
    } finally {
      setBusy(false);
    }
  }

  return { status, busy, error, toggle, refresh };
}
