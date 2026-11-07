//! The effect runner and the store actor.
//!
//! One task (the actor) owns [`State`]. It takes [`Input`]s from a single
//! channel, runs [`reduce`], and hands each [`Effect`] to the [`Runner`].
//! The runner never awaits I/O itself: HTTP calls, sign-in, keyring access,
//! the gateway and voice calls run as their own tasks and report back as
//! inputs, so a slow request no longer holds up dispatches or other commands.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use yap_cache::{Cache, Snapshot};

use crate::api::{Api, Token};
use crate::auth::{self, TokenStore};
use crate::config;
use crate::fixture::Fixture;
use crate::store::{
    reduce, ApiResponse, CallId, Command, Dispatch, Effect, GatewayEvent, Input, LocalOffset,
    RequestId, SessionId, SignInEvent, SignInId, State, ViewUpdate, VoiceCommand, VoiceMember,
    VoiceTarget, VoiceUpdate,
};
use yap_protocol::rest::UploadedFile;

/// Where [`ViewUpdate`]s go, one batch at a time, in order (the bridge
/// queues each batch onto the Qt thread).
pub type Sink = Box<dyn Fn(Vec<ViewUpdate>) + Send + Sync>;

/// Inputs reduced back to back before the view batch is flushed. Bounds the
/// latency of a burst; an idle store flushes after every input.
const MAX_INPUTS_PER_BATCH: usize = 64;

/// HTTP requests in flight at once; more wait for a slot.
const MAX_HTTP_IN_FLIGHT: usize = 8;

/// The snapshot writer commits at most this often.
const CACHE_WRITE_INTERVAL: Duration = Duration::from_millis(500);

/// How long [`shutdown`] waits for the last snapshot write.
const CACHE_SHUTDOWN_WAIT: Duration = Duration::from_secs(1);

/// The snapshot writer, for [`shutdown`]. Set once the cache is opened.
static CACHE_WRITER: OnceLock<yap_cache::Writer> = OnceLock::new();

/// Commits the snapshot writes still pending. Call once on the way out
/// (it waits up to a second); a no-op without a cache.
pub fn shutdown() {
    if let Some(writer) = CACHE_WRITER.get() {
        if !writer.flush(CACHE_SHUTDOWN_WAIT) {
            eprintln!("[cache] last write did not finish before exit");
        }
    }
}

/// Opens the cache and reads the newest snapshot; any failure is "no cache".
fn read_snapshot(path: &std::path::Path) -> (Option<Cache>, Option<Snapshot>) {
    let started = Instant::now();
    let cache = match Cache::open(path) {
        Ok(cache) => cache,
        Err(error) => {
            eprintln!("[cache] {error}; starting without it");
            return (None, None);
        }
    };
    let snapshot = cache.latest().unwrap_or_else(|error| {
        eprintln!("[cache] {error}; ignoring the snapshot");
        None
    });
    let ms = started.elapsed().as_millis();
    eprintln!("[cache] snapshot read in {ms} ms");
    (Some(cache), snapshot)
}

/// When the core was started, for startup timing in the log.
pub fn started_at() -> &'static Instant {
    static STARTED: OnceLock<Instant> = OnceLock::new();
    STARTED.get_or_init(Instant::now)
}

/// The realtime pieces that still live in the app crate (gateway socket and
/// voice calls). They report through the event handles they are given.
pub trait Realtime: Send + 'static {
    /// Starts the gateway for a session. It runs (and reconnects) until the
    /// returned task is aborted; frames sent on `outgoing` go out as text.
    fn spawn_gateway(&self, api: Api, user_id: String, events: GatewayEvents) -> GatewayHandle;

    /// Starts one voice call. It runs until `Leave`, until `commands` closes,
    /// or until it fails, and always ends with `VoiceUpdate::Ended`.
    fn spawn_voice(
        &self,
        api: Api,
        http: reqwest::Client,
        target: VoiceTarget,
        commands: UnboundedReceiver<VoiceCommand>,
        events: VoiceEvents,
    );

    /// Starts the local player (soundboard clips, the Listen Together
    /// stream) once; its reports go to `events`. The default has no player.
    fn spawn_audio(&self, http: reqwest::Client, events: AudioEvents) -> AudioHandle {
        let _ = (http, events);
        AudioHandle::none()
    }
}

/// Commands for the local player, each with the session for our media.
pub struct AudioHandle {
    pub commands: Option<UnboundedSender<(Option<Token>, crate::store::apps::AudioCommand)>>,
}

