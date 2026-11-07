//! Snapshot cache tests on temp files: round trip, schema mismatch,
//! per-user isolation, clears, the message window and the writer thread.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;
use serde_json::json;
use tempfile::TempDir;
use yap_cache::{
    Batch, Cache, ChannelsResponse, Message, Server, Snapshot, User, VoicePrefs, Write, Writer,
    MESSAGE_WINDOW, SCHEMA_VERSION,
};

const ALICE: &str = "user_alice";
const BOB: &str = "user_bob";

fn user(id: &str) -> User {
    serde_json::from_value(json!({ "id": id, "username": id, "display_name": "Name" }))
        .expect("user")
}

fn server(id: &str) -> Server {
    serde_json::from_value(json!({ "id": id, "name": format!("Server {id}") })).expect("server")
}

fn tree() -> ChannelsResponse {
    serde_json::from_value(json!({
        "channels": [
            { "id": "chan_text", "server_id": "srv", "name": "general",
              "channel_type": "text", "category_id": "cat", "position": 0 },
            { "id": "chan_voice", "server_id": "srv", "name": "lounge",
              "channel_type": "voice", "position": 1 },
        ],
        "categories": [{ "id": "cat", "server_id": "srv", "name": "Text", "rank": 0 }],
    }))
    .expect("tree")
}

fn message(channel: &str, n: usize) -> Message {
    serde_json::from_value(json!({
        "id": format!("msg_{n:04}"),
        "channel_id": channel,
        "author_id": ALICE,
        "author": { "id": ALICE, "username": "alice" },
        "content": format!("message {n}"),
        "created_at": "2026-11-01T12:00:00.000Z",
        "embeds": [{ "type": "link" }],
        "type": "default",
    }))
    .expect("message")
}

fn temp() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("nested").join("cache").join("snapshot.db");
    (dir, path)
}

/// A whole snapshot for `user_id`, written in one batch at `at`.
fn write_snapshot(cache: &mut Cache, user_id: &str, at: i64) {
    let mut batch = Batch::default();
    batch.push(user_id, Write::Me(Box::new(user(user_id))));
    batch.push(
        user_id,
        Write::Servers(vec![server("srv"), server("srv_two")]),
    );
    batch.push(
        user_id,
        Write::Channels {
            server_id: "srv".into(),
            tree: tree(),
        },
    );
    batch.push(
        user_id,
        Write::Route {
            server_id: Some("srv".into()),
            channel_id: Some("chan_text".into()),
        },
    );
    batch.push(
        user_id,
        Write::Messages {
            channel_id: "chan_text".into(),
            messages: (0..3).map(|n| message("chan_text", n)).collect(),
        },
    );
    cache.apply_at(batch, at).expect("apply");
}

fn load(cache: &Cache, user_id: &str) -> Option<Snapshot> {
    cache.load(user_id).expect("load")
}

#[test]
fn open_should_create_the_folders_and_the_schema() {
    let (_dir, path) = temp();
    let cache = Cache::open(&path).expect("open");
    assert_eq!(cache.schema_version().expect("version"), SCHEMA_VERSION);
    assert!(path.exists());
}

#[test]
fn open_should_use_wal() {
    let (_dir, path) = temp();
    let _cache = Cache::open(&path).expect("open");
    let conn = Connection::open(&path).expect("raw");
    let mode: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("mode");
    assert_eq!(mode, "wal");
}

#[test]
fn a_snapshot_should_round_trip_through_the_file() {
    let (_dir, path) = temp();
    let mut cache = Cache::open(&path).expect("open");
    write_snapshot(&mut cache, ALICE, 1_000);
    drop(cache);

    let cache = Cache::open(&path).expect("reopen");
    let snapshot = load(&cache, ALICE).expect("snapshot");
    assert_eq!(
        snapshot,
        Snapshot {
            me: user(ALICE),
            servers: vec![server("srv"), server("srv_two")],
            channels: vec![("srv".into(), tree())],
            active_server: Some("srv".into()),
            active_channel: Some("chan_text".into()),
            messages: (0..3).map(|n| message("chan_text", n)).collect(),
            written_at_ms: 1_000,
            voice: None,
            scroll_positions: Vec::new(),
        }
    );
}

#[test]
fn voice_prefs_should_round_trip_with_the_snapshot() {
    let (_dir, path) = temp();
    let mut cache = Cache::open(&path).expect("open");
    write_snapshot(&mut cache, ALICE, 1_000);
    let prefs = VoicePrefs {
        muted: true,
        deafened: true,
        muted_before_deafen: false,
    };
    let mut batch = Batch::default();
    batch.push(ALICE, Write::Voice(prefs));
    cache.apply_at(batch, 2_000).expect("apply");
    drop(cache);
    let cache = Cache::open(&path).expect("reopen");
    assert_eq!(load(&cache, ALICE).expect("snapshot").voice, Some(prefs));
}

