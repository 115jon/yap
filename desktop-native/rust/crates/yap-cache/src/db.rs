//! The SQLite file: schema, the boot read and batched writes.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{
    CacheError, ChannelsResponse, Message, Server, Snapshot, User, Write, CHANNEL_WINDOWS,
    MESSAGE_WINDOW,
};

/// Bumped whenever the tables or the JSON in them change shape. Any other
/// version on disk is dropped and recreated.
pub const SCHEMA_VERSION: i32 = 1;

const SCHEMA: &str = "
CREATE TABLE users (
    user_id        TEXT PRIMARY KEY,
    me             TEXT,
    active_server  TEXT,
    active_channel TEXT,
    written_at     INTEGER NOT NULL
);
CREATE TABLE servers (
    user_id  TEXT NOT NULL,
    position INTEGER NOT NULL,
    body     TEXT NOT NULL,
    PRIMARY KEY (user_id, position)
) WITHOUT ROWID;
CREATE TABLE channel_trees (
    user_id   TEXT NOT NULL,
    server_id TEXT NOT NULL,
    body      TEXT NOT NULL,
    PRIMARY KEY (user_id, server_id)
) WITHOUT ROWID;
CREATE TABLE messages (
    user_id    TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    position   INTEGER NOT NULL,
    body       TEXT NOT NULL,
    written_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, channel_id, position)
) WITHOUT ROWID;
";

/// Coalesced writes: per user, the newest value per key, plus whether the
/// user's snapshot is wiped first. Applied in one transaction.
#[derive(Debug, Default)]
pub struct Batch {
    users: HashMap<String, UserBatch>,
}

#[derive(Debug, Default)]
struct UserBatch {
    /// Delete everything stored for the user before the writes below.
    clear: bool,
    me: Option<Box<User>>,
    servers: Option<Vec<Server>>,
    route: Option<(Option<String>, Option<String>)>,
    channels: HashMap<String, ChannelsResponse>,
    messages: HashMap<String, Vec<Message>>,
}

impl UserBatch {
    fn has_writes(&self) -> bool {
        self.me.is_some()
            || self.servers.is_some()
            || self.route.is_some()
            || !self.channels.is_empty()
            || !self.messages.is_empty()
    }
}

impl Batch {
    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }

    /// Adds a write; it replaces an earlier one for the same key.
    pub fn push(&mut self, user_id: &str, write: Write) {
        let user = self.users.entry(user_id.to_owned()).or_default();
        match write {
            Write::Me(me) => user.me = Some(me),
            Write::Servers(servers) => user.servers = Some(servers),
            Write::Channels { server_id, tree } => {
                user.channels.insert(server_id, tree);
            }
            Write::Route {
                server_id,
                channel_id,
            } => user.route = Some((server_id, channel_id)),
            Write::Messages {
                channel_id,
                messages,
            } => {
                user.messages.insert(channel_id, messages);
            }
        }
    }

    /// Wipes the user's snapshot, dropping the writes queued before it.
    pub fn clear(&mut self, user_id: &str) {
        self.users.insert(
            user_id.to_owned(),
            UserBatch {
                clear: true,
                ..UserBatch::default()
            },
        );
    }
}

/// The open cache file. Every call blocks; keep it off async tasks.
pub struct Cache {
    conn: Connection,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn encode<T: Serialize>(value: &T) -> Result<String, CacheError> {
    Ok(serde_json::to_string(value)?)
}

/// Rows that no longer decode are skipped, like the lenient REST lists.
fn decode<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_str(text).ok()
}

/// Removes the database and its WAL side files (missing ones are fine).
fn remove_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        let _ = std::fs::remove_file(name);
    }
}

