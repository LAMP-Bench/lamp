# Lamp Bench

A local web development environment for Windows, macOS and Linux — a
from-scratch reimplementation of MAMP PRO. Bundles Apache, Nginx, MySQL
(5.7 + 8.0), multiple PHP versions, Redis, MailHog, Xdebug, OPcache,
Composer, phpMyAdmin and a Monaco-based file editor behind a native
desktop GUI.

> **Status: alpha.** Phases 0–7 are done and Phase 9 (polish + Linux) is
> well underway. The app installs as a slim ~30 MB shell that downloads
> Apache/MySQL/PHP/etc. on first launch, auto-updates via signed GitHub
> Releases, and builds on Windows + Linux + macOS (universal) in CI.
> Windows is the most exercised target by a wide margin — see "Platform
> support" for exactly where the others stand.

## Install (pre-built alpha)

Grab the installer for your machine from the rolling
[**alpha-testing release**](https://github.com/LAMP-Bench/lamp/releases/tag/alpha-testing):

| Platform | File |
|---|---|
| Windows x64 | `.exe` (NSIS) |
| macOS (Intel + Apple Silicon) | `.dmg` or `.app.tar.gz` (universal) |
| Linux Debian/Ubuntu/Mint | `.deb` |
| Linux Fedora/RHEL/openSUSE | `.rpm` |
| Any other Linux (Arch, …) | `.AppImage` |

On Windows the installer defaults to `C:\LAMP\`. First launch runs a
short setup wizard that downloads the bundled services into
`<install>/resources/`. The in-app updater offers new alpha builds on
each launch.

> **Why no `.msi`?** Lamp Bench keeps its runtime state, downloaded
> binaries and `htdocs/` next to the executable, so the install directory
> has to be user-writable. Both of the things that make that work — the
> `C:\LAMP` default and the post-install ACL grant — live in the NSIS
> installer. WiX gets neither, so an `.msi` would land in
> `C:\Program Files` and fail on first launch. Shipping one would just be
> a broken download, so the Windows bundle is NSIS-only.

> ⚠ **Alpha software.** Things change shape between builds; expect rough
> edges. Don't point it at data you can't afford to lose.

## What it does

- Start / stop Apache, Nginx, MySQL (5.7 ↔ 8.0), Redis and MailHog from a
  sidebar of toggle switches. No CMD windows flash; closing the window
  minimises to the system tray (Discord-style).
- Create virtual hosts with a per-host PHP version. A single Save:
  - inserts the host into SQLite,
  - rewrites the managed section of the system `hosts` file (elevated —
    UAC on Windows, `osascript`/`pkexec` on macOS/Linux),
  - issues a leaf SSL cert signed by a local Root CA, installed into the
    platform user trust store,
  - regenerates Apache + Nginx configs and reloads whichever is running.
- Per-host tabs: General / Apache / Nginx / SSL / Snapshots / Deploy.
  Snapshots are `.tar.zst` archives of the docroot with an optional
  `mysqldump`; Deploy uploads the docroot to a saved FTP profile.
- Tools: phpMyAdmin, MailHog inbox, image optimizer (JPG re-encode +
  lossless PNG), FTP deploy, Composer, Laravel scaffolding, one-click
  WordPress / Joomla / Drupal / MediaWiki.
- Config: edit per-version `php.ini`, `httpd.conf`, `nginx.conf`, `my.cnf`
  in a standalone Monaco window. `php -l` lint is one click.
- Logs: live tail of Apache, Nginx, MySQL, Redis and MailHog.
- Settings: language, default PHP and MySQL version, installed-components
  manager (with download progress), manual update check, Dynamic DNS,
  About (version + commit SHA + build date).
- Versions: install or remove any pinned component, toggle PHP extensions
  per version, and install the ionCube loader.
- Every service's ports are editable from the sidebar, with collisions
  rejected before they're saved.
- 13 languages, detected from the OS locale on first launch. English,
  Spanish and French are complete; the other ten cover the common UI and
  fall back to English for the longer technical strings.

## Platform support

The app, UI, hosts-file reconciliation, CA trust and DynDNS are
cross-platform, and so is the downloader (zip, tar, tar.gz and tar.xz,
with Unix permissions and symlinks preserved).

The **service binaries** are a different story, and it's worth being
precise about why:

| Component | Windows | Linux | macOS |
|---|---|---|---|
| Composer, phpMyAdmin, the CMSes | bundled | bundled | bundled |
| MailHog | bundled | bundled | bundled |
| MySQL | bundled | package manager | bundled |
| Apache, nginx, PHP, Redis | bundled | package manager | **gap** |

Upstream publishes prebuilt Windows binaries for everything, but for
Unix, Apache, nginx, PHP and Redis ship **source tarballs only** —
there is nothing to pin. On Linux that's fine: those all have good
distro packages, and Redis and MailHog already run from a system install
if one is present. Making Apache, nginx and MySQL do the same needs the
config generators to become layout-aware (a distro Apache expects its
own `ServerRoot` and module paths), which is the next piece of work.

MySQL's Linux build is deliberately not pinned: the only one offered is
a 892 MB tarball, against a few tens of MB from `apt`.

macOS is the weak spot — no system package manager to fall back on, and
no upstream binaries for four of the six services.

## Prerequisites (development)

- Node ≥ 20 and **pnpm**
- **Rust** stable (rustup)
- Windows: **MSVC Build Tools 2022** (VC++ workload + Win11 SDK), WebView2
  (preinstalled on Win11)
- Linux: `libwebkit2gtk-4.1-dev`, `libsoup-3.0-dev`, `libxdo-dev`,
  `librsvg2-dev`, `libayatana-appindicator3-dev`
- macOS: Xcode Command Line Tools

## Develop

```sh
pnpm install
pnpm scripts:fetch-binaries        # download + verify pinned binaries
pnpm tauri dev                     # run with hot reload
```

First `pnpm tauri dev` compiles the Tauri shell from source (5–10 min,
then incremental). `resources/` (binaries) and `.lamp-bench/` (dev
runtime state: SQLite, certs, generated configs, logs) are gitignored.

## Build

```sh
pnpm tauri build
```

Produces an installer for the current OS in
`src-tauri/target/release/bundle/`.

## Quality gates

```sh
pnpm exec tsc --noEmit                               # TypeScript
pnpm scripts:check-i18n                              # keys vs. code, both ways
cargo clippy --manifest-path src-tauri/Cargo.toml --no-deps -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm scripts:check-binaries                          # pinned URLs still live
```

CI runs the first four before the build matrix. The URL probe runs on
its own weekly schedule and opens an issue when an upstream purges a
file we pin.

## Repository layout

```
lamp/
├── src/                  React 19 + TS frontend
├── src-tauri/            Rust core (services, hosts, ssl, deploy, dyndns, …)
├── resources/            Bundled service binaries — gitignored
├── scripts/              binaries.json + fetch-binaries.mjs
└── .github/workflows/    release.yml (lint → build → publish)
```

## License

[MIT](./LICENSE) © 2026 caixax

## Contributing

**Please don't open pull requests.** The codebase moves fast and can
change shape entirely between commits — refactors, renames, schema churn,
whole subsystems rewritten. A PR opened today is likely to conflict with
tomorrow's work. Bug reports and ideas are welcome via issues. This note
will change once the project settles.
