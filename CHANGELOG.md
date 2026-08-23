# Changelog

All notable changes to Lamp Bench. The project is in rolling **alpha** —
builds are published to the `alpha-testing` GitHub release and versioned
`0.1.<CI run number>`. This file groups changes by theme rather than by
exact build number while in alpha.

## Unreleased (alpha)

### Configurable ports, actually wired
- Moving a service off its default port now works everywhere. MySQL's port
  was read from config in exactly one place and hardcoded as 3306 in
  another, so `mysqldump`, snapshot restore and the CMS installers' `CREATE
  DATABASE` all failed with "can't connect" against a server that was
  running fine.
- The nineteen `:8080`/`:3306`/`:8025` literals scattered through the UI are
  gone. WebStart, the phpMyAdmin and MailHog shortcuts, the per-host
  HTTP/HTTPS buttons, the CMS "open your new site" link and the phone QR
  code all read the live configuration, and editing a port in the sidebar
  updates them immediately instead of at next launch.

### php.ini has one owner again
- The settings Lamp Bench manages live in a delimited block that is
  rewritten on every service start. Previously it was written once at file
  creation and never revisited: changing MailHog's SMTP port left `php.ini`
  pointing at the old one, and no existing install could ever receive a new
  default.
- Fixed a race with lasting consequences: opening Versions → extensions
  before a service had started created a bare `php.ini`, after which the
  block was never added at all — that PHP version permanently lost
  `extension_dir`, mysqli and Xdebug. Both paths now go through the same
  seeder.
- Default extensions are enabled by uncommenting the template's own lines
  instead of appending duplicates, so the Versions panel shows one entry per
  extension and toggling one keeps working. User toggles live outside the
  managed block and survive its rewrites. Upgrading installs have the old
  block removed rather than stacked on top of, which would have loaded
  Xdebug and OPcache twice.
- Nginx seeds `php.ini` too. An Nginx-only user never starts Apache, and
  until now their PHP ran with no `extension_dir` at all.

### Packaging & release
- The code editor is bundled instead of fetched. `@monaco-editor/react`
  was loading Monaco from `cdn.jsdelivr.net` at runtime, so the editor
  never opened without an internet connection. It now ships with the app
  and loads lazily, which also keeps ~4 MB out of the main window's
  startup path.
- Windows builds an `.exe` only. The `C:\LAMP` default and the
  post-install permission grant both live in the NSIS installer, and WiX
  got neither — the `.msi` we were publishing installed somewhere it
  couldn't write and failed on first launch.
- Config section paths are supplied by the backend (`runtime_path`,
  `resources_path`) instead of derived from `htdocs_path` by string
  surgery. The derivation was right in a dev checkout and wrong in every
  real install, so every "Edit" button opened a file that didn't exist.
  The `my.cnf` row now follows the active MySQL version.
- Releases are staged before they go live. The workflow used to delete
  the published release as its first step, leaving the updater with
  nothing for the whole build — permanently, if any platform failed.
  Builds now upload to a private staging tag and only the last few
  seconds of a fully green run touch the public release.
- Dropped the ~350 MB binary fetch from the build. The installer has
  been slim since the first-launch wizard landed; nothing was bundling
  those files.
- New weekly job probes every URL in `binaries.json` and opens an issue
  when one goes dark, so a purged upstream is caught before a user
  clicks Install. Also runnable locally with
  `pnpm scripts:check-binaries`.

### Reliability
- Quitting no longer orphans the services. Apache, MySQL, Nginx, the
  php-cgi pools, Redis and MailHog are now stopped on the way out, so the
  next launch doesn't find its ports held and MySQL's data dir locked by
  processes the app can no longer see.
- MySQL is asked to shut down cleanly (`mysqladmin shutdown`, 20s grace)
  before being killed. Every stop used to be a hard kill, which meant an
  InnoDB recovery pass on every start.
- A service that fails to start now says so. Errors were being captured
  and never rendered, so a failed toggle just sprang back with no
  explanation. Starts are also re-checked after a moment, because
  httpd/mysqld/nginx exit *after* a successful spawn when their port is
  taken or the config is bad.
- php-cgi pools no longer retire themselves after 500 requests
  (`PHP_FCGI_MAX_REQUESTS=0`), which showed up as Nginx suddenly serving
  502s mid-session and as intermittent 500s under mod_fcgid. The Nginx
  pools also fork workers instead of handling one request at a time.
- Repointed every PHP download at `windows.php.net/…/releases/archives/`.
  PHP 8.2, 8.3 and 8.5 were returning 404: upstream purges superseded
  patch releases out of `/releases/`. Checksums are unchanged.
- `PRAGMA foreign_keys = ON`. SQLite defaults it off, so the
  `ON DELETE CASCADE` clauses had never done anything — deleting a host
  left its snapshot rows and deploy profile behind. Deleting a host now
  also removes its snapshot archives and leaf certificate from disk.
- Host names are validated and normalised (trimmed, lowercased, DNS
  labels only). The value is written to the system hosts file *with
  elevation*, interpolated into the Apache vhost, and used as a filename
  under `runtime/ssl/`, none of which tolerated a stray newline or
  `../`. Duplicate names now report "a host named `x` already exists"
  instead of a raw SQLite constraint error.

### Cross-platform
- Implement system `hosts` file reconciliation on Linux (`pkexec`/`sudo`)
  and macOS (`osascript` admin elevation), replacing the Windows-only stub.
- Install the local Root CA into the trust store on macOS
  (`security add-trusted-cert`) and Linux (`update-ca-certificates` /
  `update-ca-trust`), in addition to Windows.
- First-launch setup wizard now runs on every OS and surfaces per-binary
  download errors instead of a blanket "platform not supported" screen.
- Cross-platform `binaries.json` entries for the OS-agnostic bundles
  (Composer, phpMyAdmin, WordPress, Joomla, Drupal, MediaWiki).
- CI builds Intel macOS (`x86_64-apple-darwin`) alongside Apple Silicon.

### Internationalisation
- Full i18n pass across every section (Home, Hosts, Tools, Config, Logs,
  Editor, TopBar, UpdateBanner, Settings). Fixed a mislabelled sidebar nav
  group.
- `document.documentElement.lang` now tracks the active language.
- Added French locale (English + Spanish + French).

### Settings
- PHP and MySQL version selectors.
- "Installed components" manager with install/remove (`binary_remove`,
  `binary_list`).
- Update channel selector (alpha/beta/stable scaffold).
- About panel shows commit SHA + build date (embedded via `build.rs`).
- Dynamic DNS card: No-IP / Dyn / DNS-O-Matic / easyDNS / spDYN over the
  dyndns2 protocol with a manual "Update now".

### UX
- Replaced `alert()`/`confirm()` with non-blocking Toast + Confirm
  components.
- Streaming download progress (percent) for bundled binaries.
- Editor warns before closing a window with unsaved changes.
- Main-window size + position persist across launches.

### Snapshots & logs
- Snapshots record the MySQL version they were taken under and warn on
  cross-version restore.
- Logs viewer adds Redis + MailHog tabs; reads the tail from EOF instead
  of slurping the whole file; fixed the MySQL log path.

### Deploy
- Per-host deploy profiles stored in SQLite; new Deploy tab on each host.
- FTPS/SFTP are explicitly rejected for now rather than silently
  downgrading to plaintext FTP (encrypted transports are a later release).

### CI / quality
- Lint gate before the build matrix: `tsc --noEmit`, `clippy -D warnings`,
  `cargo test` (unit tests for hosts-file reconciliation + dyndns base64).
- Builds stamp `0.1.<run_number>` into all manifests so the in-app updater
  actually fires; an immutable `build-<run_number>` git tag preserves
  history alongside the rolling `alpha-testing` tag.

### Deferred
- Real FTPS transport and SFTP (needs an async runtime).
- Cloud storage sync — Google Drive / OneDrive / Dropbox (needs OAuth app
  credentials + redirect server).
- Linux/macOS native service binaries (Apache/MySQL/PHP/nginx/Redis/MailHog).
