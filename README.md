# Lamp Bench

A local web development environment for Windows, macOS and Linux — a
from-scratch reimplementation of MAMP PRO. Bundles Apache, Nginx, MySQL
(5.7 + 8.0), multiple PHP versions, Redis, MailHog, Xdebug, OPcache,
Composer, phpMyAdmin and a Monaco-based file editor behind a native
desktop GUI.

> **Status: alpha.** Phases 0–7 are done and Phase 9 (polish + Linux) is
> well underway. The app installs as a slim ~30 MB shell that downloads
> Apache/MySQL/PHP/etc. on first launch, auto-updates via signed GitHub
> Releases, ships a Settings panel with English/Spanish/French i18n, and
> builds on Windows + Linux + macOS (Intel & Apple Silicon) in CI. Windows
> is the most exercised target; Linux/macOS native service binaries are
> still being pinned (see "Platform support").

## Install (pre-built alpha)

Grab the installer for your machine from the rolling
[**alpha-testing release**](https://github.com/LAMP-Bench/lamp/releases/tag/alpha-testing):

| Platform | File |
|---|---|
| Windows x64 | `.exe` (NSIS) or `.msi` |
| macOS Apple Silicon | `.dmg` or `.app.tar.gz` (aarch64) |
| macOS Intel | `.dmg` or `.app.tar.gz` (x86_64) |
| Linux Debian/Ubuntu/Mint | `.deb` |
| Linux Fedora/RHEL/openSUSE | `.rpm` |
| Any other Linux (Arch, …) | `.AppImage` |

On Windows the installer defaults to `C:\LAMP\`. First launch runs a
short setup wizard that downloads the bundled services into
`<install>/resources/`. The in-app updater offers new alpha builds on
each launch.

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
- Settings: language (en/es/fr), PHP/MySQL version pickers, installed-
  components manager, update channel + manual check, Dynamic DNS, About
  (version + commit SHA + build date).

## Platform support

The app, UI, hosts-file reconciliation, CA trust and DynDNS are
cross-platform. The **bundled service binaries** (Apache, MySQL, PHP,
nginx, Redis, MailHog, mod_fcgid, Xdebug) are currently pinned for Windows
only in `scripts/binaries.json` — the OS-agnostic pieces (Composer,
phpMyAdmin, the CMSes) work everywhere. Linux/macOS native service
binaries are the remaining gap.

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
./node_modules/.bin/tsc --noEmit                     # TypeScript
cargo clippy --manifest-path src-tauri/Cargo.toml --no-deps -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

CI runs all three before the build matrix.

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
