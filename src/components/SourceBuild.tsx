import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { FiTool, FiCopy, FiCheck, FiAlertTriangle, FiLoader } from "react-icons/fi";

/// Mirrors `build::DepReport`.
export type DepReport = {
  distro: string;
  distro_id: string;
  family: string;
  source_version: string | null;
  missing: string[];
  packages: string[];
  install_command: string | null;
  buildable: boolean;
};

/// Path to a system copy that would satisfy this component, or null. Only
/// Redis and MailHog can be, see `services::system_names`.
export function useSystemBinary(name: string) {
  const [path, setPath] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    invoke<string | null>("component_system_binary", { name })
      .then((p) => live && setPath(p))
      .catch(() => live && setPath(null));
    return () => {
      live = false;
    };
  }, [name]);
  return path;
}

export function useDepReport(name: string) {
  const [report, setReport] = useState<DepReport | null>(null);
  useEffect(() => {
    let live = true;
    invoke<DepReport>("source_build_report", { name })
      .then((r) => live && setReport(r))
      .catch(() => live && setReport(null));
    return () => {
      live = false;
    };
  }, [name]);
  return report;
}

type Phase =
  | { kind: "consent" }
  | { kind: "installing" }
  | { kind: "building" }
  | { kind: "done" }
  | { kind: "failed"; message: string };

/// Two-step flow: show exactly what will be installed and run, then compile
/// with a live log.
///
/// The consent step is not decoration. Installing build dependencies is the
/// one thing Lamp Bench does that reaches outside its own folder, so the
/// exact command is on screen before anything happens, and it can be copied
/// and run by hand instead.
export function SourceBuildModal({
  name,
  report,
  onClose,
  onBuilt,
}: {
  name: string;
  report: DepReport;
  onClose: () => void;
  onBuilt: () => void;
}) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>({ kind: "consent" });
  const [lines, setLines] = useState<string[]>([]);
  const [copied, setCopied] = useState(false);
  const logRef = useRef<HTMLPreElement>(null);

  const needsDeps = report.missing.length > 0;
  const canAutoInstall = needsDeps && report.install_command !== null;
  // On a platform whose package manager we don't know, there is no package
  // list either, fall back to naming what the probe actually couldn't find,
  // so the box is never empty.
  const manualText =
    report.install_command ??
    (report.packages.length > 0
      ? report.packages.join(" ")
      : report.missing.join(" "));

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<{ name: string; line: string }>("source-build-log", (e) => {
      if (e.payload.name !== name) return;
      // Capped: a PHP build emits tens of thousands of lines and the DOM
      // should not have to hold all of them.
      setLines((prev) => [...prev, e.payload.line].slice(-500));
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, [name]);

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [lines]);

  async function start() {
    try {
      if (canAutoInstall) {
        setPhase({ kind: "installing" });
        await invoke("source_build_install_deps", { name });
      }
      setPhase({ kind: "building" });
      await invoke("source_build", { name });
      setPhase({ kind: "done" });
      onBuilt();
    } catch (e) {
      setPhase({ kind: "failed", message: String(e) });
    }
  }

  const busy = phase.kind === "installing" || phase.kind === "building";

  return (
    <div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-6">
      <div className="bg-white rounded-xl shadow-xl w-full max-w-2xl max-h-full flex flex-col">
        <div className="px-5 py-4 border-b border-neutral-200 flex items-center gap-3">
          <FiTool className="text-neutral-500" />
          <div className="flex-1">
            <div className="font-medium text-neutral-900">
              {t("sourceBuild.title", { name })}
            </div>
            <div className="text-xs text-neutral-500">
              {t("sourceBuild.intro", {
                distro: report.distro,
                version: report.source_version ?? "",
              })}
            </div>
          </div>
        </div>

        <div className="px-5 py-4 overflow-y-auto space-y-4">
          {phase.kind === "consent" && needsDeps && (
            <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 space-y-2">
              <div className="text-sm font-medium text-amber-900 flex items-center gap-2">
                <FiAlertTriangle />
                {t("sourceBuild.toolsNeeded")}
              </div>
              <div className="text-xs text-amber-900">
                {t("sourceBuild.toolsMissing", { list: report.missing.join(", ") })}
              </div>
              <div className="text-xs text-amber-900">
                {canAutoInstall
                  ? t("sourceBuild.toolsCommand")
                  : t("sourceBuild.toolsManual")}
              </div>
              <div className="flex items-start gap-2">
                <code className="flex-1 block bg-white border border-amber-200 rounded p-2 text-[11px] font-mono text-neutral-800 break-all">
                  {manualText}
                </code>
                <button
                  onClick={() => {
                    navigator.clipboard.writeText(manualText);
                    setCopied(true);
                  }}
                  className="px-2 py-1 rounded border border-amber-300 text-amber-800 hover:bg-amber-100 text-[11px] flex items-center gap-1 shrink-0"
                >
                  {copied ? <FiCheck /> : <FiCopy />}
                  {t("sourceBuild.copy")}
                </button>
              </div>
              <div className="text-[11px] text-amber-800">
                {t("sourceBuild.toolsNote")}
              </div>
            </div>
          )}

          {lines.length > 0 && (
            <pre
              ref={logRef}
              className="bg-neutral-900 text-neutral-100 rounded-lg p-3 text-[11px] font-mono h-64 overflow-auto whitespace-pre-wrap"
            >
              {lines.join("\n")}
            </pre>
          )}

          {phase.kind === "failed" && (
            <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-xs text-red-800 whitespace-pre-wrap">
              <div className="font-medium mb-1">{t("sourceBuild.failed")}</div>
              {phase.message}
            </div>
          )}
          {phase.kind === "done" && (
            <div className="rounded-lg border border-emerald-200 bg-emerald-50 p-3 text-sm text-emerald-800 flex items-center gap-2">
              <FiCheck />
              {t("sourceBuild.ok")}
            </div>
          )}
        </div>

        <div className="px-5 py-3 border-t border-neutral-200 flex items-center justify-end gap-2">
          <button
            onClick={onClose}
            disabled={busy}
            className="px-3 py-2 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm disabled:opacity-50"
          >
            {t("sourceBuild.close")}
          </button>
          {(phase.kind === "consent" || phase.kind === "failed") && (
            <button
              onClick={start}
              className="px-4 py-2 rounded bg-sky-600 hover:bg-sky-700 text-white text-sm font-medium"
            >
              {canAutoInstall
                ? t("sourceBuild.start")
                : t("sourceBuild.startNoDeps")}
            </button>
          )}
          {busy && (
            <span className="px-3 py-2 text-sm text-neutral-600 flex items-center gap-2">
              <FiLoader className="animate-spin" />
              {t("sourceBuild.running")}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
