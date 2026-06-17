import { FiEdit2, FiGlobe, FiPower } from "react-icons/fi";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { useService } from "../useService";

export function TopBar({ title }: { title: string }) {
  const { t } = useTranslation();
  const apache = useService("apache");
  const nginx = useService("nginx");
  const mysql = useService("mysql");

  const anyRunning =
    apache.status?.kind === "running" ||
    nginx.status?.kind === "running" ||
    mysql.status?.kind === "running";

  async function stopAll() {
    if (apache.status?.kind === "running") await apache.toggle();
    if (nginx.status?.kind === "running") await nginx.toggle();
    if (mysql.status?.kind === "running") await mysql.toggle();
  }

  async function startCore() {
    if (apache.status?.kind !== "running") await apache.toggle();
    if (mysql.status?.kind !== "running") await mysql.toggle();
  }

  const webStart = () => openUrl("http://localhost:8080/");

  return (
    <header className="border-b border-neutral-200 bg-white px-5 py-2.5 flex items-center justify-between">
      <h1 className="text-base font-semibold tracking-tight text-neutral-800">
        {title}
      </h1>
      <div className="flex items-center gap-1">
        <ActionButton icon={<FiEdit2 />} label={t("topbar.editor")} disabled />
        <ActionButton icon={<FiGlobe />} label={t("topbar.webstart")} onClick={webStart} />
        <ActionButton
          icon={<FiPower />}
          label={anyRunning ? t("topbar.stop") : t("topbar.start")}
          tone={anyRunning ? "danger" : "primary"}
          onClick={anyRunning ? stopAll : startCore}
        />
      </div>
    </header>
  );
}

function ActionButton({
  icon,
  label,
  onClick,
  tone = "neutral",
  disabled,
}: {
  icon: React.ReactNode;
  label: string;
  onClick?: () => void;
  tone?: "neutral" | "primary" | "danger";
  disabled?: boolean;
}) {
  const colors =
    tone === "danger"
      ? "text-red-600 hover:bg-red-50"
      : tone === "primary"
      ? "text-emerald-600 hover:bg-emerald-50"
      : "text-neutral-700 hover:bg-neutral-100";
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`flex flex-col items-center gap-0.5 px-3 py-1.5 rounded-md text-[11px] font-medium transition disabled:opacity-40 disabled:cursor-not-allowed ${colors}`}
    >
      <span className="text-[18px]">{icon}</span>
      <span>{label}</span>
    </button>
  );
}
