import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

type ProgressEvent = {
  name: string;
  downloaded: number;
  total: number | null;
};

/// Percentage for the binary currently being downloaded, or null when we
/// aren't downloading or the server didn't send a Content-Length.
///
/// The backend has emitted `binary-download-progress` since the setup wizard
/// gained a progress bar, but the sidebar and the Versions panel never
/// subscribed — both showed a bare "…" while pulling several hundred
/// megabytes, which reads as a hang.
export function useDownloadProgress(activeName: string | null): number | null {
  const [pct, setPct] = useState<number | null>(null);

  useEffect(() => {
    if (!activeName) {
      setPct(null);
      return;
    }
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    listen<ProgressEvent>("binary-download-progress", (event) => {
      const { name, downloaded, total } = event.payload;
      if (name !== activeName) return;
      setPct(total && total > 0 ? Math.round((downloaded / total) * 100) : null);
    }).then((un) => {
      // The effect may have been torn down while `listen` was in flight.
      if (cancelled) un();
      else unlisten = un;
    });

    return () => {
      cancelled = true;
      unlisten?.();
      setPct(null);
    };
  }, [activeName]);

  return pct;
}
