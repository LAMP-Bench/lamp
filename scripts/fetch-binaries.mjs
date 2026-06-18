#!/usr/bin/env node
// Download and verify pinned bundled binaries into ./resources/.
// Cross-platform: works on Windows, macOS, and Linux.
// Run from repo root: pnpm scripts:fetch-binaries [--force]

import { readFile, mkdir, rm, rename, copyFile } from "node:fs/promises";
import { createWriteStream, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { dirname, join, resolve } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

const execFileP = promisify(execFile);

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");
const resourcesDir = join(repoRoot, "resources");
const cacheDir = join(resourcesDir, ".cache");
const manifestPath = join(__dirname, "binaries.json");

function detectPlatform() {
  const { platform, arch } = process;
  if (platform === "win32" && arch === "x64") return "windows-x64";
  if (platform === "darwin" && arch === "arm64") return "macos-arm64";
  if (platform === "darwin" && arch === "x64") return "macos-x64";
  if (platform === "linux" && arch === "x64") return "linux-x64";
  throw new Error(`Unsupported platform: ${platform}/${arch}`);
}

async function sha256(path) {
  const buf = await readFile(path);
  return createHash("sha256").update(buf).digest("hex").toUpperCase();
}

async function download(url, destPath) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  await pipeline(Readable.fromWeb(res.body), createWriteStream(destPath));
}

async function extract(archivePath, destDir) {
  await mkdir(destDir, { recursive: true });
  const isZip = archivePath.toLowerCase().endsWith(".zip");

  if (process.platform === "win32") {
    // bsdtar ships with Windows 10+ and handles .zip natively. Pin to
    // System32\tar.exe so a GNU tar shimmed in by Git for Windows is not
    // picked up — it parses `C:` as a remote host and bails.
    const tarBin = join(
      process.env.SystemRoot ?? "C:\\Windows",
      "System32",
      "tar.exe",
    );
    await execFileP(tarBin, ["-xf", archivePath, "-C", destDir]);
    return;
  }

  // macOS / Linux: GNU tar (the default /usr/bin/tar on Ubuntu CI runners)
  // CANNOT read zip archives — it only does tar streams. Use `unzip` for
  // .zip and `tar` for actual tarballs. Both unzip and tar are preinstalled
  // on the GitHub macos/ubuntu runners.
  if (isZip) {
    await execFileP("unzip", ["-q", "-o", archivePath, "-d", destDir]);
  } else {
    await execFileP("tar", ["-xf", archivePath, "-C", destDir]);
  }
}

async function main() {
  const force = process.argv.includes("--force");
  // `--bundled-only` skips entries marked `bundled: false` in binaries.json.
  // CI release builds use this so the installer doesn't carry every CMS and
  // every PHP version — those download on-demand at runtime.
  const bundledOnly = process.argv.includes("--bundled-only");
  const platform = detectPlatform();
  console.log(`Platform: ${platform}${bundledOnly ? " (bundled-only)" : ""}`);

  await mkdir(resourcesDir, { recursive: true });
  await mkdir(cacheDir, { recursive: true });

  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));

  for (const [name, entry] of Object.entries(manifest)) {
    // `bundled` defaults to true when missing so existing entries don't break.
    const isBundled = entry.bundled !== false;
    if (bundledOnly && !isBundled) {
      console.log(`[skip] ${name} is on-demand only (bundled: false)`);
      continue;
    }

    const p = entry.platforms?.[platform];
    if (!p) {
      console.log(`[skip] no ${platform} binary configured for ${name}`);
      continue;
    }

    // `raw_file` mode: download a single file into an existing dir without
    // wiping its parent. Used for things like Xdebug DLLs that drop into
    // an existing PHP install's `ext/` folder.
    if (entry.raw_file) {
      const target = join(resourcesDir, entry.raw_file);
      if (existsSync(target) && !force) {
        console.log(`[skip] ${name} already at ${entry.raw_file} (use --force to redownload)`);
        continue;
      }
      console.log(`[fetch] ${name} ${entry.version}`);
      const cached = join(cacheDir, p.filename);
      if (!existsSync(cached)) {
        console.log(`  downloading ${p.url}`);
        await download(p.url, cached);
      } else {
        console.log(`  using cached ${p.filename}`);
      }
      console.log("  verifying SHA256");
      const actual = await sha256(cached);
      if (actual !== p.sha256.toUpperCase()) {
        await rm(cached);
        throw new Error(
          `SHA256 mismatch for ${name}: expected ${p.sha256}, got ${actual}`
        );
      }
      await mkdir(dirname(target), { recursive: true });
      await copyFile(cached, target);
      console.log(`  done -> resources/${entry.raw_file}`);
      continue;
    }

    // Archive mode (default): extract a zip into resources/<extract_to>/.
    const target = join(resourcesDir, entry.extract_to);
    if (existsSync(target) && !force) {
      console.log(`[skip] ${name} already at ${entry.extract_to}/ (use --force to redownload)`);
      continue;
    }

    console.log(`[fetch] ${name} ${entry.version}`);
    const archivePath = join(cacheDir, p.filename);

    if (!existsSync(archivePath)) {
      console.log(`  downloading ${p.url}`);
      await download(p.url, archivePath);
    } else {
      console.log(`  using cached ${p.filename}`);
    }

    console.log("  verifying SHA256");
    const actual = await sha256(archivePath);
    if (actual !== p.sha256.toUpperCase()) {
      await rm(archivePath);
      throw new Error(
        `SHA256 mismatch for ${name}: expected ${p.sha256}, got ${actual}`
      );
    }

    console.log("  extracting");
    if (existsSync(target)) {
      await rm(target, { recursive: true, force: true });
    }

    if (p.strip_root_dir) {
      const tmpDir = join(cacheDir, `.extract-${name}`);
      if (existsSync(tmpDir)) await rm(tmpDir, { recursive: true, force: true });
      await extract(archivePath, tmpDir);
      const rootDir = join(tmpDir, p.strip_root_dir);
      if (!existsSync(rootDir)) {
        throw new Error(
          `Expected root dir '${p.strip_root_dir}' not found inside ${p.filename}`
        );
      }
      await rename(rootDir, target);
      await rm(tmpDir, { recursive: true, force: true });
    } else {
      await extract(archivePath, target);
    }

    console.log(`  done -> resources/${entry.extract_to}/`);
  }

  console.log("\nAll binaries up to date.");
}

main().catch((err) => {
  console.error("FAIL:", err.message);
  process.exit(1);
});
