use rusqlite::Connection;
use std::path::Path;

const INITIAL_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS hosts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL UNIQUE,
    docroot     TEXT    NOT NULL,
    php_version TEXT    NOT NULL DEFAULT '8.4',
    created_at  TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS snapshots (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    host_id     INTEGER NOT NULL,
    label       TEXT    NOT NULL,
    path        TEXT    NOT NULL UNIQUE,
    size_bytes  INTEGER NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (host_id) REFERENCES hosts(id) ON DELETE CASCADE
);

-- Per-service listen ports, editable from the sidebar service panel.
-- `port` is the primary port (Apache HTTP, MySQL, Redis, MailHog UI);
-- `port2` is the secondary where a service has one (Apache/Nginx HTTPS,
-- MailHog SMTP) and 0 otherwise. Rows are created lazily; a missing row
-- means use the compiled-in default.
CREATE TABLE IF NOT EXISTS service_config (
    service TEXT PRIMARY KEY,
    port    INTEGER NOT NULL,
    port2   INTEGER NOT NULL DEFAULT 0
);

-- One stored deploy target per host so the FTP form doesn't have to be
-- retyped on every upload. Password is stored in plaintext — acceptable
-- for a local-only dev tool whose entire install dir is already
-- user-readable; documented as such in the UI.
CREATE TABLE IF NOT EXISTS deploy_profiles (
    host_id      INTEGER PRIMARY KEY,
    protocol     TEXT    NOT NULL DEFAULT 'ftp',
    ftp_host     TEXT    NOT NULL DEFAULT '',
    ftp_port     INTEGER NOT NULL DEFAULT 21,
    ftp_user     TEXT    NOT NULL DEFAULT '',
    ftp_password TEXT    NOT NULL DEFAULT '',
    remote_dir   TEXT    NOT NULL DEFAULT '/',
    FOREIGN KEY (host_id) REFERENCES hosts(id) ON DELETE CASCADE
);
";

pub fn open(path: &Path) -> Result<Connection, rusqlite::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(INITIAL_SCHEMA)?;
    migrate(&conn)?;
    Ok(conn)
}

/// Forward migrations applied to DBs that pre-date a column. Idempotent —
/// safe to call on fresh tables.
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    add_column_if_missing(conn, "hosts", "php_version", "TEXT NOT NULL DEFAULT '8.4'")?;
    add_column_if_missing(conn, "hosts", "apache_extra", "TEXT NOT NULL DEFAULT ''")?;
    add_column_if_missing(conn, "hosts", "nginx_extra", "TEXT NOT NULL DEFAULT ''")?;
    // Marker that this snapshot's archive carries a mysqldump alongside the
    // docroot files. Pre-existing rows default to 0 (files-only) and remain
    // restorable — `restore` looks for `db.sql` inside the archive itself,
    // so the column is informational, used by the UI to badge entries.
    add_column_if_missing(conn, "snapshots", "has_db", "INTEGER NOT NULL DEFAULT 0")?;
    // MySQL version the dump was taken under. Empty for files-only snapshots
    // and for rows created before this column existed. Used to warn on
    // cross-version restore (5.7 dump into an 8.0 server, etc.).
    add_column_if_missing(conn, "snapshots", "mysql_version", "TEXT NOT NULL DEFAULT ''")?;
    Ok(())
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    spec: &str,
) -> rusqlite::Result<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
        rusqlite::params![table, column],
        |row| row.get(0),
    )?;
    if count == 0 {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {spec}"),
            [],
        )?;
    }
    Ok(())
}
