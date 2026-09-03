# Lamp Bench

A local web development environment for Windows, macOS and Linux: a
from-scratch reimplementation of MAMP PRO. Bundles Apache, Nginx, MySQL
(5.7 + 8.0), multiple PHP versions, Redis, MailHog, Xdebug, OPcache,
Composer, phpMyAdmin and a Monaco-based file editor behind a native
desktop GUI.

> **Status: alpha.** Phases 0–7 are done and Phase 9 (polish + Linux) is
> well underway. The app installs as a slim ~30 MB shell that downloads
> Apache/MySQL/PHP/etc. on first launch, auto-updates via signed GitHub
> Releases, and builds on Windows + Linux + macOS (universal) in CI.
> Windows is the most exercised target by a wide margin. See "Platform
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
> has to be user-writable. Both of the things that make that work, the
> `C:\LAMP` default and the post-install ACL grant, live in the NSIS
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
  - rewrites the managed section of the system `hosts` file (elevated:
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
  per version, and install the ionCube loader. On Unix, components with no
  prebuilt binary can be compiled on the spot. See "Platform support".
- Every service's ports are editable from the sidebar, with collisions
  rejected before they're saved.
- 13 languages, complete, detected from the OS locale on first launch:
  English, Spanish, French, German, Italian, Portuguese, Russian,
  Turkish, Chinese, Japanese, Korean, Arabic (RTL) and Hindi.

## Platform support

The app, UI, hosts-file reconciliation, CA trust and DynDNS are
cross-platform, and so is the downloader (zip, tar, tar.gz and tar.xz, with
Unix permissions and symlinks preserved).

The **service binaries** are the interesting part, because upstream does not
publish prebuilt Unix binaries for most of them. Apache, nginx, PHP and
Redis ship source tarballs only, and the newest Linux binary the Apache
project offers is httpd 2.0.48, from 2003. There are three ways a binary
gets there, tried in this order:

1. **Bundled**: a pinned, SHA256-checksummed download into `resources/`.
   Fastest, and the only path on Windows.
2. **Compiled here**: Lamp Bench fetches the source, verifies it, and builds
   it on your machine into the same `resources/` layout. This is also what
   makes version switching work on Unix: any PHP version you ask for gets
   built, rather than being limited to whatever was pre-built for a release.
3. **From your system**: Redis and MailHog only. Both are driven entirely by
   arguments and a generated config of absolute paths, so a distro copy
   behaves identically. Apache, nginx, PHP and MySQL are not: a distro Apache
   expects its own `ServerRoot` and module layout, and the config generators
   would have to become layout-aware first.

| Component | Windows | Linux | macOS |
|---|---|---|---|
| Composer, phpMyAdmin, the CMSes | bundled | bundled | bundled |
| MailHog | bundled | bundled, or system | bundled, or system |
| MySQL | bundled | bundled | bundled |
| Redis | bundled | compiled, or system | compiled, or system |
| Apache, nginx, PHP, Xdebug | bundled | compiled | compiled (untested) |

### Compiling on your machine

Triggered from **Versions**, on any component with no prebuilt binary for
your platform. Before anything happens you see the distro that was detected,
which build tools are missing, and the exact package-manager command, with a
copy button if you would rather run it yourself. Installing those tools is
the only thing Lamp Bench does that reaches outside its own folder, and it
installs build tooling, never a service. The build log streams as it goes.

Distro handling is keyed on the **family**, not the release, so nothing is
pinned to something like "ubuntu-22.04". `/etc/os-release` reports the family
via `ID_LIKE`, which is how CachyOS resolves to Arch, Mint to Debian and
Rocky to Fedora without any of them being named. Package tables cover Debian,
Fedora, Arch, openSUSE and Alpine; anything else still builds, you just
install the tools yourself.

Budget a few minutes. On a 12-core machine Redis takes about 50 seconds and
PHP about five minutes; a four-core laptop will take considerably longer.
Nothing is bundled or RPATH-patched for a local build. It links against the
libraries already on the machine, which is why `patchelf` isn't needed and
APR and PCRE2 come from your distro's `-dev` packages instead of being built
from source.

macOS source builds use the same code path and should work with the Xcode
command line tools present, but the build-dependency tables have no Homebrew
entry yet, so the tools are on you. Untested.

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
├── resources/            Bundled service binaries, gitignored
├── scripts/              binaries.json + fetch-binaries.mjs
└── .github/workflows/    release.yml (lint → build → publish)
```

## License

[MIT](./LICENSE) © 2026 caixax

## Contributing

**Please don't open pull requests.** The codebase moves fast and can
change shape entirely between commits: refactors, renames, schema churn,
whole subsystems rewritten. A PR opened today is likely to conflict with
tomorrow's work. Bug reports and ideas are welcome via issues. This note
will change once the project settles.
