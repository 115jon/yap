# Native Yap desktop client: architecture review and target design

Status: review of `spike/qt-desktop` at `d046e32a`, 2026-11-04. Analysis only;
no code changed. Paths are relative to `desktop-native/` unless they start
with `packages/` or `src/` (repo root).

The prototype does what it set out to prove: the CMake + Corrosion + cxx-qt +
static Qt build works, and sign-in, live chat and two-way voice run against
production (see [FEASIBILITY.md](../FEASIBILITY.md)). The goal is now parity
with the Tauri + CEF app, which means going from 1 screen to about 50
surfaces. The shortcuts that made the spike fast will make each new surface
expensive. This document ranks five changes by how much later parity work
they make cheaper, then gives a target layout, a model strategy, a
performance budget and the first three PRs.

Facts below cite `file:line`. Inferred costs are marked **(inferred)**. Nothing
here was measured beyond the numbers already in FEASIBILITY.md.

## Ranked candidates

| #   | Candidate                                                                                 | Strength        |
| --- | ----------------------------------------------------------------------------------------- | --------------- |
| 1   | Split `Core` into a pure state store (reducer) plus an effect runner                      | Strong          |
| 2   | Replace JSON-string properties and the QML `ListModel` with Rust-backed models per domain | Strong          |
| 3   | One realtime connection module, with typed ops shared with `packages/protocol`            | Strong          |
| 4   | Local snapshot cache and a parallel boot for instant startup                              | Worth exploring |
| 5   | Break the voice session into a state machine, a track table and a media adapter           | Worth exploring |

1 and 2 are two halves of one seam (state on the tokio side, projections on
the Qt side). They are ranked separately because each can land alone, and 1
must come first: the models in 2 need something to project.

---

### 1. A pure state store plus an effect runner (replace the `Core` god struct)

**Evidence**

- `rust/src/client/mod.rs:126-140`: `Core` holds every piece of state
  (session, servers, channels, active ids, names, voice) as `Vec<Value>` and
  `Option<String>`. There are no typed entities.
- `mod.rs:225-236`: one `while let Some(command) = rx.recv().await` loop.
  `handle` awaits network calls inline, so every REST call blocks every other
  command _and_ every gateway event, because gateway events arrive on the same
  channel as `Command::Event` (`gateway.rs:122,127`).
  - `select_server` awaits channels, then `select_channel`, then
    `load_channel_page` (`mod.rs:456-555`): three serial round trips at
    0.3-0.8 s each while dispatches queue up behind them.
  - `send` awaits the POST (`mod.rs:589-607`). While a send is in flight, no
    `MESSAGE_CREATE`, typing or voice update is applied **(inferred: visible
    as UI stalls under a slow API)**.
  - A reconnect refetch also awaits inside the event handler
    (`mod.rs:631-635`).
- Staleness is handled with ad-hoc re-checks after each await
  (`mod.rs:461`, `mod.rs:562`); each new async flow has to remember to add one.
- `mod.rs:798`: the core enqueues `Command::ReloadServers` to itself to get
  around the loop.
- Errors collapse into one status string (`mod.rs:228-233`,
  `bridge.rs:192`); voice errors use the same string (`mod.rs:748`).
- `on_dispatch` (`mod.rs:755-802`) is the start of a port of
  `src/lib/chat-reducer.ts` (92 KB on the web), but it is not testable: it
  writes straight to the UI sink and reads `self.session`.
- Blocking calls on runtime threads: `auth::load_token` / `store_token`
  (Windows Credential Manager through `keyring`, `auth.rs:136-152`) run inside
  async handlers (`mod.rs:240,335`).
- Tests: none in the crate.

**Problem.** All state, all I/O and all sequencing live in one module whose
interface is "a channel of commands", so there is no seam to test behaviour
through, and every REST await serialises the whole client.

**Target design**

- **`yap-core::store`, a pure reducer.** `State` holds normalised, typed
  entities: `HashMap<Id, Server>`, channels by server, a `MessageLog` per
  channel (ordered, windowed), members, presence, read states, the voice
  summary. `fn reduce(&mut State, Input) -> Effects`, where `Input` is a user
  intent, a gateway dispatch or an effect result, and `Effects` lists:
  - I/O to start (`Http(RequestId, ApiRequest)`, `GatewaySend(ClientOp)`,
    `Persist(...)`);
  - `ViewDiff`s for the Qt side (candidate 2).

  No `await`, no clock, no `Value` inside. This is where the
  `chat-reducer.ts` behaviour gets ported, rule by rule, with its tests.

- **`yap-core::runtime`, the effect runner.** It owns the `Api`, the realtime
  connections and the cache writer. It spawns each HTTP effect as its own task
  (bounded by a `Semaphore`). Results come back as
  `Input::HttpDone(RequestId, Result<..>)`.
  - Staleness is one rule: the store keeps the request id it is waiting for
    per slot (channel page, server channels) and ignores others. That replaces
    the re-checks.
- **State ownership.** The store is owned by one task (an actor). Nothing
  else holds a reference: the Qt side gets diffs, the runner gets effects.
  There are no locks, so nothing is held across an `.await`.
- **Errors.** `thiserror` enums per crate (`ApiError::{Unauthorized,
Http{status, code}, Network, Decode}`, `RealtimeError`, `VoiceError`), with
  `anyhow` only at the app edge. The store turns failures into typed
  `Notice`s (a notices model), not one shared `status` string.
- **Blocking work** (`keyring`, SQLite, image decoding) goes through
  `spawn_blocking` or a dedicated thread.
- **Testing seam.** `reduce` is the test surface:
  - feed recorded gateway frames and REST fixtures, assert on `State` and
    `Effects`;
  - no network, no Qt, no runtime;
  - port `src/lib/chat-reducer.test.ts` cases as table tests.

