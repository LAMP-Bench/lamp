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
} from "react-icons/fi";
import { LANGUAGES } from "../i18n";

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
