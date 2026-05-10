import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { HostsSection } from "./sections/HostsSection";
import { ToolsSection } from "./sections/ToolsSection";
import { EditorSection } from "./sections/EditorSection";
import { LogsSection } from "./sections/LogsSection";
import type { SectionId } from "./types";

const TITLES: Record<SectionId, string> = {
  hosts: "Hosts",
  tools: "Tools",
  editor: "Editor",
  logs: "Logs",
};

function App() {
  const [section, setSection] = useState<SectionId>("hosts");
  const [version, setVersion] = useState("");
  const [editorInitialPath, setEditorInitialPath] = useState<string | null>(
    null
  );

  useEffect(() => {
    invoke<string>("app_version").then(setVersion);
  }, []);

  function openInEditor(path: string) {
    setEditorInitialPath(path);
    setSection("editor");
  }

  return (
    <div className="h-screen bg-white text-neutral-900 grid grid-cols-[240px_1fr]">
      <Sidebar active={section} onSelect={setSection} version={version} />
      <div className="flex flex-col min-w-0 min-h-0">
        <TopBar title={TITLES[section]} />
        <main className="flex-1 min-h-0 overflow-hidden">
          {section === "hosts" && <HostsSection />}
          {section === "tools" && (
            <ToolsSection openInEditor={openInEditor} />
          )}
          {section === "editor" && (
            <EditorSection
              initialPath={editorInitialPath}
              onConsumedInitialPath={() => setEditorInitialPath(null)}
            />
          )}
          {section === "logs" && <LogsSection />}
        </main>
      </div>
    </div>
  );
}

export default App;