impl Cache {
    /// Opens (creating the folders) or recreates the cache at `path`. A file
    /// that does not open as a database is deleted and started over.
    pub fn open(path: &Path) -> Result<Self, CacheError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?).or_else(|_| {
            remove_files(path);
            Self::init(Connection::open(path)?)
        })
    }

    /// A cache that lives only as long as the value (tests).
    pub fn open_in_memory() -> Result<Self, CacheError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, CacheError> {
        conn.busy_timeout(Duration::from_secs(2))?;
        // Readers never wait on the writer, and a crash mid-write keeps the
        // last committed snapshot.
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let mut cache = Self { conn };
        if version != SCHEMA_VERSION {
            cache.recreate()?;
        }
        Ok(cache)
    }

    /// Drops every table and creates the current schema.
    fn recreate(&mut self) -> Result<(), CacheError> {
        let tx = self.conn.transaction()?;
        let tables: Vec<String> = {
            let mut query = tx.prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )?;
            let rows = query.query_map([], |row| row.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        for table in tables {
            tx.execute_batch(&format!("DROP TABLE \"{}\";", table.replace('"', "\"\"")))?;
        }
        tx.execute_batch(SCHEMA)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
        Ok(())
    }

    /// The version stored in the file.
    pub fn schema_version(&self) -> Result<i32, CacheError> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    /// The most recently written user's snapshot: the one whose token the
    /// keyring holds, unless the token belongs to someone else (the caller
    /// checks against `/me`).
    pub fn latest(&self) -> Result<Option<Snapshot>, CacheError> {
        let user: Option<String> = self
            .conn
            .query_row(
                "SELECT user_id FROM users WHERE me IS NOT NULL
                 ORDER BY written_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        match user {
            Some(user) => self.load(&user),
            None => Ok(None),
        }
    }

    /// One user's snapshot, with the newest window of its active channel.
    /// `None` until `me` has been stored for that user.
    pub fn load(&self, user_id: &str) -> Result<Option<Snapshot>, CacheError> {
        let head: Option<(String, Option<String>, Option<String>, i64)> = self
            .conn
            .query_row(
                "SELECT me, active_server, active_channel, written_at FROM users
                 WHERE user_id = ?1 AND me IS NOT NULL",
                [user_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((me, active_server, active_channel, written_at_ms)) = head else {
            return Ok(None);
        };
        let Some(me) = decode::<User>(&me) else {
            return Ok(None);
        };
        let servers = self.bodies(
            "SELECT body FROM servers WHERE user_id = ?1 ORDER BY position",
            params![user_id],
        )?;
        let channels = {
            let mut query = self
                .conn
                .prepare("SELECT server_id, body FROM channel_trees WHERE user_id = ?1")?;
            let rows = query.query_map([user_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut channels = Vec::new();
            for row in rows {
                let (server_id, body) = row?;
                if let Some(tree) = decode(&body) {
                    channels.push((server_id, tree));
                }
            }
            channels
        };
        let messages = match &active_channel {
            Some(channel_id) => self.bodies(
                "SELECT body FROM messages WHERE user_id = ?1 AND channel_id = ?2
                 ORDER BY position",
                params![user_id, channel_id],
            )?,
            None => Vec::new(),
        };
        Ok(Some(Snapshot {
            me,
            servers,
            channels,
            active_server,
            active_channel,
            messages,
            written_at_ms,
        }))
    }

    /// The `body` column of a query, decoded; rows that do not decode are skipped.
    fn bodies<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<T>, CacheError> {
        let mut query = self.conn.prepare_cached(sql)?;
        let rows = query.query_map(params, |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            if let Some(value) = decode(&row?) {
                out.push(value);
            }
        }
        Ok(out)
    }

    /// Applies a batch in one transaction, stamped with the current time.
    pub fn apply(&mut self, batch: Batch) -> Result<(), CacheError> {
        self.apply_at(batch, now_ms())
    }

    /// [`Self::apply`] with an explicit write time (epoch milliseconds).
    pub fn apply_at(&mut self, batch: Batch, written_at: i64) -> Result<(), CacheError> {
        let tx = self.conn.transaction()?;
        for (user_id, user) in batch.users {
            if user.clear {
                delete_user(&tx, &user_id)?;
            }
            if user.has_writes() {
                write_user(&tx, &user_id, user, written_at)?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Wipes one user's snapshot now.
    pub fn clear(&mut self, user_id: &str) -> Result<(), CacheError> {
        let mut batch = Batch::default();
        batch.clear(user_id);
        self.apply(batch)
    }
}

fn delete_user(tx: &Transaction<'_>, user_id: &str) -> Result<(), CacheError> {
    for table in ["users", "servers", "channel_trees", "messages"] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE user_id = ?1"),
            [user_id],
        )?;
    }
    Ok(())
}

fn write_user(
    tx: &Transaction<'_>,
    user_id: &str,
    user: UserBatch,
    written_at: i64,
) -> Result<(), CacheError> {
    tx.execute(
        "INSERT INTO users (user_id, written_at) VALUES (?1, ?2)
         ON CONFLICT (user_id) DO UPDATE SET written_at = excluded.written_at",
        params![user_id, written_at],
    )?;
    if let Some(me) = &user.me {
        tx.execute(
            "UPDATE users SET me = ?2 WHERE user_id = ?1",
            params![user_id, encode(me)?],
        )?;
    }
    if let Some((server, channel)) = &user.route {
        tx.execute(
            "UPDATE users SET active_server = ?2, active_channel = ?3 WHERE user_id = ?1",
            params![user_id, server, channel],
        )?;
    }
    if let Some(servers) = &user.servers {
        tx.execute("DELETE FROM servers WHERE user_id = ?1", [user_id])?;
        let mut insert =
            tx.prepare_cached("INSERT INTO servers (user_id, position, body) VALUES (?1, ?2, ?3)")?;
        for (position, server) in (0_i64..).zip(servers) {
            insert.execute(params![user_id, position, encode(server)?])?;
        }
    }
    for (server_id, tree) in &user.channels {
        tx.execute(
            "INSERT OR REPLACE INTO channel_trees (user_id, server_id, body) VALUES (?1, ?2, ?3)",
            params![user_id, server_id, encode(tree)?],
        )?;
    }
    if user.messages.is_empty() {
        return Ok(());
    }
    let mut insert = tx.prepare_cached(
        "INSERT INTO messages (user_id, channel_id, position, body, written_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (channel_id, messages) in &user.messages {
        tx.execute(
            "DELETE FROM messages WHERE user_id = ?1 AND channel_id = ?2",
            params![user_id, channel_id],
        )?;
        let newest = &messages[messages.len().saturating_sub(MESSAGE_WINDOW)..];
        for (position, message) in (0_i64..).zip(newest) {
            insert.execute(params![
                user_id,
                channel_id,
                position,
                encode(message)?,
                written_at
            ])?;
        }
    }
    // Only the most recently written channel windows stay.
    tx.execute(
        "DELETE FROM messages WHERE user_id = ?1 AND channel_id NOT IN (
             SELECT channel_id FROM messages WHERE user_id = ?1
             GROUP BY channel_id ORDER BY MAX(written_at) DESC LIMIT ?2)",
        params![user_id, CHANNEL_WINDOWS as i64],
    )?;
    Ok(())
}
