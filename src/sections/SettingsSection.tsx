import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  FiGlobe,
  FiRefreshCw,
  FiDownload,
  FiInfo,
  FiExternalLink,
  FiCheck,
  FiPackage,
  FiTrash2,
} from "react-icons/fi";
import { SiPhp, SiMysql } from "react-icons/si";
import { LANGUAGES } from "../i18n";
import type { PhpCatalogEntry } from "../types";

type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "available"; update: Update }
  | { kind: "uptodate" }
  | { kind: "installing"; progress: number; total: number | null }
  | { kind: "error"; message: string };

export function SettingsSection() {
  const { t } = useTranslation();
  return (
    <div className="h-full overflow-y-auto">
      <div className="p-6 max-w-3xl space-y-6">
        <Card title={t("settings.general.title")}>
          <LanguageRow />
        </Card>

        <Card title={t("settings.services.title")}>
          <ServicesRows />
        </Card>

        <Card title={t("settings.versions.title")}>
          <VersionsRows />
        </Card>

        <Card title={t("settings.updates.title")}>
          <UpdatesRow />
        </Card>

        <Card title={t("settings.about.title")}>
          <AboutRow />
        </Card>
      </div>
    </div>
  );
}

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h2 className="text-xs uppercase tracking-wider text-neutral-500 font-medium mb-2">
        {title}
      </h2>
      <div className="rounded-lg border border-neutral-200 bg-white divide-y divide-neutral-100">
        {children}
      </div>
    </section>
  );
}