impl AudioHandle {
    pub fn none() -> Self {
        Self { commands: None }
    }
}

/// How the local player reports to the store.
#[derive(Clone)]
pub struct AudioEvents {
    inbox: UnboundedSender<Input>,
}

impl AudioEvents {
    pub fn send(&self, event: crate::store::apps::AudioEvent) {
        let _ = self.inbox.send(Input::Audio(event));
    }
}

pub struct GatewayHandle {
    pub outgoing: UnboundedSender<String>,
    pub task: JoinHandle<()>,
}

/// How the gateway reports to the store; tagged with its session so events
/// from a replaced session are dropped.
#[derive(Clone)]
pub struct GatewayEvents {
    session: SessionId,
    inbox: UnboundedSender<Input>,
}

impl GatewayEvents {
    fn send(&self, event: GatewayEvent) {
        let _ = self.inbox.send(Input::Gateway {
            session: self.session,
            event,
        });
    }

    pub fn ready(&self) {
        self.send(GatewayEvent::Ready);
    }

    pub fn resumed(&self) {
        self.send(GatewayEvent::Resumed);
    }

    pub fn closed(&self) {
        self.send(GatewayEvent::Closed);
    }

    pub fn session_expired(&self) {
        self.send(GatewayEvent::SessionExpired);
    }

    /// Decodes a dispatch and forwards it if the store handles it.
    pub fn dispatch(&self, event: &str, data: Value) {
        if let Some(dispatch) = Dispatch::decode(event, data) {
            self.send(GatewayEvent::Dispatch(dispatch));
        }
    }
}

/// How a voice call reports to the store; tagged with its call id.
#[derive(Clone)]
pub struct VoiceEvents {
    call: CallId,
    inbox: UnboundedSender<Input>,
}

impl VoiceEvents {
    pub fn send(&self, update: VoiceUpdate) {
        let _ = self.inbox.send(Input::Voice {
            call: self.call,
            update,
        });
    }
}

enum KeyringOp {
    Load,
    Store(Token),
    Clear,
}

/// Runs keyring calls one at a time, in order, on blocking threads (a clear
/// must never overtake the store that follows it).
async fn keyring_worker(
    store: Arc<dyn TokenStore>,
    mut ops: UnboundedReceiver<KeyringOp>,
    inbox: UnboundedSender<Input>,
) {
    while let Some(op) = ops.recv().await {
        let store = Arc::clone(&store);
        let loaded = tokio::task::spawn_blocking(move || match op {
            KeyringOp::Load => Some(store.load()),
            KeyringOp::Store(token) => {
                store.store(&token);
                None
            }
            KeyringOp::Clear => {
                store.clear();
                None
            }
        })
        .await;
        match loaded {
            Ok(Some(token)) => {
                let _ = inbox.send(Input::TokenLoaded(token));
            }
            Ok(None) => {}
            Err(error) => eprintln!("[keyring] {error}"),
        }
    }
}

/// Executes effects. Owns the handles of everything it started.
pub struct Runner {
    http: reqwest::Client,
    inbox: UnboundedSender<Input>,
    sink: Sink,
    realtime: Box<dyn Realtime>,
    keyring: UnboundedSender<KeyringOp>,
    http_slots: Arc<Semaphore>,
    gateway: Option<GatewayHandle>,
    calls: HashMap<CallId, UnboundedSender<VoiceCommand>>,
    sign_in: Option<JoinHandle<()>>,
    /// Uploads in flight, so a cancel can stop them.
    uploads: HashMap<RequestId, JoinHandle<()>>,
    startup_marks: Vec<&'static str>,
    /// View updates since the last flush.
    batch: Vec<ViewUpdate>,
    /// Answers requests locally instead of the network (`YAP_FIXTURE`).
    fixture: Option<Fixture>,
    fixture_serial: u64,
    /// The local player, once an `Audio` effect started it.
    audio: Option<AudioHandle>,
    /// Where the snapshot cache lives; `None` runs without one.
    cache_path: Option<PathBuf>,
    /// The writer thread, once `LoadSnapshot` started it.
    cache: Option<yap_cache::Writer>,
}