**Migration**

1. Introduce typed entities (`yap-protocol`) and `State`, filled from the same
   JSON. Keep `Ui` output identical.
2. Move `on_dispatch` and the select/load flows into `reduce`, emitting
   effects. The runner executes them. Delete the inline awaits.
3. Delete the `Command::Event` and `ReloadServers` self-loop.

**Risk and blast radius.** Everything in `client/mod.rs` changes; `bridge.rs`
keeps its interface during the move. Likely regressions:

- ordering (a page response arriving after a newer dispatch): handle it with
  per-channel upsert by id and `created_at`;
- forgotten staleness slots.

Fixture tests catch both.

---

### 2. Rust-backed list models and per-domain QObjects (replace JSON over QString)

**Evidence**

- `bridge.rs:19-27,198-217`: `me`, `servers`, `channels` and the voice roster
  cross the bridge as JSON strings (`Value::to_string`), and messages as JSON
  signals (`bridge.rs:74-86,206-209`).
- QML parses them again on every change:
  - `Main.qml:22`; `ServerRail.qml:11`;
  - `ChannelSidebar.qml:11,14,31,35`: `channelsJson` is parsed twice
    (`rows` and `callChannelName`);
  - the server list is parsed in two components.
- **Speaking-LED path (inferred cost):**
  1. Each speaking flag change in a call sends a whole roster (`voice.rs:388`,
     `voice.rs:395-398`).
  2. The core clones it and pushes the whole voice state (`mod.rs:730-733,752`).
  3. The bridge serialises it (`bridge.rs:217`).
  4. `ChannelSidebar.qml:31` parses it, and `rows` (`:13-29`) recomputes,
     parsing `channelsJson` again.
  5. It builds a new JS array, and `model: root.rows` (`:111`) replaces the
     ListView model, so every sidebar delegate is destroyed and recreated.

  That happens several times a second while anyone talks.

- `ChatView.qml:16-99` is a second reducer in JS:
  - a `ListModel`;
  - `indexOf` is a linear scan per upsert or remove (`:39-44`);
  - grouping is computed in JS (`:18-23,46-49`);
  - partial-update patching is split between Rust (`mod.rs:767-776`) and QML
    (`ChatView.qml:65-82`).
- `MessageDelegate.qml:83-90`: each row is a `TextEdit` with
  `TextEdit.MarkdownText`. That means one `QTextDocument` and one Markdown
  parse on the GUI thread per created delegate. Timestamps are formatted in
  JS per delegate (`:19-33`).
- Stringly typed state:
  - `phase` values (`mod.rs:242,280,320,372`) compared as strings in QML
    (`Main.qml:27,71,78`);
  - `voiceStatus === "Connected"` (`ChannelSidebar.qml:33`) depends on the
    exact English text set at `voice.rs:446`.
- One god object: `YapClient` has 15 properties, 11 invokables and 4 signals
  (`bridge.rs:11-90`). Every surface would add to it.

**Problem.** The interface between core and UI is the size of the whole app
and untyped. Each list change costs a full serialise, parse and rebuild. The
JS side duplicates reducer logic that Rust already has.

**Target design**

