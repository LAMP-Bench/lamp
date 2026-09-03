import { ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { IconType } from "react-icons";
import {
  FiHome,
  FiGlobe,
  FiTool,
  FiTerminal,
  FiSliders,
  FiChevronDown,
  FiChevronUp,
  FiSettings,
  FiDownload,
  FiAlertTriangle,
  FiX,
} from "react-icons/fi";
import { SiApache, SiNginx, SiMysql, SiRedis } from "react-icons/si";
import { FiMail } from "react-icons/fi";
import { LuLamp } from "react-icons/lu";
import { Toggle } from "./Toggle";
import { useToast } from "./Toast";
import { useService } from "../useService";
import { serviceErrorText } from "../serviceError";
import { usePorts } from "../PortsContext";
import {
  cancelBinaryDownload,
  isCancelled,
  useDownloadProgress,
} from "../useDownloadProgress";
import type { SectionId, ServiceName } from "../types";

type NavItem = { id: SectionId; icon: ReactNode };

const NAV_ITEMS: NavItem[] = [
  { id: "home", icon: <FiHome /> },
  { id: "hosts", icon: <FiGlobe /> },
  { id: "tools", icon: <FiTool /> },
  { id: "config", icon: <FiSliders /> },
  { id: "logs", icon: <FiTerminal /> },
];

type SvcSpec = {
  name: ServiceName;
  label: string;
  icon: IconType;
  iconColor: string;
  /// Manifest entry that has to exist on disk before the toggle can flip.
  /// `null` for bundled services that ship with the installer.
  binaryName: string | null;
};

const SERVICES: SvcSpec[] = [
  {
    name: "apache",
    label: "Apache",
    icon: SiApache,
    iconColor: "text-red-500",
    binaryName: null,
  },
  {
    name: "nginx",
    label: "Nginx",
    icon: SiNginx,
    iconColor: "text-emerald-500",
    binaryName: "nginx",
  },
  {
    name: "mysql",
    label: "MySQL",
    icon: SiMysql,
    iconColor: "text-sky-500",
    binaryName: null,
  },
  {
    name: "redis",
    label: "Redis",
    icon: SiRedis,
    iconColor: "text-rose-500",
    binaryName: "redis",
  },
  {
    name: "mailhog",
    label: "MailHog",
    icon: FiMail,
    iconColor: "text-amber-500",
    binaryName: "mailhog",
  },
];

export function Sidebar({
  active,
  onSelect,
  version,
}: {
  active: SectionId;
  onSelect: (id: SectionId) => void;
  version: string;
}) {
  const { t } = useTranslation();
  return (
    <aside className="bg-neutral-50 border-r border-neutral-200 flex flex-col text-sm">
      <div className="px-4 py-4 flex items-center gap-3">
        <div className="size-9 rounded-lg bg-gradient-to-br from-amber-400 to-orange-500 flex items-center justify-center text-white shadow-sm">
          <LuLamp className="text-xl" />
        </div>
        <div className="min-w-0">
          <div className="font-semibold tracking-tight text-neutral-900 truncate">
            Lamp Bench
          </div>
          <div className="text-[11px] text-neutral-500 font-mono">
            v{version || "…"}
          </div>
        </div>
      </div>

      <Group label={t("nav.navigation")} defaultOpen>
        {NAV_ITEMS.map((item) => {
          const isActive = active === item.id;
          return (
            <button
              key={item.id}
              onClick={() => onSelect(item.id)}
              className={`relative w-full flex items-center gap-3 pl-4 pr-3 py-1.5 text-left transition ${
                isActive
                  ? "bg-sky-50 text-sky-700 font-medium"
                  : "text-neutral-700 hover:bg-neutral-100"
              }`}
            >
              {isActive && (
                <span className="absolute inset-y-0 left-0 w-[3px] bg-sky-500" />
              )}
              <span className="text-[15px] text-neutral-500">{item.icon}</span>
              <span>{t(`nav.${item.id}`)}</span>
            </button>
          );
        })}
      </Group>

      <Group label={t("nav.servers")} defaultOpen>
        {SERVICES.map((s) => (
          <ServiceRow key={s.name} spec={s} />
        ))}
      </Group>

      <div className="flex-1" />

      <div className="border-t border-neutral-200 px-3 py-2 flex items-center justify-between text-[11px] text-neutral-500">
        <span className="font-mono">alpha · v{version || "…"}</span>
        <button
          onClick={() => onSelect("settings")}
          className={`p-1 rounded hover:bg-neutral-200 transition ${
            active === "settings" ? "text-sky-600" : "text-neutral-400"
          }`}
          title="Settings"
        >
          <FiSettings />
        </button>
      </div>
    </aside>
  );
}

function Group({
  label,
  defaultOpen = true,
  children,
}: {
  label: string;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="border-t border-neutral-200">
      <button
        onClick={() => setOpen((o) => !o)}
        className="w-full px-4 py-2 flex items-center justify-between text-[11px] uppercase tracking-wider text-neutral-500 font-medium hover:text-neutral-700"
      >
        <span>{label}</span>
        {open ? (
          <FiChevronUp className="text-neutral-400" />
        ) : (
          <FiChevronDown className="text-neutral-400" />
        )}
      </button>
      {open && <div className="pb-1">{children}</div>}
    </div>
  );
}

type PortCfg = { port: number; port2: number; has_secondary: boolean };

function ServiceRow({ spec }: { spec: SvcSpec }) {
  const { t } = useTranslation();
  const { status, busy, error, toggle } = useService(spec.name);
  const toast = useToast();
  const { refresh: refreshPorts } = usePorts();
  const running = status?.kind === "running";
  const Icon = spec.icon;

  // null = not yet checked, true = files exist, false = on-demand and missing.
  const [installed, setInstalled] = useState<boolean | null>(
    spec.binaryName == null ? true : null
  );
  const [installing, setInstalling] = useState(false);
  const pct = useDownloadProgress(installing ? spec.binaryName : null);
  const [expanded, setExpanded] = useState(false);
  const [cfg, setCfg] = useState<PortCfg | null>(null);
  const [versions, setVersions] = useState<string[]>([]);
  const [activeVersion, setActiveVersion] = useState("");
  const [savedFlash, setSavedFlash] = useState(false);

  useEffect(() => {
    if (spec.binaryName == null) return;
    invoke<boolean>("binary_installed", { name: spec.binaryName })
      .then(setInstalled)
      .catch(() => setInstalled(false));
  }, [spec.binaryName]);

  /// The toggle used to swallow every failure: the switch flipped, sprang
  /// back, and nothing said why. Surface it as a toast — error toasts stay
  /// until dismissed, so the message survives long enough to read.
  async function onToggle() {
    const failure = await toggle();
    if (failure) toast("error", serviceErrorText(t, spec.label, failure));
  }

  // Whether binaries.json has an entry for this component on the current OS
  // at all. Apache, nginx, PHP and Redis have no Unix entry, upstream ships
  // source tarballs only, and an Install button that always errors reads
  // like a bug rather than a platform gap.
  const [available, setAvailable] = useState(true);
  useEffect(() => {
    if (spec.binaryName == null) return;
    invoke<boolean>("binary_available", { name: spec.binaryName })
      .then(setAvailable)
      .catch(() => setAvailable(true));
  }, [spec.binaryName]);

  async function install() {
    if (spec.binaryName == null) return;
    setInstalling(true);
    try {
      await invoke("binary_download", { name: spec.binaryName });
      setInstalled(true);
    } catch (e) {
      // The user pressing stop isn't a failure worth shouting about.
      if (!isCancelled(e)) toast("error", `Install failed: ${e}`);
    } finally {
      setInstalling(false);
    }
  }

  async function openConfig() {
    if (expanded) {
      setExpanded(false);
      return;
    }
    setSavedFlash(false);
    const c = await invoke<PortCfg>("service_ports_get", { name: spec.name }).catch(
      () => null
    );
    setCfg(c);
    if (spec.name === "mysql") {
      const [vs, av] = await Promise.all([
        invoke<string[]>("mysql_versions").catch(() => []),
        invoke<string>("mysql_active_version").catch(() => ""),
      ]);
      setVersions(vs);
      setActiveVersion(av);
    }
    setExpanded(true);
  }

  async function save() {
    if (!cfg) return;
    if (cfg.port < 1 || cfg.port > 65535) {
      toast("error", t("svcConfig.invalidPort"));
      return;
    }
    try {
      await invoke("service_ports_set", {
        name: spec.name,
        port: cfg.port,
        port2: cfg.has_secondary ? cfg.port2 : 0,
      });
      // Links all over the app are built from these, so pull the new value
      // through instead of leaving them on the stale one until a restart.
      await refreshPorts();
      setSavedFlash(true);
    } catch (e) {
      toast("error", String(e));
    }
  }

  async function changeVersion(v: string) {
    try {
      await invoke("mysql_set_version", { version: v });
      setActiveVersion(v);
    } catch (e) {
      toast("error", String(e));
    }
  }

  return (
    <div>
      <div className="px-4 py-1.5 flex items-center gap-2">
        <Icon className={`text-[15px] ${spec.iconColor}`} />
        <div className="flex-1 flex items-center gap-1.5 text-neutral-800 min-w-0">
          <span className="truncate">{spec.label}</span>
          {running && (
            <span
              className="size-1.5 rounded-full bg-emerald-500 shrink-0"
              title={t("svcConfig.running")}
            />
          )}
          {!running && error && (
            // The toast can be dismissed; this keeps the failure visible
            // until the next attempt succeeds.
            <FiAlertTriangle
              className="text-red-500 text-[12px] shrink-0"
              title={serviceErrorText(t, spec.label, error)}
            />
          )}
        </div>
        {installed && (
          <button
            onClick={openConfig}
            className={`p-1 rounded hover:bg-neutral-200 transition ${
              expanded ? "text-sky-600" : "text-neutral-400"
            }`}
            title={t("svcConfig.configure")}
          >
            <FiSettings className="text-[13px]" />
          </button>
        )}
        {installed === null ? (
          <span className="text-[10px] text-neutral-400 font-mono">…</span>
        ) : installed ? (
          <Toggle checked={running} onChange={onToggle} disabled={busy} />
        ) : !available ? (
          <span
            className="px-2 py-0.5 rounded text-[11px] text-neutral-400 border border-neutral-200"
            title={t("versions.unavailableHint")}
          >
            {t("versions.unavailable")}
          </span>
        ) : (
          installing ? (
            <button
              onClick={() => spec.binaryName && cancelBinaryDownload(spec.binaryName)}
              className="px-2 py-0.5 rounded text-[11px] font-medium text-neutral-600 border border-neutral-300 hover:bg-neutral-100 flex items-center gap-1"
              title={t("versions.cancelDownload")}
            >
              {pct !== null ? `${pct}%` : "…"}
              <FiX className="text-[10px]" />
            </button>
          ) : (
            <button
              onClick={install}
              className="px-2 py-0.5 rounded text-[11px] font-medium text-sky-700 border border-sky-300 hover:bg-sky-50 flex items-center gap-1"
              title={t("versions.install")}
            >
              <FiDownload className="text-[10px]" />
              {t("versions.install")}
            </button>
          )
        )}
      </div>

      {expanded && cfg && (
        <div className="mx-3 mb-2 px-3 py-2 rounded-md bg-neutral-100 border border-neutral-200 space-y-2 text-[11px]">
          <PortField
            label={t("svcConfig.port")}
            value={cfg.port}
            onChange={(v) => setCfg({ ...cfg, port: v })}
          />
          {cfg.has_secondary && (
            <PortField
              label={spec.name === "mailhog" ? t("svcConfig.portSmtp") : t("svcConfig.portHttps")}
              value={cfg.port2}
              onChange={(v) => setCfg({ ...cfg, port2: v })}
            />
          )}
          {spec.name === "mysql" && versions.length > 1 && (
            <label className="flex items-center justify-between gap-2">
              <span className="text-neutral-600">{t("svcConfig.version")}</span>
              <select
                value={activeVersion}
                onChange={(e) => changeVersion(e.target.value)}
                disabled={running}
                title={running ? t("svcConfig.stopFirst") : ""}
                className="px-2 py-0.5 rounded border border-neutral-300 bg-white font-mono disabled:opacity-50"
              >
                {versions.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
            </label>
          )}
          <div className="flex items-center justify-between pt-0.5">
            <span className="text-neutral-400">
              {savedFlash ? t("svcConfig.saved") : t("svcConfig.restartHint")}
            </span>
            <button
              onClick={save}
              className="px-2 py-0.5 rounded bg-sky-600 hover:bg-sky-700 text-white"
            >
              {t("svcConfig.save")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function PortField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-2">
      <span className="text-neutral-600">{label}</span>
      <input
        type="number"
        value={value}
        min={1}
        max={65535}
        onChange={(e) => onChange(Number(e.target.value) || 0)}
        className="w-20 px-2 py-0.5 rounded border border-neutral-300 bg-white font-mono text-right focus:outline-none focus:border-sky-500"
      />
    </label>
  );
}