function Row({
  icon,
  label,
  hint,
  children,
}: {
  icon: React.ReactNode;
  label: string;
  hint?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="px-4 py-3 flex items-start gap-3">
      <span className="text-neutral-500 text-lg mt-0.5 shrink-0">{icon}</span>
      <div className="flex-1 min-w-0">
        <div className="text-sm text-neutral-800 font-medium">{label}</div>
        {hint && (
          <div className="text-xs text-neutral-500 mt-0.5">{hint}</div>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function ServicesRows() {
  const { t } = useTranslation();
  const [catalog, setCatalog] = useState<PhpCatalogEntry[]>([]);
  const [phpDefault, setPhpDefault] = useState<string>("");
  const [mysqlList, setMysqlList] = useState<string[]>([]);
  const [mysqlActive, setMysqlActive] = useState<string>("");
  const [phpBusy, setPhpBusy] = useState<string | null>(null);
  const [mysqlBusy, setMysqlBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    try {
      const [cat, versions, active] = await Promise.all([
        invoke<PhpCatalogEntry[]>("php_catalog"),
        invoke<string[]>("mysql_versions"),
        invoke<string>("mysql_active_version"),
      ]);
      setCatalog(cat);
      setMysqlList(versions);
      setMysqlActive(active);
      // Default PHP isn't a stored value yet — pick the highest installed one
      // as a best-effort display value. Adding a persistent setting is a
      // follow-up; the picker still acts as a one-shot installer.
      const installed = cat.filter((c) => c.installed);
      setPhpDefault(installed[installed.length - 1]?.version ?? "");
    } catch (e) {
      setError(String(e));
    }
  }

  async function pickPhp(version: string) {
    setError(null);
    const entry = catalog.find((c) => c.version === version);
    if (!entry) return;
    if (!entry.installed) {
      setPhpBusy(version);
      try {
        await invoke("php_install", { version });
      } catch (e) {
        setError(String(e));
        setPhpBusy(null);
        return;
      }
      setPhpBusy(null);
    }
    setPhpDefault(version);
    await refresh();
  }

  async function pickMysql(version: string) {
    if (version === mysqlActive) return;
    setError(null);
    setMysqlBusy(true);
    try {
      await invoke("mysql_set_version", { version });
      setMysqlActive(version);
    } catch (e) {
      setError(String(e));
    } finally {
      setMysqlBusy(false);
    }
  }

  return (
    <>
      <Row
        icon={<SiPhp />}
        label={t("settings.services.phpVersion")}
        hint={
          phpBusy
            ? t("settings.services.phpInstalling", { version: phpBusy })
            : t("settings.services.phpHint")
        }
      >
        <select
          value={phpDefault}
          onChange={(e) => pickPhp(e.target.value)}
          disabled={phpBusy !== null}
          className="px-3 py-1.5 rounded border border-neutral-300 text-sm bg-white focus:outline-none focus:border-sky-500 disabled:opacity-50"
        >
          {catalog.map((c) => (
            <option key={c.version} value={c.version}>
              {c.version}
              {c.installed ? "" : t("settings.services.download")}
            </option>
          ))}
        </select>
      </Row>
      <Row
        icon={<SiMysql />}
        label={t("settings.services.mysqlVersion")}
        hint={
          mysqlBusy
            ? t("settings.services.mysqlSwitching")
            : t("settings.services.mysqlHint")
        }
      >
        <select
          value={mysqlActive}
          onChange={(e) => pickMysql(e.target.value)}
          disabled={mysqlBusy || mysqlList.length === 0}
          className="px-3 py-1.5 rounded border border-neutral-300 text-sm bg-white focus:outline-none focus:border-sky-500 disabled:opacity-50"
        >
          {mysqlList.map((v) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
        </select>
      </Row>
      {error && (
        <div className="px-4 py-2 text-xs text-red-600 font-mono break-words bg-red-50">
          {error}
        </div>
      )}
    </>
  );
}

function VersionsRows() {
  const { t } = useTranslation();
  const [names, setNames] = useState<string[]>([]);
  const [installed, setInstalled] = useState<Record<string, boolean>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    try {
      const list = await invoke<string[]>("binary_list");
      setNames(list);
      const flags = await Promise.all(
        list.map((n) =>
          invoke<boolean>("binary_installed", { name: n }).catch(() => false),
        ),
      );
      const map: Record<string, boolean> = {};
      list.forEach((n, i) => (map[n] = flags[i]));
      setInstalled(map);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function install(name: string) {
    setError(null);
    setBusy(name);
    try {
      await invoke("binary_download", { name });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function remove(name: string) {
    if (!confirm(t("settings.versions.confirmRemove", { name }))) return;
    setError(null);
    setBusy(name);
    try {
      await invoke("binary_remove", { name });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <>
      <div className="px-4 py-2 text-[11px] text-neutral-500">
        {t("settings.versions.hint")}
      </div>
      {names.map((name) => {
        const isInstalled = installed[name];
        const isBusy = busy === name;
        return (
          <div key={name} className="px-4 py-2 flex items-center gap-3 text-sm">
            <FiPackage className="text-neutral-500 shrink-0" />
            <code className="flex-1 text-neutral-800 font-mono text-xs truncate">
              {name}
            </code>
            {isInstalled ? (
              <>
                <span className="text-emerald-600 text-xs flex items-center gap-1">
                  <FiCheck />
                  {t("settings.versions.installed")}
                </span>
                <button
                  onClick={() => remove(name)}
                  disabled={isBusy}
                  className="px-2 py-1 rounded border border-red-300 text-red-600 hover:bg-red-50 text-xs flex items-center gap-1 disabled:opacity-50"
                >
                  <FiTrash2 />
                  {isBusy ? t("settings.versions.removing") : t("settings.versions.remove")}
                </button>
              </>
            ) : (
              <button
                onClick={() => install(name)}
                disabled={isBusy}
                className="px-2 py-1 rounded border border-sky-300 text-sky-600 hover:bg-sky-50 text-xs flex items-center gap-1 disabled:opacity-50"
              >
                <FiDownload />
                {isBusy ? t("settings.versions.installing") : t("settings.versions.install")}
              </button>
            )}
          </div>
        );
      })}
      {error && (
        <div className="px-4 py-2 text-xs text-red-600 font-mono break-words bg-red-50">
          {error}
        </div>
      )}
    </>
  );
}

function LanguageRow() {
  const { t, i18n } = useTranslation();
  return (
    <Row
      icon={<FiGlobe />}
      label={t("settings.general.language")}
      hint={t("settings.general.languageHint")}
    >
      <select
        value={i18n.language.slice(0, 2)}
        onChange={(e) => i18n.changeLanguage(e.target.value)}
        className="px-3 py-1.5 rounded border border-neutral-300 text-sm bg-white focus:outline-none focus:border-sky-500"
      >
        {LANGUAGES.map((l) => (
          <option key={l.code} value={l.code}>
            {l.label}
          </option>
        ))}
      </select>
    </Row>
  );
}

function UpdatesRow() {
  const { t } = useTranslation();
  const [state, setState] = useState<UpdateState>({ kind: "idle" });
  const [version, setVersion] = useState<string>("");

  useEffect(() => {
    invoke<string>("app_version").then(setVersion).catch(() => {});
  }, []);

  async function runCheck() {
    setState({ kind: "checking" });
    try {
      const upd = await check();
      if (upd) setState({ kind: "available", update: upd });
      else setState({ kind: "uptodate" });
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }

  async function install() {
    if (state.kind !== "available") return;
    const update = state.update;
    setState({ kind: "installing", progress: 0, total: null });
    try {
      let downloaded = 0;
      let total: number | null = null;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? null;
          setState({ kind: "installing", progress: 0, total });
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setState({ kind: "installing", progress: downloaded, total });
        }
      });
      await relaunch();
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }

  let trailing: React.ReactNode = (
    <button
      onClick={runCheck}
      disabled={state.kind === "checking" || state.kind === "installing"}
      className="px-3 py-1.5 rounded border border-neutral-300 hover:bg-neutral-50 text-sm flex items-center gap-1.5 disabled:opacity-50"
    >
      <FiRefreshCw
        className={state.kind === "checking" ? "animate-spin" : ""}
      />
      {state.kind === "checking"
        ? t("settings.updates.checking")
        : t("settings.updates.checkNow")}
    </button>
  );

  let hint: React.ReactNode = t("settings.updates.autoHint");
  if (state.kind === "uptodate") {
    hint = (
      <span className="text-emerald-700 inline-flex items-center gap-1">
        <FiCheck />
        {t("settings.updates.upToDate", { version })}
      </span>
    );
  } else if (state.kind === "available") {
    hint = t("settings.updates.available", { version: state.update.version });
    trailing = (
      <button
        onClick={install}
        className="px-3 py-1.5 rounded bg-sky-600 hover:bg-sky-700 text-white text-sm flex items-center gap-1.5"
      >
        <FiDownload />
        {t("settings.updates.installNow")}
      </button>
    );
  } else if (state.kind === "installing") {
    const pct =
      state.total && state.total > 0
        ? Math.round((state.progress / state.total) * 100)
        : null;
    hint = `${t("settings.updates.checking")}${pct !== null ? ` ${pct}%` : ""}`;
    trailing = null;
  } else if (state.kind === "error") {
    hint = (
      <span className="text-red-700">
        {t("settings.updates.error", { message: state.message })}
      </span>
    );
  }

  return (
    <Row icon={<FiDownload />} label={t("settings.updates.title")} hint={hint}>
      {trailing}
    </Row>
  );
}

function AboutRow() {
  const { t } = useTranslation();
  const [version, setVersion] = useState<string>("");

  useEffect(() => {
    invoke<string>("app_version").then(setVersion).catch(() => {});
  }, []);

  return (
    <>
      <Row icon={<FiInfo />} label={t("settings.about.version")}>
        <code className="text-xs text-neutral-700">{version || "…"}</code>
      </Row>
      <Row icon={<FiExternalLink />} label={t("settings.about.github")}>
        <button
          onClick={() => openUrl("https://github.com/LAMP-Bench/lamp")}
          className="px-3 py-1.5 rounded border border-neutral-300 hover:bg-neutral-50 text-sm flex items-center gap-1.5"
        >
          <FiExternalLink />
          {t("settings.about.openRepo")}
        </button>
      </Row>
      <Row icon={<FiInfo />} label={t("settings.about.license")}>
        <code className="text-xs text-neutral-700">MIT</code>
      </Row>
    </>
  );
}