- **Models are Rust `QAbstractListModel` subclasses through cxx-qt**
  (`#[base = QAbstractListModel]`, `#[inherit]` for
  `beginInsertRows` / `endInsertRows`, `beginRemoveRows`, `dataChanged`,
  `beginResetModel`; `#[cxx_override]` for `rowCount`, `data` and `roleNames`;
  see cxx-qt's `custom_base_class` example).
  - Each holds a `Vec<Arc<RowT>>` projection (display-ready rows) and applies
    `ViewDiff`s.
  - Planned models: `ServerListModel`, `ChannelTreeModel` (flattened rows:
    label, channel, member), `MessageListModel`, `VoiceRosterModel`,
    `MemberListModel`, `NoticeModel`. Later: DMs, search results, emoji.
- **Diffs, not snapshots.** The store emits
  `ViewDiff::{Reset, Insert{at, rows}, Remove{at, n}, Update{at, roles}, Move}`
  per model id.
  - A speaking flip becomes `Update{at: i, roles: [Speaking]}`: one
    `dataChanged` and one delegate repaint.
- **Threading.** The store and diffs live on tokio; models and QObjects live
  on the GUI thread.
  - The bridge drains diffs through `CxxQtThread::queue`, **coalesced**: the
    core batches diffs and flushes at most once per ~8 ms, or right away when
    idle. That keeps a burst of dispatches from turning into hundreds of
    queued closures.
  - `data()` never locks or crosses threads; it reads the GUI-side `Vec`.
- **Per-domain QObjects** registered as QML singletons (`#[qml_singleton]`):
  `Session` (phase enum, me, sign-in), `Nav` (active server and channel,
  routes), `Chat` (send, edit, delete, `loadOlder`, the `messages` model),
  `Servers`, `Voice` (state enum, mute, deafen, the roster model), `Notices`.
  - Each is small, and each forwards intents to one command sender.
  - Enums cross as `#[qenum]`, not strings.
- **Display work moves off the delegate:**
  - Rust pre-renders message bodies with `pulldown-cmark` to the Qt rich-text
    subset (sanitised; only `http(s)` and `mailto` links) and caches by
    `(id, edited_at)`;
  - Rust computes `grouped`, `dayDivider` and a formatted timestamp as roles;
  - the delegate uses `Text { textFormat: Text.RichText }`. A selectable
    `TextEdit` is swapped in only on hover or selection.
- **Paging.** `MessageListModel` holds a window (for example the 200 newest,
  growing to 2-3k with older pages) per open channel.
  - Use `ListView { verticalLayoutDirection: ListView.BottomToTop }` with
    index 0 as the newest message. New messages insert at 0, and older pages
    append at the end with no scroll jumps.
  - At the top (`atYEnd` in the flipped view), `Chat.loadOlder()` emits an
    `Http` effect with `before=`.
  - Rows that scroll far out of view are evicted and refetched from the cache.
- **Testing seam.**
  - Model diff application is unit-testable in Rust (diff to row-vector
    state) without Qt.
  - QML components take models as `required property` and can be tested with
    Qt Quick Test against a mock or `ListModel` stand-in.

**Migration**

1. Add `MessageListModel` behind the existing signals and delete the
   `ChatView.qml` JS reducer.
2. `VoiceRosterModel` and `ChannelTreeModel`; delete `voiceRosterJson` and
   `channelsJson`.
3. Split `YapClient` into domain singletons. Keep one module URI.

**Risk and blast radius.**

- cxx-qt 0.10 `QAbstractListModel` on static Qt 6.6.3 is unproven here. Prove
  it with one model in PR 3 before converting the rest.
- Likely regressions: off-by-one in `begin*Rows` ranges (crash or assert in a
  debug Qt); scroll anchoring with `BottomToTop`.
- QML files all change.

---

### 3. One realtime connection module and typed protocol shared with `packages/protocol`

**Evidence**

- Two copies of socket plumbing:
  - URL scheme swap and ticket subprotocol headers: `gateway.rs:76-86` and
    `socket.rs:35-45`;
  - the `tick` helper: `gateway.rs:57-64` and `socket.rs:52-59`;
  - default heartbeat: 45 s in `gateway.rs:105`, 15 s in `socket.rs:80`.
- Reconnect policy is split:
  - the gateway has backoff without jitter (`gateway.rs:30-54`);
  - room and voice sockets have **none** (`socket.rs:1-3`), so any network
    blip ends the call (`voice.rs:375-376,436-439`).
- **No Resume:**
  - `seq` is dropped when dispatches are forwarded (`gateway.rs:124-127`);
  - Ready's session id is ignored (`gateway.rs:119-123`);
  - the core refetches the open channel after every reconnect
    (`mod.rs:629-635`). That loses events for every channel that is not open
    (unread counts, mentions, presence).
  - The server already supports Resume (`packages/protocol/src/opcodes.ts:30-35`,
    `client-payloads.ts:60-63` `{session_id, seq_ack}`; `codec.ts:70-82`
    `readReplaySeq`; close code 4006 `SessionInvalid`, `opcodes.ts:174`).
- Heartbeat ACK (op 6) is ignored or filtered (`socket.rs:88`; the gateway
  does not track it), so a half-open TCP connection is never detected
  **(inferred)**.
- Close codes are classified inline and only in voice (`voice.rs:428-439`).
  The gateway treats every close the same way (`gateway.rs:97`).
- Magic numbers: ops 0, 2, 3, 8, 9, 12, 13, 15, 18, 19, 27, 28, 33, 34, 35,
  40, 100, 101 appear as literals across `mod.rs`, `gateway.rs`, `socket.rs`
  and `voice.rs`. `packages/protocol/src/opcodes.ts` is the TS source of truth
  (`GatewayOp`, `GatewayCloseCode`).
- `outgoing` frames are `String` (`gateway.rs:27`), so nothing type-checks
  what the client sends.

**Problem.** Connection lifecycle (heartbeat, liveness, backoff, resume,
close-code policy) is a cross-cutting concern written twice and incompletely,
and the protocol is restated as literals the compiler cannot check against
the web's definitions.

**Target design**

- **`yap-protocol` crate:**
  - `#[repr(u8)] enum GatewayOp`, `enum CloseCode`;
  - typed `ClientFrame` and `ServerFrame` (serde, `#[serde(tag="op",
content="d")]` where it fits; a hand-written `Deserialize` for dispatches
    that reads `seq` first, the way `readReplaySeq` does);
  - REST DTOs (`User`, `Server`, `Channel`, `Message`, `MessagePage`).
  - No I/O.
- **Sharing with the web.** Do not generate Rust from TS today. The TS
  schemas are Effect Schema and still moving with the web voice refactor.
  Instead:
  1. **Shared golden fixtures.** Add `packages/protocol/fixtures/*.json`
     (frames and REST bodies). Both `vitest` and `cargo test` decode every
     fixture.
  2. **An opcode parity test.** A `cargo test` reads `opcodes.ts`, extracts
     `Name = N` pairs from `GatewayOp` and `GatewayCloseCode`, and asserts the
     Rust enums match. It runs in CI when either side changes.
  3. Later, if drift shows up: export JSON Schema from Effect Schema
     (`JSONSchema.make`) and generate Rust with `typify`.

  Coordinate with the web voice refactor so the voice-socket ops and
  `media_bootstrap` shapes it settles land as fixtures here too. The Tauri app
  (`desktop/src-tauri`) can depend on the same crate for its native
  screen-share ops.

- **`yap-realtime` crate, `Connection`.** One deep module owns connect
  (ticket fetch, URL, `Origin`, subprotocols), Hello, Identify or Resume,
  heartbeat **with an ACK deadline** (two missed means reconnect), and a
  backoff with full jitter that resets after Ready.
  - It also owns the `seq_ack` cursor, the reconnect, and a close-code
    policy table: 4006 → Identify again and emit `Resynced`; 4008 and 4010 →
    terminal; 4003/4004/401 → `SessionExpired`; 4290 → back off.
  - Its interface: `Connection::spawn(Endpoint, IdentifyFn) -> (Sender<ClientFrame>,
Receiver<ConnEvent>)`, where `ConnEvent` is
    `Ready{resumed}`, `Frame`, `Down{retry_in}` or `Closed(Terminal)`.
  - The gateway, the room socket and the voice socket are three
    `Endpoint`/policy values, not three implementations.
  - When a resume fails (`Ready{resumed: false}` after a drop), the store
    marks every loaded channel stale and refetches lazily on view. That
    replaces the open-channel-only refetch.
- **Bounded channels.** Outgoing frames use bounded `mpsc` with backpressure;
  incoming frames go to the store actor.
- **Testing seam.** Run `Connection` against an in-process
  `tokio-tungstenite` server in tests. Script Hello, drop, a 4006 close, a
  missing ACK and replay, and assert on the `ConnEvent` sequence with
  `tokio::time::pause()`.

**Migration.** `yap-protocol` first (pure types plus fixtures), then
`Connection` replacing `gateway.rs`, then the room and voice sockets. The
voice sockets reconnect in place while the peer connection stays up.

**Risk and blast radius.**

- Resume semantics on the MeetingRoom DO (replay window, what the server
  replays to a native client) need checking against the worker code before
  relying on it.
- The voice room socket must not auto-rejoin after 4010 (one voice session
  per user per room).
- Regression mode: duplicate events after resume. Make reducers idempotent by
  id and `seq`.

---

#### As built: game activity (2026-11-08)

"Playing X", phase 2: the server, web and mobile side is phase 1
(`packages/protocol/src/game-activity.ts`, `realtime/meeting-room/game-activity.ts`).
The native client detects the game itself and reports it with op 43.

- **Detection** (`crates/yap-games`, no Qt, no network). One thread
  (`detector.rs`) keeps the answer and calls back only when it changes.
  - A scan is a ToolHelp snapshot: pids and executable names, no process
    opened. Only a pid not seen before, whose name the catalog lists (or
    the user added), is opened, with
    `PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE`, for its image path.
    The handle stays open so the exit wakes the thread. Nothing reads
    another process's memory or command line.
  - Matching (`matcher.rs`) follows arRPC's `src/process/index.js`: the
    lower-cased path with `/`, every suffix from the file name to the path
    without its drive, each also with the first `64`, `.x64`, `x64` or `_64`
    taken out; `>name` must equal the file name. Launchers and entries that
    need `arguments` are dropped at load. Two rules of ours: a file name
    several apps list (428 of them, `game.exe`, `hl2.exe`, ...) or a generic
    one (`SHARED_NAMES`) matches only through an entry with its folders; and
    a process that refuses the open (anti-cheat, elevated) matches only a
    folderless, unshared name one app lists. The deepest entry wins.
  - Ignored: launchers and their helpers (`IGNORED_PROCESSES`: Steam,
    Epic, Battle.net, EA, Ubisoft, GOG, Riot), anything under `%WINDIR%`,
    and Yap itself.
  - Triggers: `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT)`
    (a scan 2 s after the first change, at most one per 2 s), Steam's
    `HKCU\Software\Valve\Steam` `RunningAppID` through
    `RegNotifyChangeKeyValue`, a game process's exit (its `SYNCHRONIZE`
    handle in `MsgWaitForMultipleObjects`), and the poll: 20 s with no
    game, 60 s while one runs. With several games running, the one most
    recently in the foreground is reported.
  - Steam: a `RunningAppID` the catalog lists through its Steam ids is that
    app; one it does not is named from `appmanifest_<id>.acf` in the Steam
    libraries (`steam:<id>`). A process match wins over Steam, and takes
    the running Steam id when the app lists it.
