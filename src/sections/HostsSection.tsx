import { FormEvent, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useConfirm, useToast } from "../components/Toast";
import {
  FiPlus,
  FiMinus,
  FiExternalLink,
  FiFolder,
  FiX,
  FiGitBranch,
  FiSave,
  FiRotateCcw,
  FiCamera,
  FiRefreshCw,
  FiTrash2,
  FiDatabase,
  FiUploadCloud,
} from "react-icons/fi";
import type { Host, PhpCatalogEntry, Snapshot, DeployProfile, DeployReport } from "../types";

type Tab = "general" | "apache" | "nginx" | "ssl" | "snapshots" | "deploy";

export function HostsSection() {
  const [hosts, setHosts] = useState<Host[]>([]);
  const [catalog, setCatalog] = useState<PhpCatalogEntry[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh(preferId?: number | null) {
    try {
      const [hostList, cat] = await Promise.all([
        invoke<Host[]>("host_list"),
        invoke<PhpCatalogEntry[]>("php_catalog"),
      ]);
      setHosts(hostList);
      setCatalog(cat);
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
            catalog={catalog}
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
            catalog={catalog}
            onSaved={(updated) => refresh(updated.id)}
          />
        ) : (
          <NoneSelected />
        )}
      </div>
    </div>
  );
}

