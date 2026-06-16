import { FormEvent, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { IconType } from "react-icons";
import { FiExternalLink, FiPackage, FiX, FiMail, FiImage, FiUploadCloud } from "react-icons/fi";
import {
  SiPhpmyadmin,
  SiLaravel,
  SiComposer,
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

export function ToolsSection() {
  return (
    <div className="h-full overflow-y-auto">
      <div className="p-6 space-y-6 max-w-3xl">
        <Section title="Database">
          <PhpMyAdminCard />
        </Section>

        <Section title="Email">
          <MailHogCard />
        </Section>

        <Section title="Images">
          <ImageOptimizerCard />
        </Section>

        <Section title="Deploy">
          <FtpDeployCard />
        </Section>

        <Section title="PHP">
          <ComposerCard />
          <LaravelCard />
        </Section>

        <Section title="CMS Extras">
          <CmsCard
            icon={SiWordpress}
            iconColor="text-blue-600"
            title="WordPress"
            subtitle="One-click install. DB, wp-config.php with fresh salts, files copied to htdocs."
            command="wordpress_install"
            binaryName="wordpress"
          />
          <CmsCard
            icon={SiJoomla}
            iconColor="text-sky-600"
            title="Joomla"
            subtitle="One-click install. DB created. Finish setup in the web installer."
            command="joomla_install"
            binaryName="joomla"
          />
          <CmsCard
            icon={SiDrupal}
            iconColor="text-blue-500"
            title="Drupal"
            subtitle="Drupal 11. DB created. Finish in the web installer."
            command="drupal_install"
            binaryName="drupal"
          />
          <CmsCard
            icon={SiWikipedia}
            iconColor="text-amber-600"
            title="MediaWiki"
            subtitle="DB created. Visit /mw-config/ to finish setup."
            command="mediawiki_install"
            binaryName="mediawiki"
          />
        </Section>
      </div>
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

function MailHogCard() {
  return (
    <ToolCard
      icon={FiMail}
      iconColor="text-amber-500"
      title="MailHog inbox"
      subtitle="Catches PHP mail() and shows them in a web UI. Start MailHog first."
      action={
        <button
          onClick={() => openUrl("http://localhost:8025/")}
          className="px-3 py-1.5 rounded border border-neutral-300 text-neutral-700 hover:bg-neutral-50 text-sm flex items-center gap-1.5"
        >
          <FiExternalLink />
          Open
        </button>
      }
    />
  );
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
  binaryName,
}: {
  icon: IconType;
  iconColor: string;
  title: string;
  subtitle: string;
  command: string;
  binaryName: string;
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
          binaryName={binaryName}
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
  binaryName,
  onClose,
}: {
  title: string;
  icon: IconType;
  iconColor: string;
  command: string;
  binaryName: string;
  onClose: () => void;
}) {
  const [siteName, setSiteName] = useState("");
  const [parentDir, setParentDir] = useState("");
  const [addHost, setAddHost] = useState(false);
  const [hostname, setHostname] = useState("");
  const [hostTouched, setHostTouched] = useState(false);
  const [phpVersions, setPhpVersions] = useState<string[]>([]);
  const [phpVersion, setPhpVersion] = useState("");
  const [busy, setBusy] = useState(false);
  const [stage, setStage] = useState<"idle" | "downloading" | "installing">(
    "idle"
  );
  const [error, setError] = useState<string | null>(null);
  const [createdUrl, setCreatedUrl] = useState<string | null>(null);

  useEffect(() => {
    invoke<string[]>("php_versions").then((v) => {
      setPhpVersions(v);
      setPhpVersion(v[v.length - 1] ?? "");
    });
    // Default the install location to the user-facing htdocs. The user only
    // overrides this if they want the project elsewhere.
    invoke<string>("htdocs_path").then(setParentDir).catch(() => {});
  }, []);

  useEffect(() => {
    if (addHost && !hostTouched && siteName) {
      const slug = siteName
        .toLowerCase()
        .replace(/[^a-z0-9-]+/g, "-")
        .replace(/^-+|-+$/g, "");
      setHostname(slug ? `${slug}.local` : "");
    }
  }, [addHost, siteName, hostTouched]);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      // Auto-download the CMS source if it's an on-demand binary that hasn't
      // been fetched yet. The runtime download is sync (blocks until done)
      // so the UI just shows a "Downloading…" message.
      const installed = await invoke<boolean>("binary_installed", {
        name: binaryName,
      });
      if (!installed) {
        setStage("downloading");
        await invoke("binary_download", { name: binaryName });
      }

      setStage("installing");
      await invoke<string>(command, {
        siteName: siteName.trim(),
        hostname: addHost ? hostname.trim() : "",
        parentDir: parentDir.trim(),
        phpVersion,
      });
      const url =
        addHost && hostname.trim()
          ? `http://${hostname.trim()}:8080/`
          : `http://localhost:8080/${siteName.trim()}/`;
      setCreatedUrl(url);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setStage("idle");
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
              Installed. Open the URL to finish the setup wizard:
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
            <Field label="Install in">
              <input
                value={parentDir}
                onChange={(e) => setParentDir(e.target.value)}
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

            <div className="grid grid-cols-[120px_1fr] gap-3 items-start pt-1">
              <div />
              <label className="flex items-start gap-2 text-neutral-700 cursor-pointer">
                <input
                  type="checkbox"
                  checked={addHost}
                  onChange={(e) => setAddHost(e.target.checked)}
                  className="mt-0.5"
                />
                <span>
                  Also register as a virtual host
                  <span className="block text-[11px] text-neutral-500">
                    Custom hostname like{" "}
                    <code>my-site.local</code> — triggers a UAC prompt to
                    update the hosts file. Skip to access it as a path under
                    <code> localhost:8080</code>.
                  </span>
                </span>
              </label>
            </div>

            {addHost && (
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
            )}

            {error && (
              <div className="text-xs text-red-600 font-mono break-words bg-red-50 border border-red-200 rounded p-2">
                {error}
              </div>
            )}

            <div className="text-xs text-neutral-500">
              {stage === "downloading" &&
                "Downloading CMS files… (first install only, can take a minute)"}
              {stage === "installing" && "Copying files, creating DB…"}
              {stage === "idle" && "MySQL must be running."}
            </div>

            <div className="flex items-center gap-2 pt-2">
              <button
                type="submit"
                disabled={
                  busy ||
                  !siteName.trim() ||
                  !parentDir.trim() ||
                  !phpVersion ||
                  (addHost && !hostname.trim())
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
    invoke<string>("htdocs_path").then(setParentDir).catch(() => {});
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
              Project created. Public dir below — add it as a host in Hosts:
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
                : "Composer will download Laravel and its dependencies."}
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

type CompressReport = {
  files_total: number;
  files_changed: number;
  bytes_before: number;
  bytes_after: number;
  errors: string[];
};

function ImageOptimizerCard() {
  const [folder, setFolder] = useState("");
  const [quality, setQuality] = useState(80);
  const [includeJpg, setIncludeJpg] = useState(true);
  const [includePng, setIncludePng] = useState(true);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<CompressReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run() {
    setBusy(true);
    setError(null);
    setReport(null);
    try {
      const r = await invoke<CompressReport>("compress_images", {
        folder: folder.trim(),
        jpegQuality: quality,
        includePng,
        includeJpg,
      });
      setReport(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const saved = report ? report.bytes_before - report.bytes_after : 0;
  const savedPct =
    report && report.bytes_before > 0
      ? Math.round((saved / report.bytes_before) * 100)
      : 0;

  return (
    <div className="rounded-lg border border-neutral-200 bg-white p-4">
      <div className="flex items-center gap-3 mb-3">
        <FiImage className="text-emerald-600 text-xl" />
        <h3 className="font-medium text-neutral-800">Image optimizer</h3>
      </div>
      <p className="text-xs text-neutral-500 mb-3">
        Walk a folder, re-encode JPGs at the chosen quality and run oxipng on
        PNGs. Files are only replaced if the new version is smaller — safe to
        run repeatedly on the same folder.
      </p>
      <div className="space-y-2 text-sm">
        <Field label="Folder">
          <input
            value={folder}
            onChange={(e) => setFolder(e.target.value)}
            placeholder="C:/path/to/images"
            className="flex-1 min-w-0 px-3 py-1.5 rounded border border-neutral-300 font-mono text-xs focus:outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-100"
          />
        </Field>
        <Field label={`JPG quality (${quality})`}>
          <input
            type="range"
            min={40}
            max={100}
            value={quality}
            onChange={(e) => setQuality(Number(e.target.value))}
            className="w-48 accent-sky-500"
          />
        </Field>
        <Field label="">
          <label className="flex items-center gap-1.5 text-xs text-neutral-700">
            <input
              type="checkbox"
              checked={includeJpg}
              onChange={(e) => setIncludeJpg(e.target.checked)}
              className="size-4"
            />
            JPG
          </label>
          <label className="flex items-center gap-1.5 text-xs text-neutral-700">
            <input
              type="checkbox"
              checked={includePng}
              onChange={(e) => setIncludePng(e.target.checked)}
              className="size-4"
            />
            PNG
          </label>
        </Field>
      </div>
      <div className="mt-3 flex items-center gap-2">
        <button
          onClick={run}
          disabled={busy || !folder.trim() || (!includeJpg && !includePng)}
          className="px-3 py-1.5 rounded bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-medium disabled:opacity-50"
        >
          {busy ? "Optimizing…" : "Optimize"}
        </button>
        {report && (
          <span className="text-xs text-neutral-700">
            {report.files_changed}/{report.files_total} shrunk · saved{" "}
            <strong className="text-emerald-700">
              {formatBytes(saved)} ({savedPct}%)
            </strong>
          </span>
        )}
      </div>
      {report && report.errors.length > 0 && (
        <div className="mt-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded p-2 max-h-32 overflow-auto">
          <strong>{report.errors.length} error(s):</strong>
          <ul className="font-mono mt-1 space-y-0.5">
            {report.errors.slice(0, 20).map((e, i) => (
              <li key={i} className="truncate">
                {e}
              </li>
            ))}
            {report.errors.length > 20 && (
              <li>… {report.errors.length - 20} more</li>
            )}
          </ul>
        </div>
      )}
      {error && (
        <div className="mt-2 text-xs text-red-600 font-mono break-words bg-red-50 border border-red-200 rounded p-2">
          {error}
        </div>
      )}
    </div>
  );
}

function formatBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  if (b < 1024 * 1024 * 1024) return `${(b / (1024 * 1024)).toFixed(1)} MB`;
  return `${(b / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

type DeployReport = {
  files_uploaded: number;
  bytes_uploaded: number;
  errors: string[];
};

function FtpDeployCard() {
  const [host, setHost] = useState("");
  const [port, setPort] = useState(21);
  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [remoteDir, setRemoteDir] = useState("/public_html");
  const [localDir, setLocalDir] = useState("");
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<DeployReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run() {
    setBusy(true);
    setError(null);
    setReport(null);
    try {
      const r = await invoke<DeployReport>("ftp_upload", {
        host: host.trim(),
        port,
        user: user.trim(),
        password,
        remoteDir: remoteDir.trim(),
        localDir: localDir.trim(),
      });
      setReport(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="rounded-lg border border-neutral-200 bg-white p-4">
      <div className="flex items-center gap-3 mb-3">
        <FiUploadCloud className="text-violet-600 text-xl" />
        <h3 className="font-medium text-neutral-800">FTP upload</h3>
      </div>
      <p className="text-xs text-neutral-500 mb-3">
        Recursive upload of a local folder to an FTP server in binary mode.
        Plain FTP only for now — SFTP is queued for the next round. Stored
        profiles per host will come once this is exercised in anger.
      </p>
      <div className="grid grid-cols-[120px_1fr_80px] gap-2 items-center text-sm">
        <label className="text-right text-neutral-600">Host</label>
        <input
          value={host}
          onChange={(e) => setHost(e.target.value)}
          placeholder="ftp.example.com"
          className="px-2 py-1.5 rounded border border-neutral-300 font-mono text-xs focus:outline-none focus:border-sky-500"
        />
        <input
          type="number"
          value={port}
          onChange={(e) => setPort(Number(e.target.value) || 21)}
          className="px-2 py-1.5 rounded border border-neutral-300 font-mono text-xs focus:outline-none focus:border-sky-500"
        />

        <label className="text-right text-neutral-600">User</label>
        <input
          value={user}
          onChange={(e) => setUser(e.target.value)}
          className="px-2 py-1.5 rounded border border-neutral-300 font-mono text-xs focus:outline-none focus:border-sky-500 col-span-2"
        />

        <label className="text-right text-neutral-600">Password</label>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          className="px-2 py-1.5 rounded border border-neutral-300 font-mono text-xs focus:outline-none focus:border-sky-500 col-span-2"
        />

        <label className="text-right text-neutral-600">Remote dir</label>
        <input
          value={remoteDir}
          onChange={(e) => setRemoteDir(e.target.value)}
          placeholder="/public_html"
          className="px-2 py-1.5 rounded border border-neutral-300 font-mono text-xs focus:outline-none focus:border-sky-500 col-span-2"
        />

        <label className="text-right text-neutral-600">Local folder</label>
        <input
          value={localDir}
          onChange={(e) => setLocalDir(e.target.value)}
          placeholder="C:/path/to/project"
          className="px-2 py-1.5 rounded border border-neutral-300 font-mono text-xs focus:outline-none focus:border-sky-500 col-span-2"
        />
      </div>
      <div className="mt-3 flex items-center gap-2">
        <button
          onClick={run}
          disabled={
            busy ||
            !host.trim() ||
            !user.trim() ||
            !remoteDir.trim() ||
            !localDir.trim()
          }
          className="px-3 py-1.5 rounded bg-violet-600 hover:bg-violet-700 text-white text-sm font-medium disabled:opacity-50"
        >
          {busy ? "Uploading…" : "Upload"}
        </button>
        {report && (
          <span className="text-xs text-neutral-700">
            {report.files_uploaded} file(s) ·{" "}
            {formatBytes(report.bytes_uploaded)} uploaded
          </span>
        )}
      </div>
      {report && report.errors.length > 0 && (
        <div className="mt-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded p-2 max-h-32 overflow-auto">
          <strong>{report.errors.length} error(s):</strong>
          <ul className="font-mono mt-1 space-y-0.5">
            {report.errors.slice(0, 20).map((e, i) => (
              <li key={i} className="truncate">
                {e}
              </li>
            ))}
            {report.errors.length > 20 && (
              <li>… {report.errors.length - 20} more</li>
            )}
          </ul>
        </div>
      )}
      {error && (
        <div className="mt-2 text-xs text-red-600 font-mono break-words bg-red-50 border border-red-200 rounded p-2">
          {error}
        </div>
      )}
    </div>
  );
}
