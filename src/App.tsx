import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { UpdateBanner } from "./components/UpdateBanner";
import { HomeSection } from "./sections/HomeSection";
import { HostsSection } from "./sections/HostsSection";
import { ToolsSection } from "./sections/ToolsSection";
import { ConfigSection } from "./sections/ConfigSection";
import { EditorSection } from "./sections/EditorSection";
import { LogsSection } from "./sections/LogsSection";
import { SettingsSection } from "./sections/SettingsSection";
import { SetupWizard, setupNeeded } from "./components/SetupWizard";
import type { SectionId } from "./types";

/// Detect editor-window mode from the URL hash. New editor windows are
/// spawned by the Rust `editor_open` command with `#editor=<path>`.
function readEditorPathFromHash(): string | null {
  const hash = window.location.hash;
  const match = hash.match(/editor=([^&]+)/);
  if (!match) return null;
  try {
    return decodeURIComponent(match[1]);
  } catch {
    return match[1];
  }
}

function App() {
  const editorPath = useMemo(() => readEditorPathFromHash(), []);

  // Standalone editor window — show only the editor full-screen.
  if (editorPath !== null) {
    return (
      <div className="h-screen bg-white text-neutral-900 flex flex-col">
        <EditorSection initialPath={editorPath} fullscreen />
      </div>
    );
  }

  return <Bootstrap />;
}

/// Gates the main app behind the first-launch setup wizard. The slim
/// installer ships the binary alone; on the first run the wizard
/// downloads Apache/MySQL/PHP/etc. into `<install>/resources/`. Every
/// subsequent launch skips the wizard because the binaries are now on
/// disk (binary_installed returns true).
function Bootstrap() {
  const [phase, setPhase] = useState<
    | { kind: "checking" }
    | { kind: "setup" }
    | { kind: "ready" }
  >({ kind: "checking" });

  useEffect(() => {
    setupNeeded()
      .then(({ needed }) => {
        setPhase({ kind: needed ? "setup" : "ready" });
      })
      .catch(() => setPhase({ kind: "ready" }));
  }, []);

  if (phase.kind === "checking") {
    return <div className="h-screen bg-neutral-50" />;
  }
  if (phase.kind === "setup") {
    return <SetupWizard onComplete={() => setPhase({ kind: "ready" })} />;
  }
  return <MainShell />;
}

function MainShell() {
  const { t } = useTranslation();
  const [section, setSection] = useState<SectionId>("home");
  const [version, setVersion] = useState("");

  useEffect(() => {
    invoke<string>("app_version").then(setVersion);
  }, []);

  return (
    <div className="h-screen bg-white text-neutral-900 grid grid-cols-[240px_1fr] overflow-hidden">
      <Sidebar active={section} onSelect={setSection} version={version} />
      <div className="flex flex-col min-w-0 min-h-0">
        <UpdateBanner />
        <TopBar title={t(`nav.${section}`)} />
        <main className="flex-1 min-h-0 overflow-hidden">
          {section === "home" && <HomeSection onNavigate={setSection} />}
          {section === "hosts" && <HostsSection />}
          {section === "tools" && <ToolsSection />}
          {section === "config" && <ConfigSection />}
          {section === "logs" && <LogsSection />}
          {section === "settings" && <SettingsSection />}
        </main>
      </div>
    </div>
  );
}

export default App;
