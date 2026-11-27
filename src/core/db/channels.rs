use anyhow::Result;
use rusqlite::{params, Connection};

use crate::core::model::Channel;
use crate::core::now_secs;

pub fn channel_upsert(conn: &Connection, channel: &Channel) -> Result<()> {
    conn.execute(
        "INSERT INTO channels(id, name, avatar_url, subscribed_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, avatar_url=excluded.avatar_url",
        params![channel.id, channel.name, channel.avatar_url, now_secs()],
    )?;
    Ok(())
}

pub fn channel_delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM channels WHERE id=?1", params![id])?;
    Ok(())
}

pub fn channel_list(conn: &Connection) -> Result<Vec<Channel>> {
    let mut stmt = conn.prepare("SELECT id, name, avatar_url FROM channels ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        Ok(Channel {
            id: row.get(0)?,
            name: row.get(1)?,
            avatar_url: row.get(2)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
