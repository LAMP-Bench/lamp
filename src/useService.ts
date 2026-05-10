import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ServiceName, ServiceStatus } from "./types";

export function useService(name: ServiceName) {
  const [status, setStatus] = useState<ServiceStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    try {
      setStatus(await invoke<ServiceStatus>("service_status", { name }));
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 1500);
    return () => clearInterval(id);
  }, [name]);

  async function toggle() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const cmd = status?.kind === "running" ? "service_stop" : "service_start";
      await invoke(cmd, { name });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return { status, busy, error, toggle, refresh };
}
