# Changelog

All notable changes to Lamp Bench. The project is in rolling **alpha** —
builds are published to the `alpha-testing` GitHub release and versioned
`0.1.<CI run number>`. This file groups changes by theme rather than by
exact build number while in alpha.

## Unreleased (alpha)

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