- **Catalog** (`catalog.rs`). The web's `game-catalog.json` (Discord's
  detectable list, trimmed) read in one streaming pass into flat arrays:
  one string arena, the apps, the executables and a sorted array of
  file-name hashes. It is loaded on the first scan with detection on and
  freed when detection is turned off. The exe bundles it gzipped
  (`assets/games`, `tools/pack-game-catalog.mjs`; flate2 is linked
  already). `refresh.rs` asks `{YAP_API_URL}/desktop/game-catalog.json` at
  most daily with the last ETag (45 s after start, then hourly checks),
  keeps a good body in the profile's `game-catalog.json` and hands it to
  the detector; a failure or a body that does not parse (a page) keeps
  what there is, and a cache older than the bundle loses to it.
- **The store** (`store/reduce/games.rs`). `Presence::games` holds what
  others play, as chat-reducer.ts `gameActivityByUserId` does: a
  `PRESENCE_UPDATE` with `game_activity` sets or clears it, one without
  leaves it, offline or invisible clears it; a `PRESENCE_LIST` rebuilds it
  (an entry without the field keeps what was known). Ours
  (`Command::SetGameActivity` from the detector) is shown as others would
  see it: nothing while "Share my activity" (`share_activity` from
  users/me, `USER_PROFILE_UPDATE`, Settings > Privacy) is off or we are
  invisible. `sync_activity` runs after every input and sends op 43 only
  when that changes; Ready and Resumed restate it (`null` too) once
  anything was sent; a drop forgets what the socket was told.
- **The gate.** The deployed gateway answers an op it does not know with an
  Error frame (4001, `unknownOpcodeError`) and keeps the socket, and the
  client only logs Error frames, so a stray op 43 would not loop. Still,
  op 43 goes only to a server that says it takes it: `ActivityState::server`
  is set when the last `PRESENCE_LIST` entries carry `game_activity` (the
  phase 1 gateway always sends the field, `null` included; older ones never
  do, and the list arrives before Ready and after Resumed). A list without
  it turns sending off again. `YAP_GAME_ACTIVITY_SEND=off` never sends.
- **The UI.** Rows (MemberEntry, DmRow, FriendRow, the account row) show
  `GameLine.qml`, "Playing **Name**" with the 16 px icon, in the custom
  status's place, and never tick. Profile cards show `ProfileGame.qml`
  with the 40 px icon and the elapsed time from the one `GameClock`
  singleton, which runs only while a card that shows a game is visible.
  Icons are our `/api/game-icons/{app}/{hash}.png` through the image
  loader, else the Phosphor gamepad. Settings > App > Registered games
  (`GamesPanel.qml`, the `Games` QObject in `src/games.rs`): the detection
  switch, the games seen (last played, Hide) and Add it from the programs
  with a window, in the profile's `games.json` (yap-cache `GamesPrefs`).
