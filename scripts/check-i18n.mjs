#!/usr/bin/env node
// Cross-check the i18n tables against the code.
//
// Two kinds of drift creep in silently: a t() call whose key was never added
// (renders the raw key on screen), and a translated string nobody references
// (dead weight that reads as "already done"). Both were present — the Tools
// cards had translations written for them while the components used
// hardcoded English.
//
// Keys reached through a template literal, e.g. t(`nav.${id}`), are resolved
// by prefix so they don't show up as false positives.
//
// Run from repo root: node ./scripts/check-i18n.mjs

import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const localesDir = join(repoRoot, "src", "i18n", "locales");
const srcDir = join(repoRoot, "src");

function flat(o, p = "") {
  let r = [];
  for (const [k, v] of Object.entries(o)) {
    const key = p ? `${p}.${k}` : k;
    if (v && typeof v === "object") r = r.concat(flat(v, key));
    else r.push(key);
  }
  return r;
}

const en = flat(JSON.parse(readFileSync(join(localesDir, "en.json"), "utf8")));

const files = [];
(function walk(d) {
  for (const f of readdirSync(d)) {
    const p = join(d, f);
    if (statSync(p).isDirectory()) {
      if (f !== "locales") walk(p);
    } else if (/\.(tsx|ts)$/.test(f)) files.push(p);
  }
})(srcDir);

const literal = new Set();
// Prefixes referenced through a template literal, e.g. t(`nav.${id}`) — every
// key under them is reachable even though no literal mentions it.
const dynamicPrefixes = new Set();

for (const f of files) {
  const c = readFileSync(f, "utf8");
  for (const m of c.matchAll(/\bt\(\s*["'`]([a-zA-Z0-9_.]+)["'`]/g)) literal.add(m[1]);
  for (const m of c.matchAll(/i18nKey=["']([a-zA-Z0-9_.]+)["']/g)) literal.add(m[1]);
  for (const m of c.matchAll(/\bt\(\s*`([a-zA-Z0-9_.]*)\$\{/g)) {
    if (m[1]) dynamicPrefixes.add(m[1].replace(/\.$/, ""));
  }
}

const reachable = (k) =>
  literal.has(k) || [...dynamicPrefixes].some((p) => k.startsWith(p + "."));

const missing = [...literal].filter((k) => !en.includes(k));
const unused = en.filter((k) => !reachable(k));

console.log("en.json keys:          ", en.length);
console.log("literal t() refs:      ", literal.size);
console.log("dynamic prefixes:      ", [...dynamicPrefixes].join(", ") || "(none)");
console.log("referenced but MISSING:", missing.length, missing.join(", "));
console.log("defined but UNUSED:    ", unused.length);
if (unused.length) console.log("  " + unused.join("\n  "));

console.log("\ncoverage per locale:");
for (const f of readdirSync(localesDir).sort()) {
  if (f === "en.json") continue;
  const k = new Set(flat(JSON.parse(readFileSync(join(localesDir, f), "utf8"))));
  const have = en.filter((x) => k.has(x)).length;
  console.log(
    "  " + f.padEnd(9),
    String(have).padStart(3) + "/" + en.length,
    Math.round((have / en.length) * 100) + "%",
  );
}

if (missing.length > 0 || unused.length > 0) {
  console.error("\ni18n check failed.");
  process.exit(1);
}
console.log("\ni18n consistent: every key used is defined, and vice versa.");