impl Runner {
    /// Must be called inside a Tokio runtime (it spawns the keyring worker).
    pub fn new(
        http: reqwest::Client,
        inbox: UnboundedSender<Input>,
        sink: Sink,
        realtime: Box<dyn Realtime>,
        tokens: Arc<dyn TokenStore>,
    ) -> Self {
        let (keyring, ops) = mpsc::unbounded_channel();
        tokio::spawn(keyring_worker(tokens, ops, inbox.clone()));
        Self {
            http,
            inbox,
            sink,
            realtime,
            keyring,
            http_slots: Arc::new(Semaphore::new(MAX_HTTP_IN_FLIGHT)),
            gateway: None,
            calls: HashMap::new(),
            sign_in: None,
            uploads: HashMap::new(),
            startup_marks: Vec::new(),
            batch: Vec::new(),
            fixture: None,
            fixture_serial: 0,
            audio: None,
            cache_path: None,
            cache: None,
        }
    }

    /// Keeps the snapshot cache at `path` (`None`: no cache).
    pub fn with_cache(mut self, path: Option<PathBuf>) -> Self {
        self.cache_path = path;
        self
    }

    /// Serves every request from `fixture` and keeps off the keyring, the
    /// gateway and voice.
    pub fn with_fixture(mut self, fixture: Option<Fixture>) -> Self {
        self.fixture = fixture;
        self
    }

    /// Hands the view updates gathered since the last flush to the sink.
    pub fn flush(&mut self) {
        if !self.batch.is_empty() {
            (self.sink)(std::mem::take(&mut self.batch));
        }
    }

    fn send(&self, input: Input) {
        let _ = self.inbox.send(input);
    }

    /// The fixture's side of an effect; `false` when the normal path runs it.
    fn run_fixture(&mut self, fixture: Fixture, effect: &Effect) -> bool {
        match effect {
            Effect::Http { id, request, .. } => {
                self.fixture_serial += 1;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
                let response = fixture.respond(request, now, self.fixture_serial);
                self.send(Input::Http {
                    id: *id,
                    result: Ok(response),
                });
            }
            Effect::LoadToken => self.send(Input::TokenLoaded(Some(Token::new("fixture")))),
            // The fixture never reads or writes the snapshot cache.
            Effect::LoadSnapshot => self.send(Input::SnapshotLoaded(None)),
            Effect::LoadCatalog => self.send(Input::CatalogLoaded(None)),
            Effect::GatewayConnect { session, .. } => self.send(Input::Gateway {
                session: *session,
                event: GatewayEvent::Ready,
            }),
            Effect::VoiceStart { call, target, .. } => {
                let me = VoiceMember {
                    id: "fixture-me".into(),
                    user_id: String::new(),
                    name: target.me_name.clone(),
                    muted: false,
                    deafened: false,
                    speaking: false,
                    me: true,
                    ..VoiceMember::default()
                };
                for update in [
                    VoiceUpdate::Roster(vec![me]),
                    VoiceUpdate::Connected,
                    VoiceUpdate::Status("Connected".into()),
                ] {
                    self.send(Input::Voice {
                        call: *call,
                        update,
                    });
                }
            }
            Effect::Upload { id, name, .. } => self.send(Input::Http {
                id: *id,
                result: Ok(ApiResponse::Uploaded(UploadedFile {
                    id: format!("att_fixture_upload_{}", id.0),
                    file_url: format!("/api/attachments/fixture/{}/{name}", id.0),
                    file_name: name.clone(),
                    file_size: 0,
                    content_type: None,
                })),
            }),
            Effect::Download { id, .. }
            | Effect::BackgroundUpload { id, .. }
            | Effect::ProfileUpload { id, .. }
            | Effect::SoundUpload { id, .. } => self.send(Input::Http {
                id: *id,
                result: Ok(ApiResponse::Done),
            }),
            Effect::CancelUpload(_)
            | Effect::GatewaySend(_)
            | Effect::GatewayDisconnect
            | Effect::StoreToken(_)
            | Effect::ClearToken
            | Effect::Revoke(_)
            | Effect::SignInStart(_)
            | Effect::SignInCancel
            | Effect::Voice { .. }
            | Effect::VoiceForget(_)
            | Effect::CacheWrite { .. }
            | Effect::CacheClear { .. }
            | Effect::SaveCatalog { .. }
            | Effect::SaveLocal { .. }
            | Effect::Audio { .. } => {}
            Effect::View(_) | Effect::Later { .. } | Effect::StartupMark(_) => return false,
        }
        true
    }