- **Cost** (this PC: 24 threads, about 600 processes; `probe`):
  - The catalog: 628 KiB gzipped in the exe (the exe grew by 1,023,488
    bytes with the code and QML); loaded in 28-30 ms; 1,465 KiB kept on
    the heap, 2,816 KiB at the peak of the load.
  - A scan with nothing new: 13-16 ms, nearly all of it the ToolHelp
    snapshot (the kernel walks every thread of every process). At the
    20 s poll that is 0.08 % of one core, 0.003 % of the machine; over
    600 s (30 scans, the catalog load included) the thread used 547 ms,
    0.09 % of one core, 0.004 % of all 24. Foreground changes add at most
    one scan per 2 s while someone switches windows.
  - Opens: 104 in 600 s, all `QUERY_LIMITED_INFORMATION | SYNCHRONIZE`,
    of short-lived processes whose names some catalog entry lists.
- **Tests.** `cargo test -p yap-games` (matcher, catalog, refresh, the
  test switch), `cargo test -p yap-core --test games` (presence, sending
  on change, the restate, sharing off, invisible, the gate).
  `YAP_TEST_GAME_PROCESSES` stands in for the process list end to end.

### 4. Local snapshot cache and parallel boot (instant startup)

**Evidence**

- Startup is network-bound (FEASIBILITY.md, "Measured"): messages on screen
  after 1.5-3.2 s, and each API call takes 0.3-0.8 s server-side.
- The boot chain in `begin_session` (`mod.rs:318-375`) is serial:
  1. `me` and `servers` together (`:324`);
  2. then `apply_servers` → `select_server` → `GET channels` (`:460`);
  3. then `select_channel` → `GET messages` (`:560`).

  The gateway ticket is fetched only after `me` resolves (`:346` →
  `gateway.rs:75`). That makes at least three serial round trips before the
  first message.

- Nothing is persisted except the token (`auth.rs:132-156`). The last
  server and channel are not remembered: the client always opens the first
  server and the first text channel (`mod.rs:449,512-516`).
- FEASIBILITY.md already lists the fix ("render the last session's state from
  a local cache at once; prefetch the gateway ticket").

**Problem.** Every launch waits on the slowest serial chain of server calls,
because the client has no memory of the last session.

**Target design**

- **`yap-core::cache`**, SQLite (`rusqlite` with `bundled`, so it links
  statically) at `%LOCALAPPDATA%\Yap Native\cache\<user_id>.db`.
  - Tables: `me`, `servers`, `channels`, `messages` (the last ~200 per
    recently opened channel), `members`, `read_states`, and `kv` (last server
    and channel per server, window geometry, `schema_version`).
  - It is a cache: if the schema version does not match, delete it and start
    over.
- **Ownership and threading.** A single writer thread takes batched
  `Persist` effects from the store (write-behind, one transaction every
  ~250 ms or on idle). Reads happen once at boot.
  - Boot read is synchronous, before `engine.loadFromModule`, so the first
    frame already shows content. Budget: under 30 ms for the snapshot.
- **Parallel boot.** Read the cache, then render. At the same time, fetch the
  ticket, `me`, `servers`, the channels of the cached active server, and the
  newest messages of the cached active channel, all in flight together. The
  gateway connects as soon as the ticket arrives. Responses reconcile into
  the store as diffs.
- **Privacy.** Wipe the user's cache on sign-out and on `SessionExpired`.
  Never write the token there (it stays in Credential Manager). Message
  bodies at rest have the same exposure as the web's IndexedDB **(confirm
  whether the web caches messages; match its policy)**.
- **Testing seam.** Store snapshot round-trip tests (`State` → cache →
  `State`) use an in-memory SQLite.

**Migration.** Needs candidate 1's typed `State`. Add the read path first
(render from cache, then revalidate), then write-behind, then remember the
last route.

**Risk.** A stale cache shows deleted or forbidden content until
revalidation finishes. Mark cached views "syncing" and drop servers or
channels the user lost access to on the first `servers` response. A
corrupt-DB fallback is to delete and continue.

#### As built: switching channels and servers (2026-11-08)

The snapshot cache above serves the launch. Switching while the app runs
goes through a second, in-memory layer in the store
(`store/state.rs` `WarmCache`, `store/reduce/warm.rs`):

- **The warm cache.** The open channel's `MessageLog` is the only log on
  screen. When another channel opens, the log that was open moves into
  `State::warm`, an LRU of channels left recently. Each entry keeps:
  - the log, trimmed to its newest 100 messages (more when the channel was
    left scrolled up: down to the top message on screen, at most 400), with
    `has_more_before` set when anything was cut;
  - the gateway generation it was confirmed in (`State::gateway_generation`,
    bumped by every fresh Ready, not by a Resume);
  - when its page was asked for (epoch ms, from `Input::UserAt`);
  - its estimated size (`MessageLog::estimated_bytes`: the strings,
    attachments, reactions, embeds and one copy per author).

  Caps: 24 entries or 6 MB estimated, whichever comes first, checked when
  an entry is stored and when a live event grows one; the least recently
  used goes. The open channel is never in it (it is
  `State::messages`), and the call's channel and the open voice view's are
  never evicted. Pending sends are not stored (the outbox puts them back
  when the channel opens). A jump's window (`has_more_after`) is not the
  newest window and is not stored. Entries go when the channel is deleted,
  when a refetched tree no longer lists it (access lost), when the server
  goes (deleted or left), when a refetched DM list drops a DM, and on
  sign-out.