function NoneSelected() {
  const { t } = useTranslation();
  return (
    <div className="h-full flex items-center justify-center text-sm text-neutral-400">
      {t("hosts.noneSelected")}
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
  const { t } = useTranslation();
  return (
    <div className="w-[320px] shrink-0 border-r border-neutral-200 bg-white flex flex-col">
      <div className="px-4 py-2 grid grid-cols-[1fr_70px_50px] gap-3 text-[11px] uppercase tracking-wider text-neutral-500 border-b border-neutral-200">
        <span>{t("hosts.columns.name")}</span>
        <span>{t("hosts.columns.php")}</span>
        <span>{t("hosts.columns.extras")}</span>
      </div>
      <div className="flex-1 overflow-auto">
        {hosts.length === 0 ? (
          <div className="p-6 text-center text-xs text-neutral-400">
            {t("hosts.empty")}
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
          title={t("hosts.add")}
        >
          <FiPlus />
        </button>
        <button
          onClick={onDelete}
          disabled={selectedId == null}
          className="p-1.5 rounded hover:bg-neutral-100 text-neutral-600 disabled:opacity-30 disabled:cursor-not-allowed"
          title={t("hosts.deleteSelected")}
        >
          <FiMinus />
        </button>
      </div>
    </div>
  );
}

function HostDetail({
  host,
  catalog,
  onSaved,
}: {
  host: Host;
  catalog: PhpCatalogEntry[];
  onSaved: (updated: Host) => void;
}) {
  const { t } = useTranslation();
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
      setInfo(t("hosts.saved"));
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
    { id: "general", label: t("hosts.tab.general"), enabled: true },
    { id: "apache", label: t("hosts.tab.apache"), enabled: true },
    { id: "nginx", label: t("hosts.tab.nginx"), enabled: true },
    { id: "ssl", label: t("hosts.tab.ssl"), enabled: true },
    { id: "snapshots", label: t("hosts.tab.snapshots"), enabled: true },
    { id: "deploy", label: t("hosts.tab.deploy"), enabled: true },
  ];

  return (
    <div className="h-full flex flex-col">
      <div className="border-b border-neutral-200 px-6 flex items-end gap-5">
        {tabs.map((tt) => {
          const isActive = tab === tt.id;
          return (
            <button
              key={tt.id}
              onClick={() => tt.enabled && setTab(tt.id)}
              disabled={!tt.enabled}
              className={`pb-2 pt-3 text-sm border-b-2 transition ${
                isActive
                  ? "border-sky-500 text-sky-600 font-medium"
                  : tt.enabled
                  ? "border-transparent text-neutral-600 hover:text-neutral-900"
                  : "border-transparent text-neutral-300 cursor-not-allowed"
              }`}
              title={tt.enabled ? "" : t("hosts.tabDisabled")}
            >
              {tt.label}
            </button>
          );
        })}
      </div>

      <div className="flex-1 overflow-auto p-6">
        {tab === "general" && (
          <GeneralTab draft={draft} setDraft={setDraft} catalog={catalog} />
        )}
        {tab === "apache" && (
          <ExtrasTab
            label={t("hosts.extrasTab.apacheLabel")}
            placeholder={t("hosts.extrasTab.apachePlaceholder")}
            value={draft.apache_extra}
            onChange={(v) => setDraft({ ...draft, apache_extra: v })}
          />
        )}
        {tab === "nginx" && (
          <ExtrasTab
            label={t("hosts.extrasTab.nginxLabel")}
            placeholder={t("hosts.extrasTab.nginxPlaceholder")}
            value={draft.nginx_extra}
            onChange={(v) => setDraft({ ...draft, nginx_extra: v })}
          />
        )}
        {tab === "ssl" && <SslTab host={host} />}
        {tab === "snapshots" && <SnapshotsTab host={host} />}
        {tab === "deploy" && <DeployTab host={host} />}
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
            {t("hosts.revert")}
          </button>
          <button
            onClick={save}
            disabled={!dirty || busy}
            className="px-4 py-1.5 rounded bg-sky-500 hover:bg-sky-600 text-white text-sm font-medium flex items-center gap-1.5 disabled:opacity-40"
          >
            <FiSave />
            {busy ? t("hosts.saving") : dirty ? t("hosts.save") : t("hosts.savedShort")}
          </button>
        </div>
      </div>
    </div>
  );
}

function GeneralTab({
  draft,
  setDraft,
  catalog,
}: {
  draft: Host;
  setDraft: (h: Host) => void;
  catalog: PhpCatalogEntry[];
}) {
  const { t } = useTranslation();
  return (
    <div className="max-w-2xl space-y-4 text-sm">
      <Field label={t("hosts.general.hostName")}>
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

      <Field label={t("hosts.general.phpVersion")}>
        <select
          value={draft.php_version}
          onChange={(e) => setDraft({ ...draft, php_version: e.target.value })}
          className="w-44 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500 bg-white"
        >
          {catalog.map((e) => (
            <option key={e.version} value={e.version}>
              {e.version}
              {e.installed ? "" : " — download"}
            </option>
          ))}
        </select>
        <span className="text-xs text-neutral-500">
          {t("hosts.general.phpDownloadHint")}
        </span>
      </Field>

      <Field label={t("hosts.general.docroot")}>
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
          {t("hosts.general.open")}
        </button>
      </Field>

      <Field label={t("hosts.general.ports")}>
        <span className="text-neutral-600 font-mono text-xs">
          {t("hosts.general.portsValue")}
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
  const { t } = useTranslation();
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
        {t("hosts.extrasTab.footer")}
      </div>
    </div>
  );
}

function SslTab({ host }: { host: Host }) {
  const { t } = useTranslation();
  return (
    <div className="max-w-2xl space-y-3 text-sm">
      <p className="text-neutral-600">{t("hosts.ssl.intro")}</p>
      <Field label={t("hosts.ssl.certFile")}>
        <code className="font-mono text-xs text-neutral-700 break-all">
          ssl/{host.name}.crt
        </code>
      </Field>
      <Field label={t("hosts.ssl.keyFile")}>
        <code className="font-mono text-xs text-neutral-700 break-all">
          ssl/{host.name}.key
        </code>
      </Field>
      <Field label={t("hosts.ssl.httpsUrl")}>
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
        <strong>Firefox:</strong>{" "}
        {t("hosts.ssl.firefoxNote", {
          pref: "security.enterprise_roots.enabled = true",
          about: "about:config",
          path: "ca/ca.crt",
        })}
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

function SnapshotsTab({ host }: { host: Host }) {
  const { t } = useTranslation();
  const toast = useToast();
  const confirm = useConfirm();
  const [list, setList] = useState<Snapshot[]>([]);
  const [label, setLabel] = useState("");
  const [dbName, setDbName] = useState("");
  const [includeDb, setIncludeDb] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeMysql, setActiveMysql] = useState<string>("");

  async function refresh() {
    try {
      const r = await invoke<Snapshot[]>("snapshot_list", {
        hostId: host.id,
      });
      setList(r);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refresh();
    invoke<string>("mysql_active_version").then(setActiveMysql).catch(() => {});
  }, [host.id]);

  async function create(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await invoke<Snapshot>("snapshot_create", {
        hostId: host.id,
        label: label.trim() || "Untitled snapshot",
        dbName: includeDb && dbName.trim() ? dbName.trim() : null,
      });
      setLabel("");
      setDbName("");
      setIncludeDb(false);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function restore(snap: Snapshot) {
    let warning = snap.has_db
      ? t("hosts.snapshots.confirmRestoreDb")
      : t("hosts.snapshots.confirmRestoreFiles");
    // Cross-version DB restore warning: a 5.7 dump piped into an 8.0 server
    // (or vice-versa) can fail or quietly mangle collations.
    if (
      snap.has_db &&
      snap.mysql_version &&
      activeMysql &&
      snap.mysql_version !== activeMysql
    ) {
      warning += t("hosts.snapshots.versionWarning", {
        snap: snap.mysql_version,
        active: activeMysql,
      });
    }
    const ok = await confirm({
      message: warning,
      confirmLabel: t("hosts.snapshots.restore"),
      tone: "danger",
    });
    if (!ok) return;
    const id = snap.id;
    setBusy(true);
    setError(null);
    try {
      await invoke("snapshot_restore", { id });
      toast("success", t("hosts.snapshots.restored"));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove(id: number) {
    const ok = await confirm({
      message: t("hosts.snapshots.confirmDelete"),
      confirmLabel: t("hosts.snapshots.deleteSnap"),
      tone: "danger",
    });
    if (!ok) return;
    setBusy(true);
    setError(null);
    try {
      await invoke("snapshot_delete", { id });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="max-w-3xl space-y-4 text-sm">
      <form
        onSubmit={create}
        className="rounded-lg border border-neutral-200 bg-neutral-50 p-3 space-y-2"
      >
        <div className="flex items-center gap-2">
          <FiCamera className="text-neutral-500 ml-1 shrink-0" />
          <input
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder={t("hosts.snapshots.labelPlaceholder")}
            className="flex-1 px-3 py-1.5 rounded border border-neutral-300 bg-white text-sm focus:outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-100"
          />
          <button
            type="submit"
            disabled={busy}
            className="px-3 py-1.5 rounded bg-sky-500 hover:bg-sky-600 text-white text-sm font-medium disabled:opacity-50"
          >
            {busy ? "…" : t("hosts.snapshots.take")}
          </button>
        </div>
        <div className="flex items-center gap-2 pl-7">
          <label className="flex items-center gap-1.5 text-xs text-neutral-700">
            <input
              type="checkbox"
              checked={includeDb}
              onChange={(e) => setIncludeDb(e.target.checked)}
              className="size-4"
            />
            <FiDatabase className="text-neutral-500" />
            {t("hosts.snapshots.includeDb")}
          </label>
          {includeDb && (
            <input
              value={dbName}
              onChange={(e) => setDbName(e.target.value)}
              placeholder={t("hosts.snapshots.dbPlaceholder")}
              className="flex-1 px-2 py-1 rounded border border-neutral-300 bg-white text-xs font-mono focus:outline-none focus:border-sky-500"
            />
          )}
        </div>
      </form>

      <p className="text-xs text-neutral-500">
        {t("hosts.snapshots.intro", { docroot: host.docroot })}
      </p>

      {list.length === 0 ? (
        <div className="rounded-lg border border-dashed border-neutral-300 p-6 text-center text-neutral-500 text-sm">
          {t("hosts.snapshots.noSnapshots")}
        </div>
      ) : (
        <div className="rounded-lg border border-neutral-200 bg-white overflow-hidden divide-y divide-neutral-100">
          {list.map((s) => (
            <div
              key={s.id}
              className="px-4 py-3 flex items-center gap-3 text-sm"
            >
              <FiCamera className="text-neutral-400 shrink-0" />
              <div className="flex-1 min-w-0">
                <div className="font-medium text-neutral-900 truncate flex items-center gap-2">
                  {s.label}
                  {s.has_db && (
                    <span
                      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-sky-100 text-sky-700"
                      title={t("hosts.snapshots.withDb")}
                    >
                      <FiDatabase className="text-[10px]" />
                      DB
                    </span>
                  )}
                </div>
                <div className="text-[11px] text-neutral-500 font-mono">
                  {s.created_at} · {formatSize(s.size_bytes)}
                </div>
              </div>
              <button
                onClick={() => restore(s)}
                disabled={busy}
                className="px-2.5 py-1 rounded border border-neutral-300 hover:bg-neutral-50 text-xs flex items-center gap-1.5 text-neutral-700 disabled:opacity-50"
              >
                <FiRefreshCw />
                {t("hosts.snapshots.restore")}
              </button>
              <button
                onClick={() => remove(s.id)}
                disabled={busy}
                className="p-1.5 rounded text-red-500 hover:bg-red-50 disabled:opacity-50"
                title={t("hosts.snapshots.deleteSnap")}
              >
                <FiTrash2 />
              </button>
            </div>
          ))}
        </div>
      )}

      {error && (
        <div className="text-xs text-red-600 font-mono break-words bg-red-50 border border-red-200 rounded p-2">
          {error}
        </div>
      )}
    </div>
  );
}

function DeployTab({ host }: { host: Host }) {
  const { t } = useTranslation();
  const toast = useToast();
  const [profile, setProfile] = useState<DeployProfile>({
    host_id: host.id,
    protocol: "ftp",
    ftp_host: "",
    ftp_port: 21,
    ftp_user: "",
    ftp_password: "",
    remote_dir: "/",
  });
  const [busy, setBusy] = useState(false);
  const [deploying, setDeploying] = useState(false);
  const [report, setReport] = useState<DeployReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<DeployProfile | null>("deploy_profile_get", { hostId: host.id })
      .then((p) => {
        if (p) setProfile(p);
      })
      .catch(() => {});
  }, [host.id]);

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await invoke("deploy_profile_save", {
        profile: { ...profile, host_id: host.id },
      });
      toast("success", t("hosts.deploy.saved"));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deploy() {
    setDeploying(true);
    setError(null);
    setReport(null);
    try {
      await invoke("deploy_profile_save", {
        profile: { ...profile, host_id: host.id },
      });
      const r = await invoke<DeployReport>("ftp_upload", {
        host: profile.ftp_host,
        port: profile.ftp_port,
        user: profile.ftp_user,
        password: profile.ftp_password,
        remoteDir: profile.remote_dir,
        localDir: host.docroot,
        protocol: profile.protocol,
      });
      setReport(r);
      if (r.errors.length === 0) {
        toast(
          "success",
          t("hosts.deploy.result", {
            files: r.files_uploaded,
            bytes: formatSize(r.bytes_uploaded),
          })
        );
      } else {
        toast(
          "error",
          t("hosts.deploy.withErrors", {
            files: r.files_uploaded,
            errors: r.errors.length,
          })
        );
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setDeploying(false);
    }
  }

  const canDeploy =
    !deploying && profile.ftp_host.trim() && profile.ftp_user.trim();

  return (
    <div className="max-w-2xl space-y-4 text-sm">
      <p className="text-xs text-neutral-500">{t("hosts.deploy.intro")}</p>

      <Field label={t("hosts.deploy.protocol")}>
        <select
          value={profile.protocol}
          onChange={(e) => setProfile({ ...profile, protocol: e.target.value })}
          className="px-3 py-1.5 rounded border border-neutral-300 bg-white focus:outline-none focus:border-sky-500"
        >
          <option value="ftp">FTP</option>
          <option value="ftps">FTPS</option>
        </select>
        <span className="text-xs text-neutral-500">{t("hosts.deploy.ftpsNote")}</span>
      </Field>

      <Field label={t("hosts.deploy.host")}>
        <input
          value={profile.ftp_host}
          onChange={(e) => setProfile({ ...profile, ftp_host: e.target.value })}
          placeholder={t("hosts.deploy.hostPlaceholder")}
          className="flex-1 min-w-0 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500"
        />
      </Field>

      <Field label={t("hosts.deploy.port")}>
        <input
          type="number"
          value={profile.ftp_port}
          onChange={(e) =>
            setProfile({ ...profile, ftp_port: Number(e.target.value) || 21 })
          }
          className="w-28 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500"
        />
      </Field>

      <Field label={t("hosts.deploy.user")}>
        <input
          value={profile.ftp_user}
          onChange={(e) => setProfile({ ...profile, ftp_user: e.target.value })}
          className="w-64 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500"
        />
      </Field>

      <Field label={t("hosts.deploy.password")}>
        <input
          type="password"
          value={profile.ftp_password}
          onChange={(e) =>
            setProfile({ ...profile, ftp_password: e.target.value })
          }
          className="w-64 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500"
        />
      </Field>

      <Field label={t("hosts.deploy.remoteDir")}>
        <input
          value={profile.remote_dir}
          onChange={(e) =>
            setProfile({ ...profile, remote_dir: e.target.value })
          }
          placeholder={t("hosts.deploy.remoteDirPlaceholder")}
          className="flex-1 min-w-0 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500"
        />
      </Field>

      <div className="flex items-center gap-2 pt-1">
        <button
          onClick={save}
          disabled={busy}
          className="px-3 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm flex items-center gap-1.5 disabled:opacity-50"
        >
          <FiSave />
          {t("hosts.deploy.save")}
        </button>
        <button
          onClick={deploy}
          disabled={!canDeploy}
          className="px-4 py-1.5 rounded bg-sky-500 hover:bg-sky-600 text-white text-sm font-medium flex items-center gap-1.5 disabled:opacity-50"
        >
          <FiUploadCloud />
          {deploying ? t("hosts.deploy.deploying") : t("hosts.deploy.deployNow")}
        </button>
      </div>

      {report && report.errors.length > 0 && (
        <div className="text-xs text-amber-800 bg-amber-50 border border-amber-200 rounded p-2 font-mono max-h-40 overflow-auto">
          {report.errors.map((e, i) => (
            <div key={i}>{e}</div>
          ))}
        </div>
      )}
      {error && (
        <div className="text-xs text-red-600 font-mono break-words bg-red-50 border border-red-200 rounded p-2">
          {error}
        </div>
      )}
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function AddHostForm({
  catalog,
  onCancel,
  onCreated,
  error,
  setError,
}: {
  catalog: PhpCatalogEntry[];
  onCancel: () => void;
  onCreated: (id: number) => void;
  error: string | null;
  setError: (s: string | null) => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [docroot, setDocroot] = useState("");
  // Default to the latest INSTALLED version. If everything is missing
  // (unusual — installer ships 8.4) fall back to the highest in the catalog.
  const installed = catalog.filter((e) => e.installed);
  const fallback =
    installed[installed.length - 1] ?? catalog[catalog.length - 1];
  const [phpVersion, setPhpVersion] = useState(fallback?.version ?? "");
  const [initGit, setInitGit] = useState(false);
  const [gitAvailable, setGitAvailable] = useState(false);
  const [busy, setBusy] = useState(false);
  const [stage, setStage] = useState<"idle" | "downloading">("idle");

  useEffect(() => {
    invoke<boolean>("git_available").then(setGitAvailable);
  }, []);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      // Auto-download the picked PHP version if it isn't on disk yet. The
      // bundled php_install command grabs both `php-X.Y` and `xdebug-X.Y`.
      const picked = catalog.find((e) => e.version === phpVersion);
      if (picked && !picked.installed) {
        setStage("downloading");
        await invoke("php_install", { version: phpVersion });
      }

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
      setStage("idle");
    }
  }

  return (
    <div className="h-full flex flex-col">
      <div className="border-b border-neutral-200 px-6 py-3 flex items-center justify-between">
        <h2 className="text-sm font-semibold text-neutral-800">{t("hosts.addForm.title")}</h2>
        <button
          onClick={onCancel}
          className="p-1 rounded hover:bg-neutral-100 text-neutral-500"
        >
          <FiX />
        </button>
      </div>
      <form onSubmit={submit} className="flex-1 overflow-auto p-6 space-y-4 text-sm">
        <div className="max-w-2xl space-y-4">
          <Field label={t("hosts.general.hostName")}>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("hosts.addForm.hostnamePlaceholder")}
              autoFocus
              className="w-72 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-100"
            />
          </Field>

          <Field label={t("hosts.general.phpVersion")}>
            <select
              value={phpVersion}
              onChange={(e) => setPhpVersion(e.target.value)}
              className="w-44 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-100 bg-white"
            >
              {catalog.map((e) => (
                <option key={e.version} value={e.version}>
                  {e.version}
                  {e.installed ? "" : " — download"}
                </option>
              ))}
            </select>
            <span className="text-xs text-neutral-500">
              {t("hosts.general.phpDownloadHint")}
            </span>
          </Field>

          <Field label={t("hosts.general.docroot")}>
            <input
              value={docroot}
              onChange={(e) => setDocroot(e.target.value)}
              placeholder={t("hosts.addForm.docrootPlaceholder")}
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
                <span>{t("hosts.addForm.gitInit")}</span>
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
            {stage === "downloading"
              ? t("hosts.addForm.phpDownloadOnSave", { version: phpVersion })
              : busy
              ? t("hosts.saving")
              : t("hosts.save")}
          </button>
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-2 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50"
          >
            {t("hosts.addForm.cancel")}
          </button>
        </div>
      </form>
    </div>
  );
}