    /// Starts one effect. Never blocks and never awaits. View updates are
    /// held until [`Self::flush`].
    pub fn run(&mut self, effect: Effect) {
        if let Some(fixture) = self.fixture {
            if self.run_fixture(fixture, &effect) {
                return;
            }
        }
        match effect {
            Effect::View(update) => self.batch.push(update),
            Effect::Later { delay, input } => {
                let inbox = self.inbox.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    let _ = inbox.send(*input);
                });
            }
            Effect::Http { id, token, request } => {
                let api = Api::new(self.http.clone(), token);
                let inbox = self.inbox.clone();
                let slots = Arc::clone(&self.http_slots);
                tokio::spawn(async move {
                    // The semaphore is never closed, so a permit always comes.
                    let _permit = slots.acquire_owned().await;
                    let (method, path, body) = (request.method(), request.path(), request.body());
                    let body = if let Some(url) = request.public_url() {
                        api.get_public(&url).await
                    } else if request.on_kova() {
                        api.call_kova(method, &path, &body).await
                    } else {
                        api.call_with(method, &path, &body, &request.headers())
                            .await
                    };
                    let result = body.and_then(|body| request.decode(body));
                    let _ = inbox.send(Input::Http { id, result });
                });
            }
            Effect::GatewayConnect {
                session,
                token,
                user_id,
            } => {
                self.disconnect_gateway();
                let api = Api::new(self.http.clone(), token);
                let events = GatewayEvents {
                    session,
                    inbox: self.inbox.clone(),
                };
                self.gateway = Some(self.realtime.spawn_gateway(api, user_id, events));
            }
            Effect::GatewaySend(frame) => {
                if let Some(gateway) = &self.gateway {
                    let _ = gateway.outgoing.send(frame.encode());
                }
            }
            Effect::GatewayDisconnect => self.disconnect_gateway(),
            Effect::LoadToken => self.keyring(KeyringOp::Load),
            Effect::LoadSnapshot => self.load_snapshot(),
            Effect::LoadCatalog => {
                let inbox = self.inbox.clone();
                tokio::spawn(async move {
                    let read = tokio::task::spawn_blocking(|| {
                        yap_cache::catalog::default_path()
                            .and_then(|path| yap_cache::catalog::load(&path))
                    })
                    .await
                    .ok()
                    .flatten();
                    let _ = inbox.send(Input::CatalogLoaded(
                        read.map(|stored| (stored.catalog, stored.fetched_at_ms)),
                    ));
                });
            }
            Effect::SaveCatalog { catalog } => {
                // Stamped as the response lands, not with the clock of the
                // page that asked.
                let fetched_at_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
                tokio::task::spawn_blocking(move || {
                    let Some(path) = yap_cache::catalog::default_path() else {
                        return;
                    };
                    let stored = yap_cache::catalog::StoredCatalog {
                        fetched_at_ms,
                        catalog,
                    };
                    if let Err(error) = yap_cache::catalog::save(&path, &stored) {
                        eprintln!("[cache] catalog: {error}");
                    }
                });
            }
            Effect::CacheWrite { user_id, write } => {
                if let Some(cache) = &self.cache {
                    cache.write(&user_id, write);
                }
            }
            Effect::CacheClear { user_id } => {
                if let Some(cache) = &self.cache {
                    cache.clear(&user_id);
                }
            }
            Effect::StoreToken(token) => self.keyring(KeyringOp::Store(token)),
            Effect::ClearToken => self.keyring(KeyringOp::Clear),
            Effect::Revoke(token) => {
                let http = self.http.clone();
                tokio::spawn(async move { auth::revoke(&http, &token).await });
            }
            Effect::SignInStart(attempt) => {
                self.cancel_sign_in();
                self.sign_in = Some(tokio::spawn(sign_in(
                    self.http.clone(),
                    attempt,
                    self.inbox.clone(),
                )));
            }
            Effect::SignInCancel => self.cancel_sign_in(),
            Effect::VoiceStart {
                call,
                token,
                target,
            } => {
                let (commands, rx) = mpsc::unbounded_channel();
                self.calls.insert(call, commands);
                let events = VoiceEvents {
                    call,
                    inbox: self.inbox.clone(),
                };
                let api = Api::new(self.http.clone(), token);
                self.realtime
                    .spawn_voice(api, self.http.clone(), target, rx, events);
            }
            Effect::Voice { call, command } => {
                let leave = command == VoiceCommand::Leave;
                if let Some(commands) = self.calls.get(&call) {
                    let _ = commands.send(command);
                }
                if leave {
                    self.calls.remove(&call);
                }
            }
            Effect::VoiceForget(call) => {
                self.calls.remove(&call);
            }
            Effect::Upload {
                id,
                token,
                channel_id,
                path,
                name,
                content_type,
            } => {
                self.uploads.retain(|_, task| !task.is_finished());
                let api = Api::new(self.http.clone(), token);
                let inbox = self.inbox.clone();
                let task = tokio::spawn(async move {
                    let path = std::path::PathBuf::from(path);
                    let target = format!("/api/channels/{channel_id}/messages/upload");
                    let pasted = is_pasted_copy(&path).then(|| path.clone());
                    let result = upload(&api, &target, path, name, content_type, id, &inbox).await;
                    // A pasted picture is a temp copy; a failed upload keeps
                    // it for the retry (startup prunes what is left).
                    if let (Some(copy), true) = (pasted, result.is_ok()) {
                        let _ = tokio::fs::remove_file(copy).await;
                    }
                    let result = result.and_then(|body| {
                        serde_json::from_value(body)
                            .map(ApiResponse::Uploaded)
                            .map_err(|e| crate::api::ApiError::Decode(format!("upload: {e}")))
                    });
                    let _ = inbox.send(Input::Http { id, result });
                });
                self.uploads.insert(id, task);
            }
            Effect::CancelUpload(id) => {
                if let Some(task) = self.uploads.remove(&id) {
                    task.abort();
                }
            }
            Effect::Download {
                id,
                token,
                url,
                dest,
            } => {
                let api = Api::new(self.http.clone(), token);
                let inbox = self.inbox.clone();
                tokio::spawn(async move {
                    let result = download(&api, &url, dest).await.map(|()| ApiResponse::Done);
                    let _ = inbox.send(Input::Http { id, result });
                });
            }
            Effect::BackgroundUpload {
                id,
                token,
                path,
                content_type,
            } => {
                let api = Api::new(self.http.clone(), token);
                let inbox = self.inbox.clone();
                tokio::spawn(async move {
                    let result =
                        background_upload(&api, path, content_type)
                            .await
                            .and_then(|body| {
                                crate::store::backgrounds::decode_asset(body).map(|asset| {
                                    ApiResponse::Backgrounds(
                                        crate::store::backgrounds::BackgroundsResponse::Uploaded(
                                            asset,
                                        ),
                                    )
                                })
                            });
                    let _ = inbox.send(Input::Http { id, result });
                });
            }
            Effect::ProfileUpload {
                id,
                token,
                asset,
                path,
                content_type,
            } => {
                let api = Api::new(self.http.clone(), token);
                let inbox = self.inbox.clone();
                tokio::spawn(async move {
                    let icon = asset == crate::store::ProfileAsset::ServerIcon;
                    let result = profile_upload(&api, asset, path, content_type).await.map(
                        |body| match body.get("url").and_then(Value::as_str) {
                            // The server icon's URL goes into the settings save.
                            Some(url) if icon => ApiResponse::Admin(
                                crate::store::AdminResponse::IconUrl(url.to_owned()),
                            ),
                            _ => ApiResponse::Done,
                        },
                    );
                    let _ = inbox.send(Input::Http { id, result });
                });
            }
            Effect::SoundUpload {
                id,
                token,
                channel_id,
                path,
                name,
                content_type,
                fields,
            } => {
                let api = Api::new(self.http.clone(), token);
                let inbox = self.inbox.clone();
                tokio::spawn(async move {
                    let target = format!("/api/channels/{channel_id}/messages/upload");
                    let result = sound_upload(&api, &target, path, name, content_type, fields)
                        .await
                        .and_then(|body| {
                            serde_json::from_value(body)
                                .map(ApiResponse::Uploaded)
                                .map_err(|e| crate::api::ApiError::Decode(format!("upload: {e}")))
                        });
                    let _ = inbox.send(Input::Http { id, result });
                });
            }
            Effect::SaveLocal { file, contents } => {
                tokio::task::spawn_blocking(move || {
                    let Some(dir) = yap_cache::settings::app_dir() else {
                        return;
                    };
                    if let Err(error) = yap_cache::settings::save_app_file(&dir, file, &contents) {
                        eprintln!("[apps] save {file}: {error}");
                    }
                });
            }
            Effect::Audio { token, command } => {
                if self.audio.is_none() {
                    let events = AudioEvents {
                        inbox: self.inbox.clone(),
                    };
                    self.audio = Some(self.realtime.spawn_audio(self.http.clone(), events));
                }
                if let Some(commands) = self.audio.as_ref().and_then(|a| a.commands.as_ref()) {
                    let _ = commands.send((token, command));
                }
            }
            Effect::StartupMark(mark) => {
                if !self.startup_marks.contains(&mark) {
                    self.startup_marks.push(mark);
                    let ms = started_at().elapsed().as_millis();
                    eprintln!("[startup] {mark} after {ms} ms");
                }
            }
        }
    }

    /// Starts the writer thread and reads the snapshot on a blocking thread;
    /// always answers with `Input::SnapshotLoaded`.
    fn load_snapshot(&mut self) {
        let Some(path) = self.cache_path.clone() else {
            return self.send(Input::SnapshotLoaded(None));
        };
        let writer = match yap_cache::Writer::spawn(CACHE_WRITE_INTERVAL) {
            Ok(writer) => writer,
            Err(error) => {
                eprintln!("[cache] writer thread: {error}");
                return self.send(Input::SnapshotLoaded(None));
            }
        };
        let _ = CACHE_WRITER.set(writer.clone());
        self.cache = Some(writer.clone());
        let inbox = self.inbox.clone();
        tokio::spawn(async move {
            let read = tokio::task::spawn_blocking(move || {
                let (cache, snapshot) = read_snapshot(&path);
                writer.attach(cache);
                snapshot
            })
            .await;
            // A failed read is no snapshot; the launch must not wait forever.
            let snapshot = read.unwrap_or_else(|error| {
                eprintln!("[cache] {error}");
                None
            });
            let _ = inbox.send(Input::SnapshotLoaded(snapshot.map(Box::new)));
        });
    }

    fn keyring(&self, op: KeyringOp) {
        let _ = self.keyring.send(op);
    }

    fn disconnect_gateway(&mut self) {
        if let Some(gateway) = self.gateway.take() {
            gateway.task.abort();
        }
    }

    fn cancel_sign_in(&mut self) {
        if let Some(task) = self.sign_in.take() {
            task.abort();
        }
    }
}

