import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/// Error text the backend returns for a user-initiated abort. Matches
/// `downloads::CANCELLED` — callers use it to tell "you stopped this" apart
/// from "this failed", and stay quiet about the former.
export const CANCELLED = "Download cancelled.";

export function isCancelled(error: unknown): boolean {
  return String(error).includes(CANCELLED);
}

/// Ask an in-flight download to stop. A timeout only rescues a dead socket;
/// this covers the far more common case of a transfer that is simply huge,
/// or that the user changed their mind about — which previously meant
/// killing the app, with the first-launch wizard offering no way out at all.
export function cancelBinaryDownload(name: string): Promise<void> {
  return invoke("binary_download_cancel", { name }).then(() => undefined);
}

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