- **Stale-while-revalidate.** `open_messages` asks the warm cache first. A
  hit puts the rows on screen in the same reduce: `ScrollRestore` (where
  the channel was left, as `scrollPositions` keeps it), `Reset(rows)`, the
  "New" divider placed as `on_page` places it, and `PageLoading(false)`.
  Then the newest page is asked for anyway (`HttpReason::Revalidate`); its
  answer goes through `on_page`'s row diff (`ordered_diff`), so a refetch
  that changes nothing changes no row. The refetch is skipped only for a
  server channel confirmed in the current gateway session less than 30 s
  ago: server channels' message events reach every member of the server
  (the client subscribes to every server), DMs' edits and deletes and all
  reactions reach only a channel's subscribers. A miss, a pending jump into
  the channel, or a saved scroll position the warm log no longer holds
  takes the cold path: the empty list and its skeleton, the window kept on
  disk, and the page.
- **The clock.** The store still has none. The runtime stamps each command
  with the wall clock (`Input::UserAt { command, now_ms }`); `Input::User`
  (tests, tools) has no clock, and a warm entry is then never fresh.
- **Live events on warm logs.** `MESSAGE_CREATE` (at the newest end only),
  full `MESSAGE_UPDATE` (only a message already there), partial updates,
  `MESSAGE_DELETE` and reactions apply to a warm log as to the open one,
  and the changed log goes to disk through the batched writer.
- **The disk window.** On a miss, `Effect::LoadWindow` reads the channel's
  newest window from `snapshot.db` through a second, read-only SQLite
  connection (WAL, so the writer never blocks it). `Input::WindowLoaded`
  shows it when the page has not answered and the list is still empty;
  the page then revalidates it like the launch's cached rows.
- **The page merge.** A refetched newest page keeps the older rows already
  loaded only when it joins up with them: it shares a message with them, or
  its oldest message is not after their newest. A page entirely newer (more
  than a page arrived since they were confirmed) used to leave the messages
  between them missing while the list read as continuous; those rows are
  dropped now and paged in again on scroll.
- **Server switches.** A server whose tree is loaded (every server's is,
  for its unread pill) shows its tree, its members and its channel in the
  same batch; `GET /channels` revalidates the tree in the background and
  `on_channels` keeps the open channel. The channel is the text channel
  opened last in that server (`State::last_channels`, the web's
  `lastActiveChannels:<userId>`, with `@me` for the last DM), else the
  first text channel; it is kept in `snapshot.db` (`last_channels`, schema
  4, migrated in place from 3). The server already shown, chosen again
  (its rail key, the switcher, a refetched server list), keeps its open
  channel and only refetches the tree.
- **Prefetch.** A channel row the pointer rests on for 120 ms, and the
  quick switcher's highlighted channel or DM, send `PrefetchChannel`: the
  newest page, into the warm cache only, unless the channel is open, warm
  and fresh, or already asked for. At most 2 are in flight, and the runner
  answers one as busy at once while more than 4 of its 8 request slots are
  taken. A prefetched page stays in memory (the disk keeps the windows of
  channels opened), stamped with the gateway session it was asked in.
- **The details panels.** The pins of the 16 channels whose pins panel
  opened last (a pin event for one drops it) and the media, links and files
  tabs of the last 8 channel and tab pairs are kept the same way: shown at
  once when they open again, then asked for anew.
- **Traces.** `YAP_TRACE_HTTP=1` logs `[http] METHOD path reason=... N ms`
  for every request as it answers, at the one place they all pass
  (`Runner::run`); every switch logs `[switch] <channel> warm|disk|cold
rows=N <ms>` from the switch to its first rows.

---

### 5. Voice engine: state machine, track table, media adapter

**Evidence**

- `voice.rs:106-507`: `session` is about 400 lines in one function. It is
  ticket bootstrap, two socket opens, ADM setup, push negotiation, then a
  `select!` over 9 sources (`:336-497`) that mixes roster, track bookkeeping,
  mute, leave, stats and token refresh.
- Track state lives in four loosely coupled collections: `pending`, `pulled`,
  `mids` and `kept_tracks` (`voice.rs:280-296`). `retire` (`:546-562`) and
  the in-flight close (`:455-462`) keep them consistent by hand. Two recent
  fixes landed here (`671d6e3e`, receivers on stop or leave).
- A process-global `PeerConnectionFactory` and ADM (`voice.rs:43-46`), with
  `release_platform_adm` in `run` (`:102`). A second call overlapping a
  closing one shares the ADM **(inferred race)**.
- The roster goes out whole on every change (`:275,350,363,388,397,426,476`),
  which feeds the cost in candidate 2.
- `log_stats` runs `get_stats` and `eprintln!` every 5 s for the whole call
  (`:302,488-491,509-537`). It may account for part of the measured 7.2 %
  in-call CPU **(inferred, unmeasured)**.
- No reconnect for signaling or ICE (see candidate 3). No device selection,
  deafen, VAD or video, all of which are in the parity scope (FEASIBILITY.md
  §4: 3-5 weeks of voice audio, 4-6 weeks of video and screen share).

**Problem.** Every voice feature on the parity list (deafen, devices, video
tiles, screen share, soundboard, moves between channels) would be added to
the same function and the same four maps. The interesting logic cannot be
tested without a real SFU and sound card.

**Target design**

