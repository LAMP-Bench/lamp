import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { FiDownload, FiX } from "react-icons/fi";

type State =
  | { kind: "idle" }
  | { kind: "available"; update: Update }
  | { kind: "installing"; progress: number; total: number | null }
  | { kind: "dismissed" }
  | { kind: "error"; message: string };

/// Auto-checks GitHub Releases on mount and shows a top banner when a newer
/// signed version is published. "Install & Restart" downloads + installs the
/// signed delta and relaunches the app. Failures are kept quiet (logged to
/// devtools only) — a missing/unreachable update server shouldn't yell at
/// the user every launch.
export function UpdateBanner() {
  const [state, setState] = useState<State>({ kind: "idle" });

  useEffect(() => {
    let cancelled = false;
    check()
      .then((update) => {
        if (cancelled) return;
        if (update) setState({ kind: "available", update });
      })
      .catch((e) => {
        console.warn("updater check failed:", e);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function install() {
    if (state.kind !== "available") return;
    const update = state.update;
    setState({ kind: "installing", progress: 0, total: null });
    try {
      let downloaded = 0;
      let contentLength: number | null = null;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          contentLength = event.data.contentLength ?? null;
          setState({ kind: "installing", progress: 0, total: contentLength });
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setState({
            kind: "installing",
            progress: downloaded,
            total: contentLength,
          });
        }
      });
      await relaunch();
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }

  if (state.kind === "idle" || state.kind === "dismissed") return null;

  if (state.kind === "error") {
    return (
      <Bar tone="error">
        <span>Update failed: {state.message}</span>
        <button
          onClick={() => setState({ kind: "dismissed" })}
          className="ml-auto p-1 rounded hover:bg-black/10"
          title="Dismiss"
        >
          <FiX />
        </button>
      </Bar>
    );
  }

  if (state.kind === "installing") {
    const pct =
      state.total && state.total > 0
        ? Math.round((state.progress / state.total) * 100)
        : null;
    return (
      <Bar tone="info">
        <span>
          Downloading update…{pct !== null ? ` ${pct}%` : ""}
        </span>
      </Bar>
    );
  }

  // state.kind === "available"
  const v = state.update.version;
  return (
    <Bar tone="info">
      <FiDownload />
      <span>
        Lamp Bench <strong>{v}</strong> is available.
      </span>
      <button
        onClick={install}
        className="ml-2 px-2 py-0.5 rounded bg-sky-600 text-white text-[12px] hover:bg-sky-700"
      >
        Install &amp; Restart
      </button>
      <button
        onClick={() => setState({ kind: "dismissed" })}
        className="ml-auto p-1 rounded hover:bg-black/10"
        title="Dismiss"
      >
        <FiX />
      </button>
    </Bar>
  );
}

function Bar({
  tone,
  children,
}: {
  tone: "info" | "error";
  children: React.ReactNode;
}) {
  const colors =
    tone === "error"
      ? "bg-red-50 text-red-800 border-red-200"
      : "bg-sky-50 text-sky-800 border-sky-200";
  return (
    <div
      className={`flex items-center gap-2 px-4 py-1.5 text-[13px] border-b ${colors}`}
    >
      {children}
    </div>
  );
}
