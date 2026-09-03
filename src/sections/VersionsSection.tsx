import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import {
  FiPackage,
  FiDownload,
  FiTrash2,
  FiCheck,
  FiChevronDown,
  FiChevronUp,
  FiShield,
  FiX,
  FiTool,
} from "react-icons/fi";
import { SiPhp } from "react-icons/si";
import { useConfirm, useToast } from "../components/Toast";
import {
  cancelBinaryDownload,
  isCancelled,
  useDownloadProgress,
} from "../useDownloadProgress";
import type { PhpExtension } from "../types";
import {
  SourceBuildModal,
  useDepReport,
  useSystemBinary,
} from "../components/SourceBuild";

/// Categorise a manifest entry name into a UI group. Anything PHP-ish goes
/// to its own group; the core daemons are "runtimes"; everything else is a
/// tool/extra.
function groupOf(name: string): "php" | "runtime" | "tool" {
  if (name.startsWith("php-") || name.startsWith("xdebug-")) return "php";
  if (
    ["apache", "mod_fcgid", "nginx", "redis", "mailhog"].includes(name) ||
    name.startsWith("mysql-")
  )
    return "runtime";
  return "tool";
}

export function VersionsSection() {
  const { t } = useTranslation();
  const confirm = useConfirm();
  const toast = useToast();
  const [names, setNames] = useState<string[]>([]);
  const [installed, setInstalled] = useState<Record<string, boolean>>({});
  // Components with no binaries.json entry for the current OS. Listing them
  // with a working Install button would be a lie.
  const [available, setAvailable] = useState<Record<string, boolean>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [showMore, setShowMore] = useState(false);
  const pct = useDownloadProgress(downloading);

  async function refresh() {
    const list = await invoke<string[]>("binary_list").catch(() => []);
    setNames(list);
    const flags = await Promise.all(
      list.map((n) =>
        invoke<boolean>("binary_installed", { name: n }).catch(() => false)
      )
    );
    const map: Record<string, boolean> = {};
    list.forEach((n, i) => (map[n] = flags[i]));
    setInstalled(map);

    const avail = await Promise.all(
      list.map((n) =>
        invoke<boolean>("binary_available", { name: n }).catch(() => true)
      )
    );
    const amap: Record<string, boolean> = {};
    list.forEach((n, i) => (amap[n] = avail[i]));
    setAvailable(amap);
  }

  useEffect(() => {
    refresh();
  }, []);

  async function install(name: string) {
    setBusy(name);
    setDownloading(name);
    try {
      await invoke("binary_download", { name });
      await refresh();
      toast("success", `${name} ✓`);
    } catch (e) {
      if (!isCancelled(e)) toast("error", String(e));
    } finally {
      setBusy(null);
      setDownloading(null);
    }
  }

  async function remove(name: string) {
    const ok = await confirm({
      message: t("versions.confirmRemove", { name }),
      confirmLabel: t("versions.remove"),
      tone: "danger",
    });
    if (!ok) return;
    setBusy(name);
    try {
      await invoke("binary_remove", { name });
      await refresh();
    } catch (e) {
      toast("error", String(e));
    } finally {
      setBusy(null);
    }
  }

  const runtimes = names.filter((n) => groupOf(n) === "runtime");
  const phpNames = names.filter((n) => groupOf(n) === "php");
  const tools = names.filter((n) => groupOf(n) === "tool");

  function Group({ title, items }: { title: string; items: string[] }) {
    const present = items.filter((n) => installed[n]);
    const missing = items.filter((n) => !installed[n]);
    const visible = showMore ? items : present;
    if (visible.length === 0 && !showMore) return null;
    return (
      <section>
        <h3 className="text-[11px] uppercase tracking-wider text-neutral-500 font-medium mb-2">
          {title}
        </h3>
        <div className="rounded-lg border border-neutral-200 bg-white divide-y divide-neutral-100">
          {visible.map((name) =>
            available[name] === false ? (
              <UnavailableRow key={name} name={name} />
            ) : (
              <Row key={name} name={name} />
            )
          )}
          {!showMore && missing.length > 0 && (
            <div className="px-4 py-1.5 text-[11px] text-neutral-400">
              {missing.length} more available — “{t("versions.downloadMore")}”
            </div>
          )}
        </div>
      </section>
    );
  }

  function UnavailableRow({ name }: { name: string }) {
    const report = useDepReport(name);
    const systemPath = useSystemBinary(name);
    const [open, setOpen] = useState(false);
    // A component the supervisor can already start from a system copy is not
    // missing anything. Offering a five-minute compile for it would be
    // actively misleading.
    const buildable = report?.buildable === true && systemPath === null;
    return (
      <div
        className={`px-4 py-2 flex items-center gap-3 text-sm ${
          buildable || systemPath ? "" : "opacity-60"
        }`}
      >
        <FiPackage className="text-neutral-400 shrink-0" />
        <code className="flex-1 text-neutral-500 font-mono text-xs truncate">
          {name}
        </code>
        {buildable ? (
          <button
            onClick={() => setOpen(true)}
            className="px-2 py-1 rounded border border-violet-300 text-violet-700 hover:bg-violet-50 text-xs flex items-center gap-1"
            title={t("sourceBuild.rowHint")}
          >
            <FiTool />
            {t("sourceBuild.compile")}
          </button>
        ) : systemPath ? (
          <span
            className="text-[11px] text-emerald-700 border border-emerald-200 bg-emerald-50 rounded px-2 py-1"
            title={t("versions.usingSystemHint", { path: systemPath })}
          >
            {t("versions.usingSystem")}
          </span>
        ) : (
          <span
            className="text-[11px] text-neutral-400 border border-neutral-200 rounded px-2 py-1"
            title={t("versions.unavailableHint")}
          >
            {t("versions.unavailable")}
          </span>
        )}
        {open && report && (
          <SourceBuildModal
            name={name}
            report={report}
            onClose={() => setOpen(false)}
            onBuilt={() => {
              refresh();
            }}
          />
        )}
      </div>
    );
  }

  function Row({ name }: { name: string }) {
    const isInstalled = installed[name];
    const isBusy = busy === name;
    return (
      <div className="px-4 py-2 flex items-center gap-3 text-sm">
        <FiPackage className="text-neutral-500 shrink-0" />
        <code className="flex-1 text-neutral-800 font-mono text-xs truncate">
          {name}
        </code>
        {isInstalled ? (
          <>
            <span className="text-emerald-600 text-xs flex items-center gap-1">
              <FiCheck />
              {t("versions.installed")}
            </span>
            <button
              onClick={() => remove(name)}
              disabled={isBusy}
              className="px-2 py-1 rounded border border-red-300 text-red-600 hover:bg-red-50 text-xs flex items-center gap-1 disabled:opacity-50"
            >
              <FiTrash2 />
              {isBusy ? t("versions.removing") : t("versions.remove")}
            </button>
          </>
        ) : (
          isBusy ? (
            <button
              onClick={() => cancelBinaryDownload(name)}
              className="px-2 py-1 rounded border border-neutral-300 text-neutral-600 hover:bg-neutral-100 text-xs flex items-center gap-1"
              title={t("versions.cancelDownload")}
            >
              {pct !== null ? `${pct}%` : t("versions.installing")}
              <FiX />
            </button>
          ) : (
            <button
              onClick={() => install(name)}
              className="px-2 py-1 rounded border border-sky-300 text-sky-600 hover:bg-sky-50 text-xs flex items-center gap-1"
            >
              <FiDownload />
              {t("versions.install")}
            </button>
          )
        )}
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="p-6 max-w-3xl space-y-6">
        <p className="text-xs text-neutral-500">{t("versions.intro")}</p>

        <div className="flex items-center justify-end">
          <button
            onClick={() => setShowMore((s) => !s)}
            className="px-3 py-1.5 rounded border border-neutral-300 hover:bg-neutral-50 text-sm flex items-center gap-1.5"
          >
            {showMore ? <FiChevronUp /> : <FiChevronDown />}
            {showMore ? t("versions.hideMore") : t("versions.downloadMore")}
          </button>
        </div>
        {showMore && (
          <p className="text-[11px] text-neutral-400 -mt-3">{t("versions.moreHint")}</p>
        )}

        <Group title={t("versions.groupRuntimes")} items={runtimes} />
        <Group title={t("versions.groupPhp")} items={phpNames} />
        <Group title={t("versions.groupTools")} items={tools} />

        <PhpToolsPanel installedPhp={phpNames.filter(
          (n) => n.startsWith("php-") && installed[n]
        ).map((n) => n.replace("php-", ""))} />
      </div>
    </div>
  );
}

/// PHP extensions toggle + ionCube installer, scoped to a picked PHP version.
function PhpToolsPanel({ installedPhp }: { installedPhp: string[] }) {
  const { t } = useTranslation();
  const toast = useToast();
  const [version, setVersion] = useState<string>("");
  const [exts, setExts] = useState<PhpExtension[]>([]);
  const [ioncubeBusy, setIoncubeBusy] = useState(false);

  useEffect(() => {
    if (installedPhp.length && !version) setVersion(installedPhp[0]);
  }, [installedPhp, version]);

  async function loadExts(v: string) {
    if (!v) return;
    const list = await invoke<PhpExtension[]>("php_extensions", { version: v }).catch(
      () => []
    );
    setExts(list);
  }

  useEffect(() => {
    loadExts(version);
  }, [version]);

  async function toggle(name: string, enable: boolean) {
    try {
      await invoke("php_extension_toggle", { version, name, enable });
      await loadExts(version);
    } catch (e) {
      toast("error", String(e));
    }
  }

  async function installIoncube() {
    setIoncubeBusy(true);
    try {
      await invoke("ioncube_install", { version });
      toast("success", t("versions.ioncubeDone", { version }));
    } catch (e) {
      toast("error", String(e));
    } finally {
      setIoncubeBusy(false);
    }
  }

  return (
    <section>
      <h3 className="text-[11px] uppercase tracking-wider text-neutral-500 font-medium mb-2">
        {t("versions.phpSection")}
      </h3>
      {installedPhp.length === 0 ? (
        <div className="rounded-lg border border-dashed border-neutral-300 bg-neutral-50 p-4 text-center text-sm text-neutral-500">
          {t("versions.noPhp")}
        </div>
      ) : (
        <div className="rounded-lg border border-neutral-200 bg-white divide-y divide-neutral-100">
          <div className="px-4 py-3 flex items-center gap-3">
            <SiPhp className="text-indigo-500 text-lg" />
            <span className="text-sm text-neutral-700">{t("versions.phpPick")}</span>
            <select
              value={version}
              onChange={(e) => setVersion(e.target.value)}
              className="ml-auto px-3 py-1.5 rounded border border-neutral-300 text-sm bg-white font-mono focus:outline-none focus:border-sky-500"
            >
              {installedPhp.map((v) => (
                <option key={v} value={v}>
                  {v}
                </option>
              ))}
            </select>
          </div>

          <div className="px-4 py-3">
            <div className="text-xs text-neutral-500 mb-2">
              {t("versions.extensions")} — {t("versions.extensionsHint")}
            </div>
            <div className="grid grid-cols-2 gap-x-4 gap-y-1.5">
              {exts.map((e) => (
                <label
                  key={e.name}
                  className="flex items-center gap-2 text-sm text-neutral-700 cursor-pointer"
                >
                  <input
                    type="checkbox"
                    checked={e.enabled}
                    onChange={(ev) => toggle(e.name, ev.target.checked)}
                    className="size-4"
                  />
                  <code className="font-mono text-xs">{e.name}</code>
                </label>
              ))}
            </div>
          </div>

          <div className="px-4 py-3 flex items-start gap-3">
            <FiShield className="text-amber-500 text-lg mt-0.5 shrink-0" />
            <div className="flex-1">
              <div className="text-sm font-medium text-neutral-800">
                {t("versions.ioncube")}
              </div>
              <div className="text-xs text-neutral-500 mt-0.5">
                {t("versions.ioncubeHint")}
              </div>
            </div>
            <button
              onClick={installIoncube}
              disabled={ioncubeBusy}
              className="px-3 py-1.5 rounded bg-sky-600 hover:bg-sky-700 text-white text-sm disabled:opacity-50 shrink-0"
            >
              {ioncubeBusy
                ? t("versions.ioncubeInstalling")
                : t("versions.ioncubeInstall")}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
