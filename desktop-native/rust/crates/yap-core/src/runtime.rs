//! The effect runner and the store actor.
//!
//! One task (the actor) owns [`State`]. It takes [`Input`]s from a single
//! channel, runs [`reduce`], and hands each [`Effect`] to the [`Runner`].
//! The runner never awaits I/O itself: HTTP calls, sign-in, keyring access,
//! the gateway and voice calls run as their own tasks and report back as
//! inputs, so a slow request no longer holds up dispatches or other commands.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use serde_json::Value;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use crate::api::{Api, Token};
use crate::auth::{self, TokenStore};
use crate::config;
use crate::store::{
    reduce, CallId, Command, Dispatch, Effect, GatewayEvent, Input, SessionId, SignInEvent,
    SignInId, State, Ui, VoiceCommand, VoiceTarget, VoiceUpdate,
};

/// Where [`Ui`] updates go (the bridge queues them onto the Qt thread).
pub type Sink = Box<dyn Fn(Ui) + Send + Sync>;

/// HTTP requests in flight at once; more wait for a slot.
const MAX_HTTP_IN_FLIGHT: usize = 8;

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
    startup_marks: Vec<&'static str>,
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
            startup_marks: Vec::new(),
        }
    }

    /// Starts one effect. Never blocks and never awaits.
    pub fn run(&mut self, effect: Effect) {
        match effect {
            Effect::Ui(update) => (self.sink)(update),
            Effect::Http { id, token, request } => {
                let api = Api::new(self.http.clone(), token);
                let inbox = self.inbox.clone();
                let slots = Arc::clone(&self.http_slots);
                tokio::spawn(async move {
                    // The semaphore is never closed, so a permit always comes.
                    let _permit = slots.acquire_owned().await;
                    let path = request.path();
                    let body = if request.is_post() {
                        api.post(&path, &request.body()).await
                    } else {
                        api.get(&path).await
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
                if let Some(commands) = self.calls.get(&call) {
                    let _ = commands.send(command);
                }
                if command == VoiceCommand::Leave {
                    self.calls.remove(&call);
                }
            }
            Effect::VoiceForget(call) => {
                self.calls.remove(&call);
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

/// The store actor: the only owner of [`State`].
async fn actor(mut state: State, mut runner: Runner, mut inbox: UnboundedReceiver<Input>) {
    while let Some(input) = inbox.recv().await {
        for effect in reduce(&mut state, input) {
            runner.run(effect);
        }
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
pub fn spawn(
    sink: Sink,
    http: reqwest::Client,
    realtime: Box<dyn Realtime>,
    tokens: Arc<dyn TokenStore>,
) -> UnboundedSender<Command> {
    started_at();
    let (commands, mut command_rx) = mpsc::unbounded_channel::<Command>();
    let (inbox, inbox_rx) = mpsc::unbounded_channel();
    let state = State::new(&config::get().api_base);
    runtime().spawn(async move {
        let runner = Runner::new(http, inbox.clone(), sink, realtime, tokens);
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
