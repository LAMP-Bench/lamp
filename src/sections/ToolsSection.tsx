import { FormEvent, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { IconType } from "react-icons";
import { FiExternalLink, FiPackage, FiX, FiEdit3 } from "react-icons/fi";
import {
  SiPhpmyadmin,
  SiLaravel,
  SiComposer,
  SiPhp,
  SiWordpress,
  SiJoomla,
  SiDrupal,
  SiWikipedia,
} from "react-icons/si";

type CommandResult = {
  success: boolean;
  stdout: string;
  stderr: string;
  exit_code: number;
};

export function ToolsSection({
  openInEditor,
}: {
  openInEditor: (path: string) => void;
}) {
  return (
    <div className="p-6 space-y-6 max-w-3xl">
      <Section title="Database">
        <PhpMyAdminCard />
      </Section>

      <Section title="PHP">
        <PhpIniCard openInEditor={openInEditor} />
        <ComposerCard />
        <LaravelCard />
      </Section>

      <Section title="CMS Extras">
        <CmsCard
          icon={SiWordpress}
          iconColor="text-blue-600"
          title="WordPress"
          subtitle="One-click install. Creates DB, generates wp-config.php with fresh salts, registers host."
          command="wordpress_install"
        />
        <CmsCard
          icon={SiJoomla}
          iconColor="text-sky-600"
          title="Joomla"
          subtitle="One-click install. Creates DB + host. Complete setup in the web installer."
          command="joomla_install"
        />
        <CmsCard
          icon={SiDrupal}
          iconColor="text-blue-500"
          title="Drupal"
          subtitle="One-click install. Drupal 11. DB + host registered. Finish in the web installer."
          command="drupal_install"
        />
        <CmsCard
          icon={SiWikipedia}
          iconColor="text-amber-600"
          title="MediaWiki"
          subtitle="One-click install. DB + host registered. Visit the URL to run mw-config/."
          command="mediawiki_install"
        />
      </Section>
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-2">
      <h2 className="text-[11px] uppercase tracking-wider text-neutral-500 font-medium">
        {title}
      </h2>
      <div className="space-y-2">{children}</div>
    </section>
  );
}

function PhpMyAdminCard() {
  return (
    <ToolCard
      icon={SiPhpmyadmin}
      iconColor="text-amber-500"
      title="phpMyAdmin"
      subtitle="Database admin — requires apache + mysql running"
      action={
        <button
          onClick={() => openUrl("http://localhost:8080/phpmyadmin/")}
          className="px-3 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm flex items-center gap-1.5"
        >
          <FiExternalLink />
          Open
        </button>
      }
    />
  );
}

function PhpIniCard({
  openInEditor,
}: {
  openInEditor: (path: string) => void;
}) {
  const [versions, setVersions] = useState<string[]>([]);
  useEffect(() => {
    invoke<string[]>("php_versions").then(setVersions);
  }, []);

  return (
    <div className="rounded-lg border border-neutral-200 bg-white p-4 shadow-sm">
      <div className="flex items-center gap-4">
        <div className="size-11 rounded-md bg-neutral-100 flex items-center justify-center shrink-0">
          <SiPhp className="text-2xl text-indigo-500" />
        </div>
        <div className="flex-1 min-w-0">
          <div className="font-medium text-neutral-900">php.ini</div>
          <div className="text-xs text-neutral-500">
            Per-version PHP config. Apache must restart to pick up changes.
          </div>
        </div>
      </div>
      <div className="mt-3 flex flex-wrap gap-2 ml-[60px]">
        {versions.map((v) => {
          const iniPath = `${repoBase()}/resources/php-${v}/php.ini`;
          return (
            <button
              key={v}
              onClick={() => openInEditor(iniPath)}
              className="px-3 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm flex items-center gap-1.5 font-mono"
            >
              <FiEdit3 />
              php-{v}/php.ini
            </button>
          );
        })}
      </div>
    </div>
  );
}

function repoBase() {
  return "C:/Users/anthropic/Desktop/a/lamp";
}

function ComposerCard() {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    invoke<CommandResult>("composer_version")
      .then((r) =>
        setVersion(
          r.success
            ? r.stdout.split("\n")[0].trim()
            : `error: ${r.stderr.trim()}`
        )
      )
      .catch((e) => setVersion(`error: ${e}`));
  }, []);

  return (
    <ToolCard
      icon={SiComposer}
      iconColor="text-amber-700"
      title="Composer"
      subtitle={version ?? "checking…"}
      action={null}
    />
  );
}

function LaravelCard() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <ToolCard
        icon={SiLaravel}
        iconColor="text-red-500"
        title="Laravel"
        subtitle="Scaffold a new Laravel app with composer create-project"
        action={
          <button
            onClick={() => setOpen(true)}
            className="px-3 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm flex items-center gap-1.5"
          >
            <FiPackage />
            New project
          </button>
        }
      />
      {open && <LaravelDialog onClose={() => setOpen(false)} />}
    </>
  );
}

