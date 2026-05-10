import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Editor, { OnMount } from "@monaco-editor/react";
import { FiFolder, FiSave, FiCheckCircle, FiAlertCircle } from "react-icons/fi";

type LintResult = {
  success: boolean;
  stdout: string;
  stderr: string;
};

function languageForPath(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    php: "php",
    js: "javascript",
    mjs: "javascript",
    ts: "typescript",
    tsx: "typescript",
    jsx: "javascript",
    json: "json",
    html: "html",
    htm: "html",
    css: "css",
    scss: "scss",
    md: "markdown",
    yml: "yaml",
    yaml: "yaml",
    xml: "xml",
    sql: "sql",
    py: "python",
    rb: "ruby",
    sh: "shell",
    rs: "rust",
    conf: "ini",
    ini: "ini",
    env: "ini",
  };
  return map[ext] ?? "plaintext";
}

export function EditorSection({
  initialPath,
  onConsumedInitialPath,
}: {
  initialPath?: string | null;
  onConsumedInitialPath?: () => void;
} = {}) {
  const [path, setPath] = useState("");
  const [content, setContent] = useState("");
  const [originalContent, setOriginalContent] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [lint, setLint] = useState<LintResult | null>(null);
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);

  useEffect(() => {
    if (initialPath) {
      open(initialPath);
      onConsumedInitialPath?.();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialPath]);

  const dirty = content !== originalContent;
  const isPhp = path.toLowerCase().endsWith(".php");

  async function open(p: string) {
    setBusy(true);
    setError(null);
    setInfo(null);
    setLint(null);
    try {
      const c = await invoke<string>("file_read", { path: p });
      setContent(c);
      setOriginalContent(c);
      setPath(p);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function save() {
    if (!path) return;
    setBusy(true);
    setError(null);
    setInfo(null);
    try {
      await invoke("file_write", { path, content });
      setOriginalContent(content);
      setInfo("Saved.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runLint() {
    if (!path || !isPhp) return;
    setBusy(true);
    setError(null);
    setInfo(null);
    setLint(null);
    try {
      const result = await invoke<LintResult>("php_lint", { path });
      setLint(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  // Ctrl+S to save
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        if (path && dirty && !busy) save();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [path, dirty, busy, content]);

  return (
    <div className="h-full flex flex-col bg-white">
      <div className="border-b border-neutral-200 px-5 py-2.5 flex items-center gap-2">
        <FiFolder className="text-neutral-500 shrink-0" />
        <input
          type="text"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") open(path);
          }}
          placeholder="C:/path/to/file.php (Enter to open)"
          className="flex-1 px-2 py-1 rounded border border-neutral-300 bg-white font-mono text-sm focus:outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-100"
        />
        <button
          onClick={() => open(path)}
          disabled={!path || busy}
          className="px-3 py-1 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm disabled:opacity-40"
        >
          Open
        </button>
        <button
          onClick={save}
          disabled={!path || !dirty || busy}
          className="px-3 py-1 rounded bg-sky-500 hover:bg-sky-600 text-white text-sm font-medium flex items-center gap-1.5 disabled:opacity-40 disabled:cursor-not-allowed"
          title="Ctrl+S"
        >
          <FiSave />
          {dirty ? "Save *" : "Saved"}
        </button>
        {isPhp && (
          <button
            onClick={runLint}
            disabled={!path || busy}
            className="px-3 py-1 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm disabled:opacity-40"
          >
            Check syntax
          </button>
        )}
      </div>

      {(error || info || lint) && (
        <div className="border-b border-neutral-200 px-5 py-2 text-xs font-mono flex items-start gap-2">
          {error && (
            <span className="text-red-600 flex items-start gap-1.5">
              <FiAlertCircle className="mt-0.5 shrink-0" />
              {error}
            </span>
          )}
          {info && (
            <span className="text-emerald-600 flex items-center gap-1.5">
              <FiCheckCircle />
              {info}
            </span>
          )}
          {lint && (
            <span
              className={`flex items-start gap-1.5 ${
                lint.success ? "text-emerald-600" : "text-red-600"
              }`}
            >
              {lint.success ? <FiCheckCircle /> : <FiAlertCircle />}
              <span className="break-words">
                {(lint.stdout + lint.stderr).trim() ||
                  (lint.success ? "No syntax errors detected." : "Lint failed.")}
              </span>
            </span>
          )}
        </div>
      )}

      <div className="flex-1 min-h-0">
        {path ? (
          <Editor
            height="100%"
            language={languageForPath(path)}
            value={content}
            onChange={(v) => setContent(v ?? "")}
            onMount={(ed) => {
              editorRef.current = ed;
            }}
            theme="vs"
            options={{
              fontSize: 13,
              minimap: { enabled: true },
              scrollBeyondLastLine: false,
              wordWrap: "on",
              tabSize: 4,
            }}
          />
        ) : (
          <div className="h-full flex items-center justify-center text-sm text-neutral-400">
            Type a file path above and press Enter (or click Open).
          </div>
        )}
      </div>
    </div>
  );
}