/// A picture pasted from the clipboard, which the shell saved under
/// `YapNative/cache/paste/` (`YapNative/profiles/<name>/cache/paste/` for a
/// named profile; cpp/mediashell.cpp). Nothing else is deleted.
fn is_pasted_copy(path: &Path) -> bool {
    let parts: Vec<_> = path.components().rev().skip(1).take(5).collect();
    let is_app = |c: &std::path::Component| c.as_os_str() == "YapNative";
    match parts.as_slice() {
        [paste, cache, rest @ ..]
            if paste.as_os_str() == "paste" && cache.as_os_str() == "cache" =>
        {
            match rest {
                [app, ..] if is_app(app) => true,
                [name, profiles, app, ..] => {
                    profiles.as_os_str() == "profiles"
                        && is_app(app)
                        && name
                            .as_os_str()
                            .to_str()
                            .is_some_and(yap_cache::profile::is_valid)
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// Chunks an upload is streamed in; progress is reported per chunk.
const UPLOAD_CHUNK: usize = 64 * 1024;

/// Progress reports are at least this far apart (bytes, as thousandths).
const PROGRESS_STEP: u64 = 10;

/// The largest profile picture, banner or server icon the server takes
/// (`MAX_IMAGE_SIZE`, src/lib/image-validation.ts).
const MAX_IMAGE_UPLOAD_BYTES: u64 = 8 * 1024 * 1024;

/// Refuses a file over `cap` before reading it, as the server's 413 would
/// (`describe_error` words it "The file is too large.").
async fn check_size(path: &Path, cap: u64) -> Result<(), crate::api::ApiError> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|error| crate::api::ApiError::Network(error.to_string()))?;
    if meta.len() > cap {
        return Err(crate::api::ApiError::Http {
            status: 413,
            message: "file is too large".into(),
        });
    }
    Ok(())
}

/// Reads the file and posts it as multipart `file`, reporting the bytes
/// handed to the connection as they go.
async fn upload(
    api: &Api,
    target: &str,
    path: PathBuf,
    name: String,
    content_type: String,
    id: RequestId,
    inbox: &UnboundedSender<Input>,
) -> Result<Value, crate::api::ApiError> {
    use futures_util::StreamExt;
    check_size(&path, crate::store::view::media::MAX_UPLOAD_BYTES).await?;
    let read = tokio::task::spawn_blocking(move || std::fs::read(path)).await;
    let bytes = match read {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(error)) => return Err(crate::api::ApiError::Network(error.to_string())),
        Err(error) => return Err(crate::api::ApiError::Network(error.to_string())),
    };
    let total = bytes.len() as u64;
    let chunks: Vec<Vec<u8>> = bytes.chunks(UPLOAD_CHUNK).map(<[u8]>::to_vec).collect();
    let progress = inbox.clone();
    let mut sent = 0u64;
    let mut reported = 0u64;
    let stream = futures_util::stream::iter(chunks).map(move |chunk| {
        sent += chunk.len() as u64;
        let step = sent.saturating_mul(1000) / total.max(1);
        if step >= reported + PROGRESS_STEP || sent == total {
            reported = step;
            let _ = progress.send(Input::UploadProgress { id, sent });
        }
        Ok::<_, std::io::Error>(chunk)
    });
    let body = reqwest::Body::wrap_stream(stream);
    let part = reqwest::multipart::Part::stream_with_length(body, total).file_name(name);
    let part = match part.mime_str(&content_type) {
        Ok(part) => part,
        Err(_) => {
            return Err(crate::api::ApiError::Decode("bad content type".into()));
        }
    };
    let form = reqwest::multipart::Form::new().part("file", part);
    api.upload(target, form).await
}

/// Posts a soundboard sound: the file as `file` plus the form `fields`
/// (SoundboardPicker.tsx `handleUpload`), up to the 50 MB soundboard limit.
async fn sound_upload(
    api: &Api,
    target: &str,
    path: String,
    name: String,
    content_type: String,
    fields: Vec<(String, String)>,
) -> Result<Value, crate::api::ApiError> {
    check_size(Path::new(&path), crate::store::apps::MAX_UPLOAD_BYTES).await?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| crate::api::ApiError::Network(error.to_string()))?;
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(name)
        .mime_str(&content_type)
        .map_err(|_| crate::api::ApiError::Decode("bad content type".into()))?;
    let mut form = reqwest::multipart::Form::new().part("file", part);
    for (key, value) in fields {
        form = form.text(key, value);
    }
    api.upload(target, form).await
}

/// Posts a profile picture or banner: the file as `file`, with `kind` for a
/// banner and the kept collectibles for an avatar.
/// `POST /api/camera-backgrounds` with the picture as multipart `file`.
async fn background_upload(
    api: &Api,
    path: String,
    content_type: String,
) -> Result<Value, crate::api::ApiError> {
    let name = Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "background".into());
    check_size(
        Path::new(&path),
        crate::store::backgrounds::MAX_BACKGROUND_BYTES,
    )
    .await?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| crate::api::ApiError::Network(error.to_string()))?;
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(name)
        .mime_str(&content_type)
        .map_err(|_| crate::api::ApiError::Decode("bad content type".into()))?;
    api.upload(
        "/api/camera-backgrounds",
        reqwest::multipart::Form::new().part("file", part),
    )
    .await
}

