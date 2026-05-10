# Lamp Bench

A local web development environment for Windows, macOS and Linux — a
from-scratch reimplementation of MAMP PRO. Bundles Apache, Nginx, MySQL
(5.7 + 8.0), multiple PHP versions, Redis, Xdebug, OPcache, Composer,
phpMyAdmin and a Monaco-based file editor behind a native desktop GUI.

> **Status:** working MVP. Phases 0–4 of the roadmap are done: services,
> hosts CRUD with system `hosts` file reconciliation, local Root CA + leaf
> certs, multi-PHP via `mod_fcgid`, Xdebug, Composer, Laravel scaffolding,
> editor with PHP lint. Phase 5 (WordPress / Joomla / Drupal one-click) is
> next. Linux is wired into the architecture but ships in Phase 9 — until
> then the verified platforms are Windows and macOS.

## What it does today

- Start / stop Apache, Nginx, MySQL (5.7 ↔ 8.0 toggle) and Redis from a
  sidebar of toggle switches.
- Create virtual hosts with per-host PHP version. A single Save:
  - inserts the host into SQLite,
  - rewrites the managed section of `C:\Windows\System32\drivers\etc\hosts`
    via an elevated child process (UAC prompt once per change),
  - issues a leaf SSL cert for the hostname, signed by the local Root CA
    that lives in `.lamp-bench/ca/`,
  - regenerates Apache + Nginx configs and restarts whichever is running.
- Edit each host's General / Apache / Nginx / SSL settings inline. The
  Apache and Nginx tabs accept raw directives that get injected inside the
  per-host vhost block.
- Edit per-version `php.ini` files in the embedded Monaco editor. PHP lint
  via `php -l` is one click. Ctrl+S saves.
- Tools panel: open phpMyAdmin in the browser, check Composer version,
  scaffold a Laravel project with `composer create-project`.
- Logs panel: live tail of Apache, Nginx, and MySQL error logs.

## Prerequisites

- Node ≥ 20 and **pnpm**
- **Rust** stable, installed via `rustup`
- **MSVC Build Tools 2022** with the VC++ workload + Windows 11 SDK on
  Windows, Xcode Command Line Tools on macOS
- **WebView2** runtime (already on Windows 11)

## Develop

```sh
pnpm install
pnpm scripts:fetch-binaries
pnpm tauri dev
```

The binary fetch pulls ~500 MB of pinned upstream zips
(Apache, Nginx, MySQL ×2, PHP ×2, Redis, phpMyAdmin, Xdebug DLLs,
Composer phar) into `resources/`, verifying SHA256 against
`scripts/binaries.json`. They are not committed.

First `pnpm tauri dev` compiles the Tauri shell from source — 5–10 min
the first time, then incremental.

## Build

```sh
pnpm tauri build
```

Produces an installer for the current OS in
`src-tauri/target/release/bundle/`.

## Repository layout

```
lamp/
├── src/                  React frontend
├── src-tauri/            Rust core (services, hosts, ssl, db)
├── resources/            Bundled service binaries — gitignored
├── scripts/              binaries.json + fetch-binaries.mjs
└── docs/
```

`resources/` is not committed. `scripts/fetch-binaries.mjs` (single Node
script, cross-platform) downloads pinned versions per OS and verifies SHA256.

`.lamp-bench/` (in the project root) holds the runtime state: SQLite DB,
generated configs, per-host certs, log tails, MySQL data dirs. Also
gitignored.

## License

TBD.

## Contributing

**Please don't open pull requests.** The codebase is moving fast and can
change shape entirely between commits — refactors, file renames, schema
churn, whole subsystems getting rewritten. A PR opened today is very
likely to conflict with something I do tomorrow, and merging it would
mean throwing your time away.

Bug reports and ideas are welcome via issues. Once the project settles
into a stable shape this section will change.

---

### Postscript: this was built with AI

Yes, this whole thing was built sitting in a chair with Claude doing the
typing. It wasn't a learning exercise, it was a quality-of-life thing,
because in 2026 if I want a working multi-PHP local dev env on Windows by
the end of a weekend, I'm going to use the tool that makes that happen.

If using AI to ship code rubs you the wrong way, that's fine, but I'm not
slowing down for it. The world moves; I'd rather keep up.

And hell nah im not gonna use MAMP or XAMMP, they run like shit.

— @caixax
