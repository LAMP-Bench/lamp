import { ReactNode, useEffect, useState } from "react";
import { IconType } from "react-icons";
import {
  FiHome,
  FiTool,
  FiTerminal,
  FiEdit3,
  FiChevronDown,
  FiChevronUp,
  FiHelpCircle,
} from "react-icons/fi";
import { SiApache, SiNginx, SiMysql, SiRedis } from "react-icons/si";
import { LuLamp } from "react-icons/lu";
import { invoke } from "@tauri-apps/api/core";
import { Toggle } from "./Toggle";
import { useService } from "../useService";
import type { SectionId, ServiceName } from "../types";

type NavItem = { id: SectionId; label: string; icon: ReactNode };

const NAV_ITEMS: NavItem[] = [
  { id: "hosts", label: "Hosts", icon: <FiHome /> },
  { id: "tools", label: "Tools", icon: <FiTool /> },
  { id: "editor", label: "Editor", icon: <FiEdit3 /> },
  { id: "logs", label: "Logs", icon: <FiTerminal /> },
];

type SvcSpec = {
  name: ServiceName;
  label: string;
  icon: IconType;
  iconColor: string;
};

const SERVICES: SvcSpec[] = [
  { name: "apache", label: "Apache", icon: SiApache, iconColor: "text-red-500" },
  { name: "nginx", label: "Nginx", icon: SiNginx, iconColor: "text-emerald-500" },
  { name: "mysql", label: "MySQL", icon: SiMysql, iconColor: "text-sky-500" },
  { name: "redis", label: "Redis", icon: SiRedis, iconColor: "text-rose-500" },
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

      <Group label="Settings" defaultOpen>
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
              <span>{item.label}</span>
            </button>
          );
        })}
      </Group>

      <Group label="Servers & Services" defaultOpen>
        {SERVICES.map((s) => (
          <ServiceRow key={s.name} spec={s} />
        ))}
      </Group>

      <div className="flex-1" />

      <div className="border-t border-neutral-200 px-3 py-2 flex items-center justify-between text-[11px] text-neutral-500">
        <span className="font-mono">Phase 4 · MVP</span>
        <FiHelpCircle className="text-neutral-400" />
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

function ServiceRow({ spec }: { spec: SvcSpec }) {
  const { status, busy, toggle } = useService(spec.name);
  const running = status?.kind === "running";
  const Icon = spec.icon;

  return (
    <div className="px-4 py-1.5 flex items-center gap-3">
      <Icon className={`text-[15px] ${spec.iconColor}`} />
      <div className="flex-1 flex items-center gap-1.5 text-neutral-800 min-w-0">
        <span className="truncate">{spec.label}</span>
        {spec.name === "mysql" && <MysqlVersionPicker running={running} />}
        {running && (
          <span
            className="size-1.5 rounded-full bg-emerald-500 shrink-0"
            title="running"
          />
        )}
      </div>
      <Toggle checked={running} onChange={toggle} disabled={busy} />
    </div>
  );
}

function MysqlVersionPicker({ running }: { running: boolean }) {
  const [versions, setVersions] = useState<string[]>([]);
  const [active, setActive] = useState("");

  useEffect(() => {
    invoke<string[]>("mysql_versions").then(setVersions);
    invoke<string>("mysql_active_version").then(setActive);
  }, []);

  async function change(v: string) {
    try {
      await invoke("mysql_set_version", { version: v });
      setActive(v);
    } catch (e) {
      alert(String(e));
    }
  }

  if (versions.length <= 1) return null;

  return (
    <select
      value={active}
      onChange={(e) => change(e.target.value)}
      onClick={(e) => e.stopPropagation()}
      disabled={running}
      title={running ? "Stop MySQL before switching" : "Switch MySQL version"}
      className="text-[11px] font-mono bg-transparent border border-neutral-200 rounded px-1 py-0 focus:outline-none focus:border-sky-400 disabled:opacity-50"
    >
      {versions.map((v) => (
        <option key={v} value={v}>
          {v}
        </option>
      ))}
    </select>
  );
}