async fn profile_upload(
    api: &Api,
    asset: crate::store::ProfileAsset,
    path: String,
    content_type: String,
) -> Result<Value, crate::api::ApiError> {
    use crate::store::ProfileAsset;
    let name = Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".into());
    check_size(Path::new(&path), MAX_IMAGE_UPLOAD_BYTES).await?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| crate::api::ApiError::Network(error.to_string()))?;
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(name)
        .mime_str(&content_type)
        .map_err(|_| crate::api::ApiError::Decode("bad content type".into()))?;
    let mut form = reqwest::multipart::Form::new();
    match &asset {
        ProfileAsset::Banner => form = form.text("kind", "banner"),
        ProfileAsset::Avatar {
            display: Some(display),
        } => form = form.text("avatar_display", display.clone()),
        ProfileAsset::Avatar { display: None } | ProfileAsset::ServerIcon => {}
    }
    api.upload(asset.path(), form.part("file", part)).await
}

/// Fetches `url` into `dest`, creating its folder (at most 25 MB).
async fn download(api: &Api, url: &str, dest: String) -> Result<(), crate::api::ApiError> {
    api.download(url, Path::new(&dest), crate::api::MAX_DOWNLOAD_BYTES)
        .await
}

/// One device sign-in; reports the code, then the outcome.
async fn sign_in(http: reqwest::Client, attempt: SignInId, inbox: UnboundedSender<Input>) {
    let send = |event: SignInEvent| {
        let _ = inbox.send(Input::SignIn { attempt, event });
    };
    let started = match auth::start(&http).await {
        Ok(started) => started,
        Err(error) => return send(SignInEvent::Failed(error.to_string())),
    };
    send(SignInEvent::DeviceCode {
        user_code: started.user_code.clone(),
        url: started.verification_uri_complete.clone(),
    });
    // Cancels the code on Kova too if this task is aborted mid-poll.
    struct CancelOnDrop(Option<(reqwest::Client, String)>);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            if let Some((http, code)) = self.0.take() {
                tokio::spawn(async move { auth::cancel(&http, &code).await });
            }
        }
    }
    let mut guard = CancelOnDrop(Some((http.clone(), started.device_code.clone())));
    let result = auth::wait_for_approval(&http, &started).await;
    guard.0 = None;
    send(match result {
        Ok(token) => SignInEvent::Approved(token),
        Err(error) => SignInEvent::Failed(error.to_string()),
    });
}

