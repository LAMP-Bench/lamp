import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FiEdit3, FiFileText } from "react-icons/fi";
import { SiPhp, SiApache, SiNginx, SiMysql } from "react-icons/si";

export function ConfigSection() {
  const [versions, setVersions] = useState<string[]>([]);
  const [htdocs, setHtdocs] = useState<string>("");

  useEffect(() => {
    invoke<string[]>("php_versions").then(setVersions).catch(() => {});
    invoke<string>("htdocs_path").then(setHtdocs).catch(() => {});
  }, []);

  // htdocs gives us the runtime path one level up; resources is sibling.
  const runtime = htdocs.replace(/\/htdocs\/?$/, "");
  const resources = runtime.replace(/\/[^/]+$/, "") + "/resources";

  async function open(path: string) {
    try {
      await invoke("editor_open", { path });
    } catch (e) {
      alert(`Could not open editor: ${e}`);
    }
  }

  if (!htdocs) {
    return (
      <div className="p-6 text-sm text-neutral-500">Loading paths…</div>
    );
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="p-6 max-w-3xl space-y-6">
        <section>
          <h2 className="text-xs uppercase tracking-wider text-neutral-500 font-medium mb-2">
            PHP
          </h2>
          <div className="rounded-lg border border-neutral-200 bg-white overflow-hidden divide-y divide-neutral-100">
            {versions.map((v) => (
              <ConfigRow
                key={v}
                icon={<SiPhp className="text-indigo-500" />}
                title={`php.ini — PHP ${v}`}
                subtitle={`${resources}/php-${v}/php.ini`}
                onEdit={() => open(`${resources}/php-${v}/php.ini`)}
              />
            ))}
          </div>
        </section>

        <section>
          <h2 className="text-xs uppercase tracking-wider text-neutral-500 font-medium mb-2">
            Servers
          </h2>
          <div className="rounded-lg border border-neutral-200 bg-white overflow-hidden divide-y divide-neutral-100">
            <ConfigRow
              icon={<SiApache className="text-red-500" />}
              title="Apache httpd.conf"
              subtitle={`${runtime}/apache/httpd.conf`}
              hint="Regenerated on each Apache start — manual edits get clobbered. Use the Apache tab on a host for per-host directives."
              onEdit={() => open(`${runtime}/apache/httpd.conf`)}
            />
            <ConfigRow
              icon={<SiNginx className="text-emerald-500" />}
              title="Nginx nginx.conf"
              subtitle={`${runtime}/nginx/nginx.conf`}
              hint="Regenerated on each Nginx start. Use the Nginx tab on a host for per-host directives."
              onEdit={() => open(`${runtime}/nginx/nginx.conf`)}
            />
            <ConfigRow
              icon={<SiMysql className="text-sky-500" />}
              title="MySQL my.cnf"
              subtitle={`${runtime}/mysql-<version>/my.cnf`}
              hint="Path depends on the active MySQL version. Edit after Start so the file exists."
              onEdit={() => {
                // Best effort — try 8.0, fall back to 5.7.
                open(`${runtime}/mysql-8.0/my.cnf`).catch(() =>
                  open(`${runtime}/mysql-5.7/my.cnf`)
                );
              }}
            />
          </div>
        </section>

        <section>
          <p className="text-xs text-neutral-500">
            Edits open in a separate Monaco window. Use{" "}
            <kbd className="px-1 py-0.5 rounded bg-neutral-100 border border-neutral-300 font-mono text-[10px]">
              Ctrl+S
            </kbd>{" "}
            to save. Server processes need a Stop + Start for changes to{" "}
            <code>php.ini</code> to take effect — they're loaded at process
            spawn time.
          </p>
        </section>
      </div>
    </div>
  );
}

function ConfigRow({
  icon,
  title,
  subtitle,
  hint,
  onEdit,
}: {
  icon: React.ReactNode;
  title: string;
  subtitle: string;
  hint?: string;
  onEdit: () => void;
}) {
  return (
    <div className="px-4 py-3 flex items-center gap-4">
      <div className="text-xl shrink-0">{icon}</div>
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-neutral-900 flex items-center gap-2">
          <FiFileText className="text-neutral-400" />
          {title}
        </div>
        <div className="text-[11px] text-neutral-500 font-mono truncate">
          {subtitle}
        </div>
        {hint && (
          <div className="text-[11px] text-neutral-500 mt-1">{hint}</div>
        )}
      </div>
      <button
        onClick={onEdit}
        className="px-3 py-1.5 rounded border border-neutral-300 hover:bg-neutral-50 text-sm flex items-center gap-1.5 text-neutral-700"
      >
        <FiEdit3 />
        Edit
      </button>
    </div>
  );
}