- **`yap-voice` crate** with three parts:
  - **`CallMachine`:** a pure state machine
    (`Bootstrapping → Signaling → Negotiating → Live ↔ Reconnecting → Ended(reason)`)
    taking `CallInput` (frames, SFU results, PC state, commands) and emitting
    `CallEffect`.
  - **`TrackTable`:** one type owning remote tracks:
    `announced → pulling → live{mid} → closing`, with `announce`, `stop`,
    `participant_left` and `pull_done` transitions. This replaces `pending`,
    `pulled`, `mids` and `kept_tracks`.
  - **`MediaEngine` trait:** create offer, apply answer, add or stop
    transceivers, device list and select, stats. It has two adapters:
    `LibWebRtcEngine` (production, owns the factory and ADM lifetime) and
    `FakeEngine` (tests). With video and screen share coming, the trait has
    real variation, not a hypothetical seam.
- **Threading.** One tokio task per call runs the machine. libwebrtc
  callbacks (track, state) post into its channel. Audio stays inside
  libwebrtc's own threads, and Qt never touches media. Video frames later go
  to Qt through a `QQuickItem` or `QVideoSink` bridge fed from the libwebrtc
  sink thread with a lock-free latest-frame slot; that is out of scope here.
- **Roster** is store state (candidate 1), sent as `Update` diffs.
- **Stats** are sampled only when a diagnostics panel is open, or every
  30-60 s at debug level through `tracing`.
- **Shared with the web.** The web voice refactor toward a shared protocol
  core should own the canonical join sequence. Capture it as recorded frame
  fixtures (`VOICE_PROTOCOL.md` §6) in `packages/protocol/fixtures/voice/`.
  Replay those through `CallMachine` with `FakeEngine` in `cargo test`, and
  through the TS core in vitest.

**Migration.** Extract `TrackTable` with tests first (no behaviour change),
then `MediaEngine` around the existing calls, then the state machine.

**Risk.** Voice regressions are only partly caught by fakes; keep the
headless-Edge peer smoke from FEASIBILITY.md as a manual gate. Blast radius:
`voice.rs`, `sfu.rs`, `socket.rs`.

---

## Recommended target layout

```
desktop-native/
  CMakeLists.txt              exe, static plugins, imports the bridge crate + QML module
  cpp/main.cpp                bootstrap only (cache-first boot hook, title bar)
  Cargo.toml                  [workspace], shared [workspace.lints], profiles
  crates/
    yap-protocol/             opcodes, close codes, frames, REST DTOs; serde only
      tests/fixtures.rs       decodes packages/protocol/fixtures/**, opcode parity with opcodes.ts
    yap-realtime/             Connection: ticket, heartbeat+ACK, resume(seq), backoff, close policy
    yap-core/                 store (reducer, State, ViewDiff), runtime (effect runner, Api),
                              cache (SQLite write-behind), auth (device flow, keyring)
    yap-voice/                CallMachine, TrackTable, MediaEngine + libwebrtc adapter
    yap-qt/                   cxx-qt bridge: domain singletons + QAbstractListModels; staticlib
      qml/                    (must stay under the crate dir, FEASIBILITY.md "Findings")
        Main.qml              ApplicationWindow + top-level Loader/StackView only
        theme/Theme.qml       singleton tokens
        components/           Avatar, IconButton, CallKey, ListRow, Badge, Toast, Markdown-free bits
        delegates/            MessageDelegate, ChannelRow, MemberRow, ServerIcon
        screens/              SignIn/, Shell/ (rail, tree, chat), Settings/, ServerAdmin/, Dms/, ...
        web/                  WebView2 host for long-tail routes (later)
  tools/
    fixtures/                 YAP_FIXTURE=chat-10k etc.: synthetic store states for perf and UI capture
```

Dependency direction:
`yap-qt → yap-core → {yap-realtime, yap-voice} → yap-protocol`.
Only `yap-qt` depends on cxx-qt. Everything below it builds and tests with
plain `cargo test`, without Qt or CMake, which is most of the code.

- **Threading summary:**
  - **GUI thread:** QML, the QObject singletons and the model row vectors.
  - **Tokio runtime (2-4 workers):** the store actor, the effect tasks and
    the connections.
  - **Dedicated threads:** the SQLite writer, plus libwebrtc's internal
    signaling, worker and audio threads.
  - **Crossings:** intents go GUI → store as an mpsc `Command`. Diffs go
    store → GUI through a coalesced `CxxQtThread::queue` flush, so there is
    one queued closure per frame-sized batch.
- **Error and reconnect policy:** typed errors per crate. Connection policy
  is a table inside `yap-realtime` (candidate 3). User-visible failures
  become `Notice` rows with a severity and an action, never a free-form
  status string.
- **QML module structure:** keep one module URI (`com.yap.native`), with files
  in sub-folders (`components/`, `delegates/`, `screens/`) listed in
  `build.rs`.
  - Screens load through `Loader { asynchronous: true; active: ... }`, so the
    ~50 surfaces cost nothing until opened.
  - `Main.qml` stays a bootstrap.
  - Components take data as `required property` and models, never a `client`
    object, so they can be tested and captured in isolation.
  - Use `ListView.reuseItems: true` (already on in `ChatView.qml:155`), and
    reset per-row state in `ListView.onPooled` / `onReused`.

## Model strategy (cxx-qt)

| model              | rows                                  | update pattern                                                        | paging                                            |
| ------------------ | ------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------- |
| `MessageListModel` | one per open channel (LRU of ~5)      | insert at 0 (new), `dataChanged` (edit, embed, reaction), remove      | `loadOlder()` appends `before=` pages; window cap |
| `ChannelTreeModel` | flattened label, channel, member rows | insert or remove members under a voice channel; `dataChanged` unread  | none                                              |
| `ServerListModel`  | servers                               | reset on `servers`; `dataChanged` unread or mention badges            | none                                              |
| `VoiceRosterModel` | members in the current call           | `dataChanged([Speaking])` per flip; insert or remove on join or leave | none                                              |
| `MemberListModel`  | server members, sectioned by role     | presence `dataChanged`                                                | range requests if servers get large               |
| `NoticeModel`      | toasts and banners                    | insert, expire                                                        | none                                              |

