# Changelog

All notable changes to Lamp Bench. The project is in rolling **alpha** —
builds are published to the `alpha-testing` GitHub release and versioned
`0.1.<CI run number>`. This file groups changes by theme rather than by
exact build number while in alpha.

## Unreleased (alpha)

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
