import { FiEdit2, FiGlobe, FiPower } from "react-icons/fi";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { useService } from "../useService";
import { usePorts } from "../PortsContext";
import { useToast } from "./Toast";
import { serviceErrorText } from "../serviceError";

export function TopBar({ title }: { title: string }) {
  const { t } = useTranslation();
  const toast = useToast();
  const { siteUrl } = usePorts();
  const apache = useService("apache");
  const nginx = useService("nginx");
  const mysql = useService("mysql");

  const anyRunning =
    apache.status?.kind === "running" ||
    nginx.status?.kind === "running" ||
    mysql.status?.kind === "running";

  /// Flip one service and report the failure instead of dropping it. These
  /// buttons drive two or three services in a row, so a silent failure here
  /// used to leave the user with a "Start" button that visibly did nothing.
  async function flip(
    svc: ReturnType<typeof useService>,
    label: string,
  ): Promise<void> {
    const failure = await svc.toggle();
    if (failure) toast("error", serviceErrorText(t, label, failure));
  }

  async function stopAll() {
    if (apache.status?.kind === "running") await flip(apache, "Apache");
    if (nginx.status?.kind === "running") await flip(nginx, "Nginx");
    if (mysql.status?.kind === "running") await flip(mysql, "MySQL");
  }

  async function startCore() {
    if (mysql.status?.kind !== "running") await flip(mysql, "MySQL");
    if (apache.status?.kind !== "running") await flip(apache, "Apache");
  }

  const webStart = () => openUrl(siteUrl());

  /// Opens an editor window with no file loaded — it prompts for a path.
  /// This button had been `disabled` since the standalone editor landed,
  /// which is just a dead control sitting in the main toolbar.
  const openEditor = async () => {
    try {
      await invoke("editor_open", { path: "" });
    } catch (e) {
      toast("error", String(e));
    }
  };

  return (
    <header className="border-b border-neutral-200 bg-white px-5 py-2.5 flex items-center justify-between">
      <h1 className="text-base font-semibold tracking-tight text-neutral-800">
        {title}
      </h1>
      <div className="flex items-center gap-1">
        <ActionButton icon={<FiEdit2 />} label={t("topbar.editor")} onClick={openEditor} />
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