Rules:

- Rows are display-ready (`Arc<Row>` with pre-formatted strings and
  pre-rendered body HTML). `data()` is a match on role returning `QVariant`
  with no parsing.
- Role names are an enum declared once in Rust. QML delegates use
  `required property` per role.
- The store computes diffs with id → index maps (O(1) lookup), never a linear
  scan.
- Big resets (opening a channel) use `beginResetModel`. Steady state never
  resets.

## Performance budget

| metric                              | today (FEASIBILITY.md)    | budget                                                                 | how to measure                                                                                                                  |
| ----------------------------------- | ------------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| cold start, launch → first frame    | 480-730 ms (warm cache)   | ≤ 450 ms p50 warm; ≤ 900 ms first launch after reboot                  | existing `[startup] first frame` stderr line; 10 runs, report p50/p95; first-boot runs after a reboot                           |
| first content from cache            | n/a (network: 1.7-3.2 s)  | ≤ 600 ms after launch (same frame as, or one frame after, first frame) | new `[startup] cached content` marker on `frameSwapped` after the first non-empty `MessageListModel` reset                      |
| fresh content (network revalidated) | 1.7-3.2 s                 | ≤ 1.2 s p50 (one parallel round trip plus page)                        | `[startup] messages shown` (exists, `mod.rs:579-585`)                                                                           |
| gateway Ready                       | 1.9-3.7 s                 | ≤ 1.5 s p50                                                            | `[startup] gateway ready` (exists, `gateway.rs:121`)                                                                            |
| idle CPU, 60 s                      | 0.00-0.03 %               | ≤ 0.1 % of one core; 0 frames rendered while idle                      | existing Get-Process sampler; count `frameSwapped` over 60 s (debug counter)                                                    |
| idle RAM                            | 84 MB WS / 100 MB private | ≤ 120 MB WS, ≤ 150 MB private with 10 servers and 5 channels visited   | same sampler after a scripted tour (`YAP_FIXTURE` or a real account)                                                            |
| RAM per 1k loaded messages          | not measured              | ≤ 4 MB                                                                 | `chat-10k` fixture: private bytes delta before and after loading                                                                |
| scroll, 10k messages in a channel   | not measured              | 60 fps: no frame over 16.7 ms in a 10 s fling at 1080p; p99 ≤ 12 ms    | `chat-10k` fixture; `QSG_RENDER_TIMING=1` and the QML profiler (`qmlprofiler`, `-qmljsdebugger`); delegate creation ≤ 1 ms each |
| dispatch → visible                  | not measured              | ≤ 1 frame after the frame arrives; zero JS in the update path          | QML profiler: no JS on `MESSAGE_CREATE`; `tracing` span from frame received to diff flushed                                     |
| in-call CPU                         | 7.2 % (1 peer, Opus)      | ≤ 5 % at 1 peer; ≤ 9 % at 5 peers (audio only)                         | same sampler during a call with headless Edge peers (FEASIBILITY.md setup); compare with stats logging off                      |
| binary                              | 49.6 MB                   | ≤ 60 MB at parity (excluding WebView2)                                 | file size in CI                                                                                                                 |

Put the startup markers and the sampler in `tools/` as one script, so every
PR that touches boot or models can paste before-and-after numbers.

## First 3 PRs

1. **Workspace and `yap-protocol` with shared fixtures.**
   - Turn `rust/` into a cargo workspace (`crates/yap-protocol`, with the
     existing crate kept as `yap-qt` for now).
   - Add typed opcodes, close codes, frames and REST DTOs.
   - Add `packages/protocol/fixtures/` (gateway frames, Ready, dispatches,
     message pages) and the opcode parity test against `opcodes.ts`.
   - Replace the op literals in `gateway.rs`, `socket.rs`, `voice.rs` and
     `mod.rs` with the enums.
   - Add `[workspace.lints]` (clippy `-D warnings`) and a CI job running
     `cargo test -p yap-protocol` (no Qt needed).
   - No behaviour change.
   - _Makes cheap:_ every later feature starts from typed data and a fixture.
2. **`yap-core` store and effect runner.**
   - Move `Core` state into a typed `State` with `reduce(State, Input) ->
Effects`, and run HTTP effects as concurrent tasks with request-id
     staleness.
   - Port `on_dispatch` and the select/load and send flows.
   - Add reducer table tests from the fixtures, starting with the
     `chat-reducer.test.ts` cases for create, update (partial), delete and
     typing.
   - Move `keyring` calls to `spawn_blocking`.
   - Keep the current `Ui` output so the QML side is untouched.
   - _Makes cheap:_ each new dispatch or surface is a reducer arm plus a
     test, and REST no longer serialises the client.
3. **Rust models and domain singletons (`yap-qt`).**
   - Add `MessageListModel` (with `BottomToTop`, `loadOlder` paging and
     pre-rendered bodies), `ChannelTreeModel`, `ServerListModel` and
     `VoiceRosterModel` as cxx-qt `QAbstractListModel`s fed by coalesced
     `ViewDiff`s.
   - Split `YapClient` into the `Session`, `Nav`, `Chat`, `Servers` and
     `Voice` singletons.
   - Delete every `*Json` property and the `ChatView.qml` JS reducer.
   - Restructure `qml/` into `components/`, `delegates/` and `screens/`.
   - Add the `chat-10k` fixture and record the scroll and idle budget numbers
     in the PR.
   - _Makes cheap:_ a new surface is a model (or a role) plus a QML file in
     `screens/`.

Next, in order:

4. `yap-realtime` `Connection` with resume, ACK liveness and close policy,
   used by the gateway and then by the voice sockets.
5. Cache-first boot.
6. The `yap-voice` split, before deafen, devices and video.