/// The store actor: the only owner of [`State`]. It reduces whatever is
/// already queued (up to [`MAX_INPUTS_PER_BATCH`]) before flushing the view,
/// so a burst of dispatches reaches the UI as one batch.
async fn actor(mut state: State, mut runner: Runner, mut inbox: UnboundedReceiver<Input>) {
    while let Some(first) = inbox.recv().await {
        let mut next = Some(first);
        let mut reduced = 0;
        while let Some(input) = next.take() {
            for effect in reduce(&mut state, input) {
                runner.run(effect);
            }
            reduced += 1;
            if reduced < MAX_INPUTS_PER_BATCH {
                next = inbox.try_recv().ok();
            }
        }
        runner.flush();
    }
}

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("yap-core")
            .enable_all()
            .build()
            // Without a runtime there is no client at all; this is the app edge.
            .expect("tokio runtime")
    })
}

/// Starts the core on its own runtime and returns its command channel.
/// `local_offset` puts message times in the user's time zone. With
/// `YAP_FIXTURE` set, the core runs offline on that fixture.
pub fn spawn(
    sink: Sink,
    http: reqwest::Client,
    realtime: Box<dyn Realtime>,
    tokens: Arc<dyn TokenStore>,
    local_offset: LocalOffset,
) -> UnboundedSender<Command> {
    started_at();
    let (commands, mut command_rx) = mpsc::unbounded_channel::<Command>();
    let (inbox, inbox_rx) = mpsc::unbounded_channel();
    let fixture = Fixture::from_env();
    if let Some(fixture) = fixture {
        eprintln!("[fixture] {fixture:?}: offline, no keyring, gateway or voice");
    }
    let state = State::new(&config::get().api_base)
        .with_local_offset(local_offset)
        .with_voice_standby(config::get().voice_standby);
    runtime().spawn(async move {
        // The fixture runs offline and never touches the cache.
        let cache = fixture.is_none().then(yap_cache::default_path).flatten();
        let runner = Runner::new(http, inbox.clone(), sink, realtime, tokens)
            .with_fixture(fixture)
            .with_cache(cache);
        // Commands join the same queue as effect results, in arrival order.
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                if inbox.send(Input::User(command)).is_err() {
                    break;
                }
            }
        });
        actor(state, runner, inbox_rx).await;
    });
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_shells_paste_folder_counts_as_a_pasted_copy() {
        let pasted = r"C:\Users\a\AppData\Local\YapNative\cache\paste\image-1.png";
        let picked = r"C:\Users\a\Pictures\image-1.png";
        let lookalike = r"C:\Users\a\cache\paste\image-1.png";
        let profiled = r"C:\Users\a\AppData\Local\YapNative\profiles\jon\cache\paste\image-1.png";
        let bad_profile =
            r"C:\Users\a\AppData\Local\YapNative\profiles\Jon\cache\paste\image-1.png";
        let stranger = r"C:\Users\a\AppData\Local\Other\profiles\jon\cache\paste\image-1.png";
        assert_eq!(
            [pasted, picked, lookalike, profiled, bad_profile, stranger]
                .map(|p| is_pasted_copy(Path::new(p))),
            [cfg!(windows), false, false, cfg!(windows), false, false]
        );
    }
}
