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
