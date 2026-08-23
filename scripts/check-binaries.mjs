#!/usr/bin/env node
// Verify every URL pinned in binaries.json still resolves.
//
// Upstreams move files out from under us. windows.php.net purges superseded
// patch releases from /releases/ into /releases/archives/, and ApacheLounge
// keeps only its latest build — both silently, both breaking a download the
// user only discovers when they click "Install". Three PHP entries were
// already dead by the time anyone noticed.
//
// Run from the repo root: node ./scripts/check-binaries.mjs
// Exits non-zero if any URL is unreachable.

import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const manifestPath = join(dirname(fileURLToPath(import.meta.url)), "binaries.json");

/// HEAD is enough to prove a file is there and costs nothing. Some CDNs
/// answer HEAD with 405 while serving GET fine, so fall back to a ranged GET
/// that pulls a single byte rather than the whole 350 MB archive.
async function probe(url) {
  try {
    const head = await fetch(url, { method: "HEAD", redirect: "follow" });
    if (head.ok) return { ok: true, status: head.status };
    if (head.status !== 405 && head.status !== 403) {
      return { ok: false, status: head.status };
    }
  } catch (e) {
    return { ok: false, status: `HEAD ${e.message}` };
  }
  try {
    const ranged = await fetch(url, {
      method: "GET",
      redirect: "follow",
      headers: { Range: "bytes=0-0" },
    });
    // 206 Partial Content when the range was honoured, 200 when it wasn't.
    return { ok: ranged.ok, status: ranged.status };
  } catch (e) {
    return { ok: false, status: `GET ${e.message}` };
  }
}

const manifest = JSON.parse(await readFile(manifestPath, "utf8"));

const targets = [];
for (const [name, entry] of Object.entries(manifest)) {
  for (const [platform, p] of Object.entries(entry.platforms ?? {})) {
    targets.push({ name, platform, url: p.url });
  }
}

console.log(`Checking ${targets.length} pinned URLs across ${Object.keys(manifest).length} entries…\n`);

const broken = [];
// Sequential on purpose: a handful of these hosts rate-limit, and the whole
// sweep still finishes in well under a minute.
for (const t of targets) {
  const { ok, status } = await probe(t.url);
  const mark = ok ? "ok  " : "FAIL";
  console.log(`${mark} ${String(status).padEnd(5)} ${t.name} (${t.platform})`);
  if (!ok) broken.push({ ...t, status });
}

if (broken.length === 0) {
  console.log(`\nAll ${targets.length} URLs reachable.`);
  process.exit(0);
}

console.log(`\n${broken.length} of ${targets.length} URLs are broken:\n`);
for (const b of broken) {
  console.log(`  ${b.name} (${b.platform}) → ${b.status}`);
  console.log(`    ${b.url}`);
}
console.log(
  "\nFor PHP this usually means the release moved to " +
    "windows.php.net/downloads/releases/archives/. The archived file is " +
    "byte-identical, so only the URL needs changing — not the SHA256.",
);
process.exit(1);