#[test]
fn a_schema_mismatch_should_drop_and_recreate_the_cache() {
    let (_dir, path) = temp();
    let mut cache = Cache::open(&path).expect("open");
    write_snapshot(&mut cache, ALICE, 1_000);
    drop(cache);
    let conn = Connection::open(&path).expect("raw");
    conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .expect("bump");
    conn.execute_batch("CREATE TABLE legacy (x INTEGER);")
        .expect("legacy");
    drop(conn);

    let cache = Cache::open(&path).expect("reopen");
    assert_eq!(cache.schema_version().expect("version"), SCHEMA_VERSION);
    assert_eq!(load(&cache, ALICE), None);
    assert_eq!(cache.latest().expect("latest"), None);
}

#[test]
fn a_file_that_is_not_a_database_should_be_replaced() {
    let (_dir, path) = temp();
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(
        &path,
        b"this is not sqlite, just some bytes padding it out ..........",
    )
    .expect("garbage");
    let cache = Cache::open(&path).expect("open");
    assert_eq!(cache.latest().expect("latest"), None);
}

#[test]
fn snapshots_should_be_kept_per_user() {
    let mut cache = Cache::open_in_memory().expect("open");
    write_snapshot(&mut cache, ALICE, 1_000);
    let mut batch = Batch::default();
    batch.push(BOB, Write::Me(Box::new(user(BOB))));
    batch.push(BOB, Write::Servers(vec![server("srv_bob")]));
    cache.apply_at(batch, 2_000).expect("apply");

    let alice = load(&cache, ALICE).expect("alice");
    let bob = load(&cache, BOB).expect("bob");
    assert_eq!(alice.servers.len(), 2);
    assert_eq!(bob.servers, vec![server("srv_bob")]);
    assert!(bob.messages.is_empty() && bob.channels.is_empty());
}

#[test]
fn latest_should_be_the_most_recently_written_user() {
    let mut cache = Cache::open_in_memory().expect("open");
    write_snapshot(&mut cache, ALICE, 2_000);
    write_snapshot(&mut cache, BOB, 1_000);
    let latest = cache.latest().expect("latest").expect("snapshot");
    assert_eq!(latest.me.id, ALICE);
}

#[test]
fn a_user_without_me_should_have_no_snapshot() {
    let mut cache = Cache::open_in_memory().expect("open");
    let mut batch = Batch::default();
    batch.push(ALICE, Write::Servers(vec![server("srv")]));
    cache.apply_at(batch, 1_000).expect("apply");
    assert_eq!(load(&cache, ALICE), None);
}

#[test]
fn clear_should_wipe_only_that_user() {
    let mut cache = Cache::open_in_memory().expect("open");
    write_snapshot(&mut cache, ALICE, 1_000);
    write_snapshot(&mut cache, BOB, 2_000);
    cache.clear(BOB).expect("clear");
    assert_eq!(load(&cache, BOB), None);
    assert!(load(&cache, ALICE).is_some());
    assert_eq!(
        cache.latest().expect("latest").map(|s| s.me.id),
        Some(ALICE.to_owned())
    );
}

#[test]
fn a_clear_should_drop_the_writes_queued_before_it() {
    let mut cache = Cache::open_in_memory().expect("open");
    let mut batch = Batch::default();
    batch.push(ALICE, Write::Me(Box::new(user(ALICE))));
    batch.clear(ALICE);
    cache.apply_at(batch, 1_000).expect("apply");
    assert_eq!(load(&cache, ALICE), None);
}

#[test]
fn the_message_window_should_keep_the_newest() {
    let mut cache = Cache::open_in_memory().expect("open");
    write_snapshot(&mut cache, ALICE, 1_000);
    let mut batch = Batch::default();
    batch.push(
        ALICE,
        Write::Messages {
            channel_id: "chan_text".into(),
            messages: (0..MESSAGE_WINDOW + 20)
                .map(|n| message("chan_text", n))
                .collect(),
        },
    );
    cache.apply_at(batch, 2_000).expect("apply");
    let messages = load(&cache, ALICE).expect("snapshot").messages;
    assert_eq!(messages.len(), MESSAGE_WINDOW);
    assert_eq!(messages[0].id, "msg_0020");
    assert_eq!(messages[MESSAGE_WINDOW - 1].id, "msg_0119");
}

