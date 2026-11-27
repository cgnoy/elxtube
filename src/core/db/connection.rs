use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::time::Duration;

use crate::core::paths;

const SCHEMA: &str = include_str!("../schema.sql");
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const THUMB_FORMAT_VERSION: &str = "source-resolution-v3";

pub(crate) fn configure_connection(conn: &Connection) -> Result<()> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    Ok(())
}

fn ensure_thumbnail_format(conn: &Connection) -> Result<()> {
    let current = conn
        .query_row(
            "SELECT value FROM settings WHERE key='thumbnail_format_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if current.as_deref() == Some(THUMB_FORMAT_VERSION) {
        return Ok(());
    }
    conn.execute("DELETE FROM thumb_blob", [])?;
    conn.execute(
        "INSERT INTO settings(key, value) VALUES('thumbnail_format_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![THUMB_FORMAT_VERSION],
    )?;
    Ok(())
}

pub fn open() -> Result<Connection> {
    open_at(&paths::db_path()?)
}

pub fn open_at(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path).with_context(|| format!("open db {}", path.display()))?;
    configure_connection(&conn)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    super::migrations::apply(&conn)?;
    conn.execute_batch(SCHEMA)?;
    ensure_thumbnail_format(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure_connection(&conn)?;
    super::migrations::apply(&conn)?;
    conn.execute_batch(SCHEMA)?;
    ensure_thumbnail_format(&conn)?;
    Ok(conn)
}
