import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FiPause, FiPlay } from "react-icons/fi";
import { SiApache, SiNginx, SiMysql } from "react-icons/si";
import type { LogName } from "../types";

const TABS: Array<{
  id: LogName;
  label: string;
  icon: typeof SiApache;
  color: string;
}> = [
  { id: "apache", label: "apache", icon: SiApache, color: "text-red-500" },
  { id: "nginx", label: "nginx", icon: SiNginx, color: "text-emerald-500" },
  { id: "mysql", label: "mysql", icon: SiMysql, color: "text-sky-500" },
];

export function LogsSection() {
  const [service, setService] = useState<LogName>("apache");
  const [log, setLog] = useState("");
  const [paused, setPaused] = useState(false);
  const preRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    if (paused) return;
    let cancelled = false;
    async function tick() {
      try {
        const t = await invoke<string>("read_log", { service, lines: 300 });
        if (!cancelled) setLog(t);
      } catch (e) {
        if (!cancelled) setLog(`<error: ${e}>`);
      }
    }
    tick();
    const id = setInterval(tick, 1500);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [service, paused]);

  useEffect(() => {
    if (preRef.current) {
      preRef.current.scrollTop = preRef.current.scrollHeight;
    }
  }, [log]);

  return (
    <div className="h-full flex flex-col p-6">
      <div className="flex items-center justify-between mb-3">
        <div className="flex gap-1 p-1 rounded-md bg-neutral-100 border border-neutral-200">
          {TABS.map((t) => {
            const Icon = t.icon;
            const isActive = service === t.id;
            return (
              <button
                key={t.id}
                onClick={() => setService(t.id)}
                className={`px-3 py-1 rounded text-sm font-mono flex items-center gap-2 transition ${
                  isActive
                    ? "bg-white text-neutral-900 shadow-sm"
                    : "text-neutral-500 hover:text-neutral-700"
                }`}
              >
                <Icon className={isActive ? t.color : ""} />
                {t.label}
              </button>
            );
          })}
        </div>
        <button
          onClick={() => setPaused((p) => !p)}
          className="px-3 py-1 rounded text-sm flex items-center gap-1.5 border border-neutral-300 hover:bg-neutral-50 text-neutral-700"
          title={paused ? "Resume polling" : "Pause polling"}
        >
          {paused ? <FiPlay /> : <FiPause />}
          {paused ? "Resume" : "Pause"}
        </button>
      </div>

      <pre
        ref={preRef}
        className="flex-1 rounded-md border border-neutral-200 bg-neutral-950 text-neutral-200 p-3 text-xs font-mono overflow-auto whitespace-pre-wrap break-words"
      >
        {log || <span className="text-neutral-500">(no log content yet)</span>}
      </pre>
    </div>
  );
}