#[test]
fn later_writes_of_a_key_should_replace_earlier_ones() {
    let mut cache = Cache::open_in_memory().expect("open");
    let mut batch = Batch::default();
    batch.push(ALICE, Write::Me(Box::new(user(ALICE))));
    batch.push(ALICE, Write::Servers(vec![server("old")]));
    batch.push(ALICE, Write::Servers(vec![server("new")]));
    cache.apply_at(batch, 1_000).expect("apply");
    assert_eq!(
        load(&cache, ALICE).expect("snapshot").servers,
        vec![server("new")]
    );
}

fn reopen(path: &Path) -> Cache {
    Cache::open(path).expect("reopen")
}

#[test]
fn the_writer_should_commit_on_flush() {
    let (_dir, path) = temp();
    let writer = Writer::spawn(Duration::from_secs(60)).expect("writer");
    writer.attach(Some(Cache::open(&path).expect("open")));
    assert!(writer.write(ALICE, Write::Me(Box::new(user(ALICE)))));
    assert!(writer.flush(Duration::from_secs(5)));
    assert_eq!(
        reopen(&path).latest().expect("latest").map(|s| s.me.id),
        Some(ALICE.to_owned())
    );
}

#[test]
fn the_writer_should_keep_writes_queued_before_attach() {
    let (_dir, path) = temp();
    let writer = Writer::spawn(Duration::from_millis(10)).expect("writer");
    writer.write(ALICE, Write::Me(Box::new(user(ALICE))));
    writer.attach(Some(Cache::open(&path).expect("open")));
    assert!(writer.flush(Duration::from_secs(5)));
    assert!(reopen(&path).load(ALICE).expect("load").is_some());
}

#[test]
fn the_writer_should_commit_within_its_interval() {
    let (_dir, path) = temp();
    let writer = Writer::spawn(Duration::from_millis(20)).expect("writer");
    writer.attach(Some(Cache::open(&path).expect("open")));
    writer.write(ALICE, Write::Me(Box::new(user(ALICE))));
    let reader = reopen(&path);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while reader.load(ALICE).expect("load").is_none() {
        assert!(std::time::Instant::now() < deadline, "never committed");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn the_writer_should_clear_at_once() {
    let (_dir, path) = temp();
    let mut cache = Cache::open(&path).expect("open");
    write_snapshot(&mut cache, ALICE, 1_000);
    let writer = Writer::spawn(Duration::from_secs(60)).expect("writer");
    writer.attach(Some(cache));
    writer.clear(ALICE);
    let reader = reopen(&path);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while reader.load(ALICE).expect("load").is_some() {
        assert!(std::time::Instant::now() < deadline, "never cleared");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn the_writer_should_commit_when_dropped() {
    let (_dir, path) = temp();
    let writer = Writer::spawn(Duration::from_secs(60)).expect("writer");
    writer.attach(Some(Cache::open(&path).expect("open")));
    writer.write(ALICE, Write::Me(Box::new(user(ALICE))));
    drop(writer);
    let reader = reopen(&path);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while reader.load(ALICE).expect("load").is_none() {
        assert!(std::time::Instant::now() < deadline, "never committed");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_writer_without_a_database_should_drop_writes() {
    let writer = Writer::spawn(Duration::from_millis(10)).expect("writer");
    writer.attach(None);
    writer.write(ALICE, Write::Me(Box::new(user(ALICE))));
    assert!(writer.flush(Duration::from_secs(5)));
}

#[test]
fn scroll_positions_should_round_trip_and_replace_per_channel() {
    let (_dir, path) = temp();
    let mut cache = Cache::open(&path).expect("open");
    write_snapshot(&mut cache, ALICE, 1_000);
    let mut batch = Batch::default();
    for (channel, at) in [
        ("chan_text", "msg_1"),
        ("chan_two", "BOTTOM"),
        ("chan_text", "msg_2"),
    ] {
        batch.push(
            ALICE,
            Write::Scroll {
                channel_id: channel.into(),
                message_id: at.into(),
            },
        );
    }
    cache.apply_at(batch, 2_000).expect("apply");
    drop(cache);

    let cache = Cache::open(&path).expect("reopen");
    let mut positions = load(&cache, ALICE).expect("snapshot").scroll_positions;
    positions.sort();
    assert_eq!(
        positions,
        vec![
            ("chan_text".to_owned(), "msg_2".to_owned()),
            ("chan_two".to_owned(), "BOTTOM".to_owned()),
        ]
    );
}