function CmsCard({
  icon,
  iconColor,
  title,
  subtitle,
  command,
}: {
  icon: IconType;
  iconColor: string;
  title: string;
  subtitle: string;
  command: string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <ToolCard
        icon={icon}
        iconColor={iconColor}
        title={title}
        subtitle={subtitle}
        action={
          <button
            onClick={() => setOpen(true)}
            className="px-3 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm flex items-center gap-1.5"
          >
            <FiPackage />
            New site
          </button>
        }
      />
      {open && (
        <CmsInstallDialog
          title={`Install ${title}`}
          icon={icon}
          iconColor={iconColor}
          command={command}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}

function CmsInstallDialog({
  title,
  icon: Icon,
  iconColor,
  command,
  onClose,
}: {
  title: string;
  icon: IconType;
  iconColor: string;
  command: string;
  onClose: () => void;
}) {
  const [siteName, setSiteName] = useState("");
  const [hostname, setHostname] = useState("");
  const [parentDir, setParentDir] = useState("");
  const [phpVersions, setPhpVersions] = useState<string[]>([]);
  const [phpVersion, setPhpVersion] = useState("");
  const [hostTouched, setHostTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [createdUrl, setCreatedUrl] = useState<string | null>(null);

  useEffect(() => {
    invoke<string[]>("php_versions").then((v) => {
      setPhpVersions(v);
      setPhpVersion(v[v.length - 1] ?? "");
    });
  }, []);

  useEffect(() => {
    if (!hostTouched && siteName) {
      const slug = siteName
        .toLowerCase()
        .replace(/[^a-z0-9-]+/g, "-")
        .replace(/^-+|-+$/g, "");
      setHostname(slug ? `${slug}.local` : "");
    }
  }, [siteName, hostTouched]);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await invoke<string>(command, {
        siteName: siteName.trim(),
        hostname: hostname.trim(),
        parentDir: parentDir.trim(),
        phpVersion,
      });
      setCreatedUrl(`http://${hostname.trim()}:8080/`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 bg-neutral-900/40 flex items-center justify-center z-50 p-4">
      <div className="bg-white rounded-lg shadow-xl w-full max-w-lg">
        <div className="px-5 py-3 border-b border-neutral-200 flex items-center justify-between">
          <h3 className="font-semibold text-neutral-900 flex items-center gap-2">
            <Icon className={iconColor} />
            {title}
          </h3>
          <button
            onClick={onClose}
            className="p-1 rounded hover:bg-neutral-100 text-neutral-500"
          >
            <FiX />
          </button>
        </div>

        {createdUrl ? (
          <div className="p-5 space-y-3 text-sm">
            <p className="text-emerald-700">
              Installed and host registered. Open the URL to finish the setup
              wizard:
            </p>
            <a
              href="#"
              onClick={(e) => {
                e.preventDefault();
                openUrl(createdUrl);
              }}
              className="block rounded bg-neutral-100 p-3 font-mono text-xs break-all text-sky-700 hover:underline"
            >
              {createdUrl}
            </a>
            <div className="flex justify-end gap-2 pt-2">
              <button
                onClick={() => openUrl(createdUrl)}
                className="px-4 py-1.5 rounded bg-sky-500 hover:bg-sky-600 text-white text-sm font-medium flex items-center gap-1.5"
              >
                <FiExternalLink />
                Open site
              </button>
              <button
                onClick={onClose}
                className="px-4 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm"
              >
                Done
              </button>
            </div>
          </div>
        ) : (
          <form onSubmit={submit} className="p-5 space-y-3 text-sm">
            <Field label="Site name">
              <input
                value={siteName}
                onChange={(e) => setSiteName(e.target.value)}
                placeholder="my-site"
                autoFocus
                className="flex-1 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500"
              />
            </Field>
            <Field label="Hostname">
              <input
                value={hostname}
                onChange={(e) => {
                  setHostname(e.target.value);
                  setHostTouched(true);
                }}
                placeholder="my-site.local"
                className="flex-1 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500"
              />
            </Field>
            <Field label="Parent directory">
              <input
                value={parentDir}
                onChange={(e) => setParentDir(e.target.value)}
                placeholder="C:/Users/me/projects"
                className="flex-1 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500"
              />
            </Field>
            <Field label="PHP version">
              <select
                value={phpVersion}
                onChange={(e) => setPhpVersion(e.target.value)}
                className="px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500 bg-white"
              >
                {phpVersions.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
            </Field>

            {error && (
              <div className="text-xs text-red-600 font-mono break-words bg-red-50 border border-red-200 rounded p-2">
                {error}
              </div>
            )}

            <div className="text-xs text-neutral-500">
              {busy
                ? "Copying files, creating DB, updating hosts file (UAC)…"
                : "MySQL must be running. Triggers a UAC prompt to register the hostname."}
            </div>

            <div className="flex items-center gap-2 pt-2">
              <button
                type="submit"
                disabled={
                  busy ||
                  !siteName.trim() ||
                  !hostname.trim() ||
                  !parentDir.trim() ||
                  !phpVersion
                }
                className="px-4 py-1.5 rounded bg-sky-500 hover:bg-sky-600 text-white font-medium disabled:opacity-50"
              >
                {busy ? "Installing…" : "Install"}
              </button>
              <button
                type="button"
                onClick={onClose}
                disabled={busy}
                className="px-4 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50"
              >
                Cancel
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}

function LaravelDialog({ onClose }: { onClose: () => void }) {
  const [name, setName] = useState("");
  const [parentDir, setParentDir] = useState("");
  const [phpVersions, setPhpVersions] = useState<string[]>([]);
  const [phpVersion, setPhpVersion] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [createdPath, setCreatedPath] = useState<string | null>(null);

  useEffect(() => {
    invoke<string[]>("php_versions").then((v) => {
      setPhpVersions(v);
      setPhpVersion(v[v.length - 1] ?? "");
    });
  }, []);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const publicDir = await invoke<string>("laravel_create", {
        name: name.trim(),
        parentDir: parentDir.trim(),
        phpVersion,
      });
      setCreatedPath(publicDir);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 bg-neutral-900/40 flex items-center justify-center z-50 p-4">
      <div className="bg-white rounded-lg shadow-xl w-full max-w-lg">
        <div className="px-5 py-3 border-b border-neutral-200 flex items-center justify-between">
          <h3 className="font-semibold text-neutral-900">
            Create Laravel project
          </h3>
          <button
            onClick={onClose}
            className="p-1 rounded hover:bg-neutral-100 text-neutral-500"
          >
            <FiX />
          </button>
        </div>

        {createdPath ? (
          <div className="p-5 space-y-3 text-sm">
            <p className="text-emerald-700">
              Project created. The Laravel public dir is below — add it as a
              host in the Hosts panel:
            </p>
            <pre className="rounded bg-neutral-100 p-3 font-mono text-xs break-words whitespace-pre-wrap">
              {createdPath}
            </pre>
            <div className="flex justify-end">
              <button
                onClick={onClose}
                className="px-4 py-1.5 rounded bg-sky-500 hover:bg-sky-600 text-white text-sm font-medium"
              >
                Done
              </button>
            </div>
          </div>
        ) : (
          <form onSubmit={submit} className="p-5 space-y-3 text-sm">
            <Field label="Project name">
              <input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="my-laravel-app"
                autoFocus
                className="flex-1 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500"
              />
            </Field>
            <Field label="Parent directory">
              <input
                value={parentDir}
                onChange={(e) => setParentDir(e.target.value)}
                placeholder="C:/Users/me/projects"
                className="flex-1 px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500"
              />
            </Field>
            <Field label="PHP version">
              <select
                value={phpVersion}
                onChange={(e) => setPhpVersion(e.target.value)}
                className="px-3 py-1.5 rounded border border-neutral-300 font-mono focus:outline-none focus:border-sky-500 bg-white"
              >
                {phpVersions.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
            </Field>

            {error && (
              <div className="text-xs text-red-600 font-mono break-words bg-red-50 border border-red-200 rounded p-2">
                {error}
              </div>
            )}

            <div className="text-xs text-neutral-500">
              {busy
                ? "Running composer create-project — this can take 1-3 min on first download."
                : "Composer will download Laravel and its dependencies. Be patient."}
            </div>

            <div className="flex items-center gap-2 pt-2">
              <button
                type="submit"
                disabled={
                  busy || !name.trim() || !parentDir.trim() || !phpVersion
                }
                className="px-4 py-1.5 rounded bg-sky-500 hover:bg-sky-600 text-white font-medium disabled:opacity-50"
              >
                {busy ? "Creating…" : "Create"}
              </button>
              <button
                type="button"
                onClick={onClose}
                disabled={busy}
                className="px-4 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50"
              >
                Cancel
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}

function ToolCard({
  icon: Icon,
  iconColor,
  title,
  subtitle,
  action,
}: {
  icon: IconType;
  iconColor: string;
  title: string;
  subtitle: string;
  action: React.ReactNode;
}) {
  return (
    <div className="rounded-lg border border-neutral-200 bg-white p-4 flex items-center gap-4 shadow-sm">
      <div className="size-11 rounded-md bg-neutral-100 flex items-center justify-center shrink-0">
        <Icon className={`text-2xl ${iconColor}`} />
      </div>
      <div className="flex-1 min-w-0">
        <div className="font-medium text-neutral-900">{title}</div>
        <div className="text-xs text-neutral-500 truncate">{subtitle}</div>
      </div>
      {action}
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
    <div className="grid grid-cols-[120px_1fr] gap-3 items-center">
      <label className="text-neutral-600 text-right">{label}:</label>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  );
}
