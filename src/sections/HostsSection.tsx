import { FormEvent, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  FiPlus,
  FiMinus,
  FiExternalLink,
  FiFolder,
  FiX,
  FiGitBranch,
  FiSave,
  FiRotateCcw,
} from "react-icons/fi";
import type { Host } from "../types";

type Tab = "general" | "apache" | "nginx" | "ssl" | "extras";

export function HostsSection() {
  const [hosts, setHosts] = useState<Host[]>([]);
  const [versions, setVersions] = useState<string[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh(preferId?: number | null) {
    try {
      const [hostList, vers] = await Promise.all([
        invoke<Host[]>("host_list"),
        invoke<string[]>("php_versions"),
      ]);
      setHosts(hostList);
      setVersions(vers);
      const wanted = preferId ?? selectedId;
      if (wanted == null && hostList.length) {
        setSelectedId(hostList[0].id);
      } else if (wanted != null && !hostList.some((h) => h.id === wanted)) {
        setSelectedId(hostList[0]?.id ?? null);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  const selected = useMemo(
    () => hosts.find((h) => h.id === selectedId) ?? null,
    [hosts, selectedId]
  );

  async function del(id: number) {
    try {
      await invoke("host_delete", { id });
      if (selectedId === id) setSelectedId(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="h-full flex bg-white">
      <HostList
        hosts={hosts}
        selectedId={selectedId}
        onSelect={(id) => {
          setAdding(false);
          setSelectedId(id);
        }}
        onAdd={() => setAdding(true)}
        onDelete={() => selected && del(selected.id)}
      />

      <div className="flex-1 min-w-0 overflow-auto">
        {adding ? (
          <AddHostForm
            versions={versions}
            onCancel={() => {
              setAdding(false);
              setError(null);
            }}
            onCreated={async (newId) => {
              setAdding(false);
              await refresh(newId);
            }}
            error={error}
            setError={setError}
          />
        ) : selected ? (
          <HostDetail
            key={selected.id}
            host={selected}
            versions={versions}
            onSaved={(updated) => refresh(updated.id)}
          />
        ) : (
          <div className="h-full flex items-center justify-center text-sm text-neutral-400">
            No host selected.
          </div>
        )}
      </div>
    </div>
  );
}

function HostList({
  hosts,
  selectedId,
  onSelect,
  onAdd,
  onDelete,
}: {
  hosts: Host[];
  selectedId: number | null;
  onSelect: (id: number) => void;
  onAdd: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="w-[320px] shrink-0 border-r border-neutral-200 bg-white flex flex-col">
      <div className="px-4 py-2 grid grid-cols-[1fr_70px_50px] gap-3 text-[11px] uppercase tracking-wider text-neutral-500 border-b border-neutral-200">
        <span>Name</span>
        <span>PHP</span>
        <span>Extras</span>
      </div>
      <div className="flex-1 overflow-auto">
        {hosts.length === 0 ? (
          <div className="p-6 text-center text-xs text-neutral-400">
            No hosts yet. Click + to add one.
          </div>
        ) : (
          hosts.map((h) => {
            const sel = h.id === selectedId;
            const hasExtras =
              (h.apache_extra && h.apache_extra.trim().length > 0) ||
              (h.nginx_extra && h.nginx_extra.trim().length > 0);
            return (
              <button
                key={h.id}
                onClick={() => onSelect(h.id)}
                className={`w-full grid grid-cols-[1fr_70px_50px] gap-3 items-center px-4 py-2 text-left text-sm border-l-[3px] transition ${
                  sel
                    ? "bg-sky-500 text-white border-sky-700"
                    : "border-transparent text-neutral-800 hover:bg-neutral-50"
                }`}
              >
                <span className="font-mono truncate">{h.name}</span>
                <span
                  className={`text-xs font-mono ${
                    sel ? "text-sky-100" : "text-neutral-500"
                  }`}
                >
                  {h.php_version}
                </span>
                <span
                  className={`text-xs ${
                    sel ? "text-sky-100" : "text-neutral-400"
                  }`}
                >
                  {hasExtras ? "•" : ""}
                </span>
              </button>
            );
          })
        )}
      </div>
      <div className="border-t border-neutral-200 px-3 py-1.5 flex items-center gap-1">
        <button
          onClick={onAdd}
          className="p-1.5 rounded hover:bg-neutral-100 text-neutral-600"
          title="Add host"
        >
          <FiPlus />
        </button>
        <button
          onClick={onDelete}
          disabled={selectedId == null}
          className="p-1.5 rounded hover:bg-neutral-100 text-neutral-600 disabled:opacity-30 disabled:cursor-not-allowed"
          title="Delete selected host"
        >
          <FiMinus />
        </button>
      </div>
    </div>
  );
}

function HostDetail({
  host,
  versions,
  onSaved,
}: {
  host: Host;
  versions: string[];
  onSaved: (updated: Host) => void;
}) {
  const [tab, setTab] = useState<Tab>("general");
  const [draft, setDraft] = useState<Host>(host);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);

  // Reset draft when the selected host changes (key prop in parent already remounts,
  // but keep this defensive).
  useEffect(() => {
    setDraft(host);
    setError(null);
    setInfo(null);
  }, [host.id]);

  const dirty =
    draft.name !== host.name ||
    draft.docroot !== host.docroot ||
    draft.php_version !== host.php_version ||
    draft.apache_extra !== host.apache_extra ||
    draft.nginx_extra !== host.nginx_extra;

  async function save() {
    setBusy(true);
    setError(null);
    setInfo(null);
    try {
      const updated = await invoke<Host>("host_update", {
        id: draft.id,
        name: draft.name,
        docroot: draft.docroot,
        phpVersion: draft.php_version,
        apacheExtra: draft.apache_extra,
        nginxExtra: draft.nginx_extra,
      });
      setInfo("Saved.");
      onSaved(updated);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function revert() {
    setDraft(host);
    setError(null);
    setInfo(null);
  }

  const tabs: Array<{ id: Tab; label: string; enabled: boolean }> = [
    { id: "general", label: "General", enabled: true },
    { id: "apache", label: "Apache", enabled: true },
    { id: "nginx", label: "Nginx", enabled: true },
    { id: "ssl", label: "SSL", enabled: true },
    { id: "extras", label: "Extras", enabled: false },
  ];

  return (
    <div className="h-full flex flex-col">
      <div className="border-b border-neutral-200 px-6 flex items-end gap-5">
        {tabs.map((t) => {
          const isActive = tab === t.id;
          return (
            <button
              key={t.id}
              onClick={() => t.enabled && setTab(t.id)}
              disabled={!t.enabled}
              className={`pb-2 pt-3 text-sm border-b-2 transition ${
                isActive
                  ? "border-sky-500 text-sky-600 font-medium"
                  : t.enabled
                  ? "border-transparent text-neutral-600 hover:text-neutral-900"
                  : "border-transparent text-neutral-300 cursor-not-allowed"
              }`}
              title={t.enabled ? "" : "Comes in a later phase"}
            >
              {t.label}
            </button>
          );
        })}
      </div>

      <div className="flex-1 overflow-auto p-6">
        {tab === "general" && (
          <GeneralTab draft={draft} setDraft={setDraft} versions={versions} />
        )}
        {tab === "apache" && (
          <ExtrasTab
            label="Extra Apache directives (injected inside this host's <VirtualHost>)"
            placeholder={`# Example:\nDirectoryIndex index.php\n<Directory ".../public">\n    AllowOverride All\n</Directory>`}
            value={draft.apache_extra}
            onChange={(v) => setDraft({ ...draft, apache_extra: v })}
          />
        )}
        {tab === "nginx" && (
          <ExtrasTab
            label="Extra Nginx directives (injected inside this host's server { } block)"
            placeholder={`# Example:\nclient_max_body_size 50m;\nrewrite ^/old-page$ /new-page permanent;`}
            value={draft.nginx_extra}
            onChange={(v) => setDraft({ ...draft, nginx_extra: v })}
          />
        )}
        {tab === "ssl" && <SslTab host={host} />}
      </div>

      <div className="border-t border-neutral-200 px-6 py-2.5 flex items-center gap-3 bg-neutral-50">
        {error && (
          <span className="text-xs text-red-600 font-mono break-words">
            {error}
          </span>
        )}
        {info && !error && (
          <span className="text-xs text-emerald-600">{info}</span>
        )}
        <div className="ml-auto flex items-center gap-2">
          <button
            onClick={revert}
            disabled={!dirty || busy}
            className="px-3 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm flex items-center gap-1.5 disabled:opacity-40"
          >
            <FiRotateCcw />
            Revert
          </button>
          <button
            onClick={save}
            disabled={!dirty || busy}
            className="px-4 py-1.5 rounded bg-sky-500 hover:bg-sky-600 text-white text-sm font-medium flex items-center gap-1.5 disabled:opacity-40"
          >
            <FiSave />
            {busy ? "Saving…" : dirty ? "Save" : "Saved"}
          </button>
        </div>
      </div>
    </div>
  );
}

function GeneralTab({
  draft,
  setDraft,
  versions,
}: {
  draft: Host;
  setDraft: (h: Host) => void;
  versions: string[];
}) {
  return (
    <div className="max-w-2xl space-y-4 text-sm">
      <Field label="Host name">
        <input
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
          className="w-72 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-100"
        />
        <button
          onClick={() => openUrl(`http://${draft.name}:8080/`)}
          disabled={!draft.name}
          className="px-3 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 flex items-center gap-1.5 text-xs disabled:opacity-50"
        >
          <FiExternalLink />
          HTTP
        </button>
        <button
          onClick={() => openUrl(`https://${draft.name}:8443/`)}
          disabled={!draft.name}
          className="px-3 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 flex items-center gap-1.5 text-xs disabled:opacity-50"
        >
          <FiExternalLink />
          HTTPS
        </button>
      </Field>

      <Field label="PHP version">
        <select
          value={draft.php_version}
          onChange={(e) => setDraft({ ...draft, php_version: e.target.value })}
          className="w-32 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500 bg-white"
        >
          {versions.map((v) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
        </select>
      </Field>

      <Field label="Document root">
        <input
          value={draft.docroot}
          onChange={(e) => setDraft({ ...draft, docroot: e.target.value })}
          className="flex-1 min-w-0 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500"
        />
        <button
          onClick={() => openUrl(draft.docroot)}
          disabled={!draft.docroot}
          className="px-3 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 flex items-center gap-1.5 text-xs disabled:opacity-50"
        >
          <FiFolder />
          Open
        </button>
      </Field>

      <Field label="Ports">
        <span className="text-neutral-600 font-mono text-xs">
          Apache :8080 (HTTP) · :8443 (HTTPS) · Nginx :8081 / :8444
        </span>
      </Field>
    </div>
  );
}

function ExtrasTab({
  label,
  placeholder,
  value,
  onChange,
}: {
  label: string;
  placeholder: string;
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div className="h-full flex flex-col gap-2">
      <div className="text-xs text-neutral-600">{label}</div>
      <textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        spellCheck={false}
        placeholder={placeholder}
        className="flex-1 min-h-[280px] p-3 rounded border border-neutral-300 font-mono text-xs focus:outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-100 resize-none"
      />
      <div className="text-[11px] text-neutral-500">
        Saved as part of this host. Lines are injected verbatim. Apache restarts
        after Save.
      </div>
    </div>
  );
}

function SslTab({ host }: { host: Host }) {
  return (
    <div className="max-w-2xl space-y-3 text-sm">
      <p className="text-neutral-600">
        Each host gets an auto-generated cert signed by the local Lamp Bench
        CA. The CA is installed to your{" "}
        <span className="font-mono">CurrentUser\\Root</span> trust store on
        first Apache start.
      </p>
      <Field label="Cert file">
        <code className="font-mono text-xs text-neutral-700 break-all">
          .lamp-bench/ssl/{host.name}.crt
        </code>
      </Field>
      <Field label="Key file">
        <code className="font-mono text-xs text-neutral-700 break-all">
          .lamp-bench/ssl/{host.name}.key
        </code>
      </Field>
      <Field label="HTTPS URL">
        <a
          href="#"
          onClick={(e) => {
            e.preventDefault();
            openUrl(`https://${host.name}:8443/`);
          }}
          className="text-sky-600 hover:underline font-mono text-xs"
        >
          https://{host.name}:8443/
        </a>
      </Field>
      <div className="rounded border border-amber-200 bg-amber-50 p-3 text-xs text-amber-800">
        <strong>Firefox:</strong> set{" "}
        <span className="font-mono">security.enterprise_roots.enabled = true</span>{" "}
        in <span className="font-mono">about:config</span>, or import the CA at{" "}
        <span className="font-mono">.lamp-bench/ca/ca.crt</span> manually.
      </div>
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-[140px_1fr] gap-4 items-center">
      <label className="text-neutral-600 text-right">{label}:</label>
      <div className="flex items-center gap-2 flex-wrap">{children}</div>
    </div>
  );
}

function AddHostForm({
  versions,
  onCancel,
  onCreated,
  error,
  setError,
}: {
  versions: string[];
  onCancel: () => void;
  onCreated: (id: number) => void;
  error: string | null;
  setError: (s: string | null) => void;
}) {
  const [name, setName] = useState("");
  const [docroot, setDocroot] = useState("");
  const [phpVersion, setPhpVersion] = useState(
    versions[versions.length - 1] ?? ""
  );
  const [initGit, setInitGit] = useState(false);
  const [gitAvailable, setGitAvailable] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    invoke<boolean>("git_available").then(setGitAvailable);
  }, []);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const host = await invoke<Host>("host_create", {
        name: name.trim(),
        docroot: docroot.trim(),
        phpVersion,
      });
      if (initGit && gitAvailable) {
        try {
          await invoke("git_init", { path: docroot.trim() });
        } catch (e) {
          setError(`Host created, but git init failed: ${e}`);
          return;
        }
      }
      onCreated(host.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="h-full flex flex-col">
      <div className="border-b border-neutral-200 px-6 py-3 flex items-center justify-between">
        <h2 className="text-sm font-semibold text-neutral-800">Add host</h2>
        <button
          onClick={onCancel}
          className="p-1 rounded hover:bg-neutral-100 text-neutral-500"
        >
          <FiX />
        </button>
      </div>
      <form onSubmit={submit} className="flex-1 overflow-auto p-6 space-y-4 text-sm">
        <div className="max-w-2xl space-y-4">
          <Field label="Host name">
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="myproject.local"
              autoFocus
              className="w-72 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-100"
            />
          </Field>

          <Field label="PHP version">
            <select
              value={phpVersion}
              onChange={(e) => setPhpVersion(e.target.value)}
              className="w-32 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-100 bg-white"
            >
              {versions.map((v) => (
                <option key={v} value={v}>
                  {v}
                </option>
              ))}
            </select>
          </Field>

          <Field label="Document root">
            <input
              value={docroot}
              onChange={(e) => setDocroot(e.target.value)}
              placeholder="C:/path/to/project"
              className="flex-1 min-w-0 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-100"
            />
          </Field>

          {gitAvailable && (
            <Field label="">
              <label className="flex items-center gap-2 text-sm text-neutral-700 cursor-pointer">
                <input
                  type="checkbox"
                  checked={initGit}
                  onChange={(e) => setInitGit(e.target.checked)}
                  className="size-4"
                />
                <FiGitBranch className="text-neutral-500" />
                <span>Initialize git repo in the document root</span>
              </label>
            </Field>
          )}
        </div>

        {error && (
          <div className="max-w-2xl text-xs text-red-600 font-mono break-words bg-red-50 border border-red-200 rounded p-2">
            {error}
          </div>
        )}

        <div className="max-w-2xl pt-2 flex items-center gap-2">
          <button
            type="submit"
            disabled={busy || !name.trim() || !docroot.trim() || !phpVersion}
            className="px-4 py-2 rounded bg-sky-500 hover:bg-sky-600 text-white font-medium disabled:opacity-50"
          >
            {busy ? "Saving…" : "Save"}
          </button>
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-2 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50"
          >
            Cancel
          </button>
          <span className="ml-auto text-xs text-neutral-500">
            Triggers a UAC prompt to update <span className="font-mono">hosts</span>.
          </span>
        </div>
      </form>
    </div>
  );
}
