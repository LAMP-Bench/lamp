import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  FiGlobe,
  FiTool,
  FiSliders,
  FiTerminal,
  FiFolder,
  FiExternalLink,
  FiSmartphone,
} from "react-icons/fi";
import { SiApache, SiNginx, SiMysql, SiPhp } from "react-icons/si";
import { useService } from "../useService";
import { MobileQRModal } from "../components/MobileQRModal";
import type { Host, SectionId } from "../types";

export function HomeSection({
  onNavigate,
}: {
  onNavigate: (id: SectionId) => void;
}) {
  const { t } = useTranslation();
  const apache = useService("apache");
  const nginx = useService("nginx");
  const mysql = useService("mysql");
  const redis = useService("redis");
  const [hosts, setHosts] = useState<Host[]>([]);
  const [htdocs, setHtdocs] = useState<string>("");
  const [qrOpen, setQrOpen] = useState(false);

  useEffect(() => {
    invoke<Host[]>("host_list").then(setHosts).catch(() => {});
    invoke<string>("htdocs_path").then(setHtdocs).catch(() => {});
  }, []);

  return (
    <div className="h-full overflow-y-auto">
      <div className="p-6 max-w-4xl space-y-6">
        <section>
          <h2 className="text-xs uppercase tracking-wider text-neutral-500 font-medium mb-2">
            {t("home.services")}
          </h2>
          <div className="grid grid-cols-2 gap-3">
            <StatusBox
              label="Apache"
              port="8080 · 8443"
              icon={<SiApache className="text-red-500" />}
              status={apache.status}
            />
            <StatusBox
              label="MySQL"
              port="3306"
              icon={<SiMysql className="text-sky-500" />}
              status={mysql.status}
            />
            <StatusBox
              label="Nginx"
              port="8081 · 8444"
              icon={<SiNginx className="text-emerald-500" />}
              status={nginx.status}
            />
            <StatusBox
              label="Redis"
              port="6379"
              icon={<span className="text-rose-500 font-bold text-lg">●</span>}
              status={redis.status}
            />
          </div>
        </section>

        <section>
          <h2 className="text-xs uppercase tracking-wider text-neutral-500 font-medium mb-2">
            {t("home.defaultHtdocs")}
          </h2>
          <div className="rounded-lg border border-neutral-200 bg-neutral-50 p-3 flex items-center gap-3">
            <FiFolder className="text-neutral-500 shrink-0" />
            <code className="text-xs flex-1 truncate text-neutral-700">
              {htdocs || "…"}
            </code>
            <button
              onClick={() => openUrl("http://localhost:8080/")}
              className="px-2 py-1 rounded text-xs border border-neutral-300 hover:bg-white flex items-center gap-1.5 text-neutral-700"
            >
              <FiExternalLink />
              localhost:8080
            </button>
          </div>
          <p className="text-xs text-neutral-500 mt-2">
            {t("home.htdocsHint")}{" "}
            <button
              onClick={() => onNavigate("tools")}
              className="text-sky-600 hover:underline"
            >
              {t("home.htdocsHintLink")}
            </button>{" "}
            {t("home.htdocsHintTail")}
          </p>
        </section>

        <section>
          <div className="flex items-baseline justify-between mb-2">
            <h2 className="text-xs uppercase tracking-wider text-neutral-500 font-medium">
              {t("home.virtualHosts")}
            </h2>
            <button
              onClick={() => onNavigate("hosts")}
              className="text-xs text-sky-600 hover:underline"
            >
              {t("home.manage")}
            </button>
          </div>
          {hosts.length === 0 ? (
            <div className="rounded-lg border border-dashed border-neutral-300 bg-neutral-50 p-4 text-center text-sm text-neutral-500">
              {t("home.noHosts")}
              <code className="ml-1 text-neutral-700">localhost:8080</code>.
            </div>
          ) : (
            <div className="rounded-lg border border-neutral-200 bg-white overflow-hidden">
              {hosts.slice(0, 5).map((h) => (
                <div
                  key={h.id}
                  className="px-3 py-2 flex items-center gap-3 text-sm border-b border-neutral-100 last:border-b-0"
                >
                  <FiGlobe className="text-neutral-500" />
                  <span className="font-mono flex-1 truncate">{h.name}</span>
                  <span className="text-xs text-neutral-500 font-mono flex items-center gap-1">
                    <SiPhp className="text-indigo-500" />
                    {h.php_version}
                  </span>
                  <button
                    onClick={() => openUrl(`http://${h.name}:8080/`)}
                    className="text-neutral-400 hover:text-neutral-700"
                    title="Open"
                  >
                    <FiExternalLink />
                  </button>
                </div>
              ))}
              {hosts.length > 5 && (
                <button
                  onClick={() => onNavigate("hosts")}
                  className="w-full px-3 py-2 text-xs text-sky-600 hover:bg-neutral-50"
                >
                  {t("home.moreHosts", { count: hosts.length - 5 })}
                </button>
              )}
            </div>
          )}
        </section>

        <section>
          <h2 className="text-xs uppercase tracking-wider text-neutral-500 font-medium mb-2">
            {t("home.quickAccess")}
          </h2>
          <div className="grid grid-cols-3 gap-3">
            <QuickAction
              icon={<FiGlobe />}
              label={t("home.quickHosts")}
              onClick={() => onNavigate("hosts")}
            />
            <QuickAction
              icon={<FiTool />}
              label={t("home.quickTools")}
              onClick={() => onNavigate("tools")}
            />
            <QuickAction
              icon={<FiSliders />}
              label={t("home.quickConfig")}
              onClick={() => onNavigate("config")}
            />
            <QuickAction
              icon={<FiTerminal />}
              label={t("home.quickLogs")}
              onClick={() => onNavigate("logs")}
            />
            <QuickAction
              icon={<FiExternalLink />}
              label={t("home.quickPhpMyAdmin")}
              onClick={() => openUrl("http://localhost:8080/phpmyadmin/")}
            />
            <QuickAction
              icon={<FiSmartphone />}
              label={t("home.quickMobileQR")}
              onClick={() => setQrOpen(true)}
            />
          </div>
        </section>
      </div>
      <MobileQRModal open={qrOpen} onClose={() => setQrOpen(false)} />
    </div>
  );
}

function StatusBox({
  label,
  port,
  icon,
  status,
}: {
  label: string;
  port: string;
  icon: React.ReactNode;
  status: any;
}) {
  const { t } = useTranslation();
  const running = status?.kind === "running";
  return (
    <div className="rounded-lg border border-neutral-200 bg-white px-4 py-3 flex items-center gap-3">
      <div className="text-xl">{icon}</div>
      <div className="flex-1 min-w-0">
        <div className="font-medium text-sm">{label}</div>
        <div className="text-xs text-neutral-500 font-mono">:{port}</div>
      </div>
      <span
        className={`size-2.5 rounded-full ${
          running
            ? "bg-emerald-500 shadow shadow-emerald-500/50"
            : "bg-neutral-300"
        }`}
        title={running ? t("home.statusRunning") : t("home.statusStopped")}
      />
    </div>
  );
}

function QuickAction({
  icon,
  label,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="rounded-lg border border-neutral-200 bg-white px-3 py-3 flex flex-col items-center gap-1.5 hover:bg-neutral-50 hover:border-neutral-300 transition"
    >
      <span className="text-lg text-neutral-600">{icon}</span>
      <span className="text-xs text-neutral-700">{label}</span>
    </button>
  );
}
