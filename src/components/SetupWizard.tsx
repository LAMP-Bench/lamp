import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import {
  FiCheck,
  FiDownload,
  FiAlertTriangle,
  FiLoader,
} from "react-icons/fi";
import { SiApache, SiMysql, SiPhp, SiPhpmyadmin, SiComposer } from "react-icons/si";
import { LuLamp } from "react-icons/lu";

/// Manifest entries the app needs to function. `binary_download` is the same
/// Tauri command the on-demand sidebar uses — calling it here just batches
/// the bundled-essentials downloads into a one-time setup flow that runs
/// the first time someone opens the installer's app.
const ESSENTIALS: { name: string; label: string; icon: React.ReactNode }[] = [
  { name: "apache", label: "Apache 2.4", icon: <SiApache /> },
  { name: "mod_fcgid", label: "mod_fcgid", icon: <SiApache /> },
  { name: "mysql-8.0", label: "MySQL 8.0", icon: <SiMysql /> },
  { name: "php-8.4", label: "PHP 8.4", icon: <SiPhp /> },
  { name: "xdebug-8.4", label: "Xdebug 8.4", icon: <SiPhp /> },
  { name: "phpmyadmin", label: "phpMyAdmin", icon: <SiPhpmyadmin /> },
  { name: "composer", label: "Composer", icon: <SiComposer /> },
];

export async function setupNeeded(): Promise<{
  needed: boolean;
  platformSupported: boolean;
}> {
  const platform = await invoke<string>("current_platform").catch(() => "unsupported");
  const platformSupported = platform === "windows-x64";
  if (!platformSupported) return { needed: true, platformSupported: false };
  const checks = await Promise.all(
    ESSENTIALS.map((e) =>
      invoke<boolean>("binary_installed", { name: e.name }).catch(() => false),
    ),
  );
  return { needed: !checks.every(Boolean), platformSupported: true };
}

type ItemStatus =
  | { kind: "pending" }
  | { kind: "downloading" }
  | { kind: "done" }
  | { kind: "error"; message: string };

export function SetupWizard({
  onComplete,
  platformSupported,
}: {
  onComplete: () => void;
  platformSupported: boolean;
}) {
  const { t } = useTranslation();
  const [statuses, setStatuses] = useState<ItemStatus[]>(
    ESSENTIALS.map(() => ({ kind: "pending" } as ItemStatus)),
  );
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(false);

  async function run() {
    if (running) return;
    setRunning(true);
    setDone(false);
    const next: ItemStatus[] = [...statuses];

    for (let i = 0; i < ESSENTIALS.length; i++) {
      // Skip items that already succeeded on a previous attempt.
      if (next[i].kind === "done") continue;
      const installed = await invoke<boolean>("binary_installed", {
        name: ESSENTIALS[i].name,
      }).catch(() => false);
      if (installed) {
        next[i] = { kind: "done" };
        setStatuses([...next]);
        continue;
      }
      next[i] = { kind: "downloading" };
      setStatuses([...next]);
      try {
        await invoke("binary_download", { name: ESSENTIALS[i].name });
        next[i] = { kind: "done" };
      } catch (e) {
        next[i] = { kind: "error", message: String(e) };
      }
      setStatuses([...next]);
    }

    setRunning(false);
    if (next.every((s) => s.kind === "done")) {
      setDone(true);
    }
  }

  // Auto-run on mount when supported.
  useEffect(() => {
    if (platformSupported) {
      run();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const anyErrors = statuses.some((s) => s.kind === "error");

  return (
    <div className="h-screen w-screen bg-gradient-to-br from-amber-50 via-orange-50 to-rose-50 flex items-center justify-center p-6">
      <div className="bg-white rounded-2xl shadow-xl w-full max-w-xl p-8">
        <div className="flex items-center gap-3 mb-6">
          <div className="size-12 rounded-xl bg-gradient-to-br from-amber-400 to-orange-500 flex items-center justify-center text-white shadow">
            <LuLamp className="text-2xl" />
          </div>
          <div>
            <div className="text-lg font-semibold text-neutral-900">
              {platformSupported
                ? t("setup.title")
                : t("setup.unsupportedTitle")}
            </div>
            <div className="text-xs text-neutral-500">
              {platformSupported
                ? t("setup.subtitle")
                : t("setup.unsupportedHint")}
            </div>
          </div>
        </div>

        {platformSupported ? (
          <>
            <ul className="space-y-1 mb-5">
              {ESSENTIALS.map((e, i) => (
                <li
                  key={e.name}
                  className="flex items-center gap-3 px-3 py-2 rounded text-sm"
                >
                  <span className="text-neutral-500 text-lg shrink-0">
                    {e.icon}
                  </span>
                  <span className="flex-1 text-neutral-800">{e.label}</span>
                  <StatusBadge status={statuses[i]} />
                </li>
              ))}
            </ul>

            {anyErrors && !running && (
              <div className="mb-4 text-xs text-amber-800 bg-amber-50 border border-amber-200 rounded p-2">
                <strong>{t("setup.errorTitle")}.</strong>{" "}
                {t("setup.errorHint")}
              </div>
            )}

            <div className="flex items-center gap-2 justify-end">
              {done ? (
                <button
                  onClick={onComplete}
                  className="px-4 py-2 rounded bg-sky-600 hover:bg-sky-700 text-white text-sm font-medium"
                >
                  {t("setup.getStarted")}
                </button>
              ) : anyErrors && !running ? (
                <>
                  <button
                    onClick={onComplete}
                    className="px-3 py-2 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm"
                  >
                    {t("setup.skip")}
                  </button>
                  <button
                    onClick={run}
                    className="px-4 py-2 rounded bg-sky-600 hover:bg-sky-700 text-white text-sm font-medium"
                  >
                    {t("setup.retry")}
                  </button>
                </>
              ) : null}
            </div>
          </>
        ) : (
          <div className="flex justify-end">
            <button
              onClick={onComplete}
              className="px-4 py-2 rounded bg-sky-600 hover:bg-sky-700 text-white text-sm font-medium"
            >
              {t("setup.continueAnyway")}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function StatusBadge({ status }: { status: ItemStatus }) {
  switch (status.kind) {
    case "pending":
      return (
        <span className="text-[11px] text-neutral-400 font-mono">…</span>
      );
    case "downloading":
      return (
        <span className="text-sky-600 inline-flex items-center gap-1 text-xs">
          <FiLoader className="animate-spin" />
          <FiDownload />
        </span>
      );
    case "done":
      return <FiCheck className="text-emerald-600" />;
    case "error":
      return (
        <span
          className="text-red-600 inline-flex items-center gap-1 text-xs"
          title={status.message}
        >
          <FiAlertTriangle />
        </span>
      );
  }
}
