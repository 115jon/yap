//! The client core. It runs on a Tokio runtime, owns all state, and talks to
//! the UI only through [`Ui`] updates (Qt thread) and [`Command`]s (from QML).

pub mod api;
pub mod auth;
pub mod config;
pub mod gateway;

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use api::Api;

/// What the UI asked for, plus events from the core's own tasks.
pub enum Command {
    Start,
    StartSignIn,
    CancelSignIn,
    UseToken(String),
    SignOut,
    SelectServer(String),
    SelectChannel(String),
    Send(String),
    ReloadServers,
    Event(Event),
}

pub enum Event {
    DeviceCode { user_code: String, url: String },
    SignedIn(String),
    SignInFailed(String),
    GatewayReady,
    GatewayClosed,
    SessionExpired,
    Dispatch(String, Value),
}

/// Updates for the Qt side. Lists travel as JSON so QML can bind them
/// directly; the message list is incremental (reset / upsert / remove).
pub enum Ui {
    Phase(&'static str),
    DeviceCode { user_code: String, url: String },
    Status(String),
    Connected(bool),
    Me(Value),
    Servers(Value),
    ActiveServer(String),
    Channels(Value),
    ActiveChannel { id: String, name: String },
    MessagesReset(Value),
    MessageUpsert(Value),
    MessageRemoved(String),
    Typing(String),
}

pub type Sink = Box<dyn Fn(Ui) + Send + Sync>;

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("yap-core")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

/// When the core was started, for startup timing in the log.
pub(crate) fn started_at() -> &'static std::time::Instant {
    static STARTED: OnceLock<std::time::Instant> = OnceLock::new();
    STARTED.get_or_init(std::time::Instant::now)
}

/// Starts the core and returns its command channel.
pub fn spawn(sink: Sink) -> UnboundedSender<Command> {
    started_at();
    let (tx, rx) = mpsc::unbounded_channel();
    let core = Core::new(sink, tx.clone());
    runtime().spawn(core.run(rx));
    tx
}

struct Session {
    api: Api,
    token: String,
    me: Value,
    gateway_tx: UnboundedSender<String>,
    gateway: JoinHandle<()>,
    gateway_ready: bool,
}

struct Core {
    sink: Sink,
    tx: UnboundedSender<Command>,
    http: reqwest::Client,
    session: Option<Session>,
    sign_in: Option<JoinHandle<()>>,
    servers: Vec<Value>,
    channels: Vec<Value>,
    active_server: Option<String>,
    active_channel: Option<String>,
    /// Display names seen so far, for typing indicators.
    names: HashMap<String, String>,
}

fn str_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn display_name(user: &Value) -> String {
    let name = str_field(user, "display_name");
    if name.is_empty() {
        str_field(user, "username").to_owned()
    } else {
        name.to_owned()
    }
}

fn absolute_url(url: &str) -> String {
    if url.starts_with('/') {
        format!("{}{url}", config::get().api_base)
    } else {
        url.to_owned()
    }
}

/// The fields the message list needs, in QML-friendly names.
fn ui_message(message: &Value) -> Value {
    let author = message.get("author").cloned().unwrap_or(Value::Null);
    let attachments: Vec<Value> = message
        .get("attachments")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|a| {
                    json!({
                        "name": str_field(a, "filename"),
                        "url": absolute_url(str_field(a, "url")),
                        "contentType": str_field(a, "content_type"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id": str_field(message, "id"),
        "channelId": str_field(message, "channel_id"),
        "authorId": str_field(message, "author_id"),
        "author": display_name(&author),
        "avatar": absolute_url(str_field(&author, "avatar_url")),
        "content": str_field(message, "content"),
        "createdAt": str_field(message, "created_at"),
        "edited": message.get("updated_at").is_some_and(|v| !v.is_null()),
        "attachments": attachments,
    })
}

impl Core {
    fn new(sink: Sink, tx: UnboundedSender<Command>) -> Self {
        Self {
            sink,
            tx,
            http: api::http_client(),
            session: None,
            sign_in: None,
            servers: Vec::new(),
            channels: Vec::new(),
            active_server: None,
            active_channel: None,
            names: HashMap::new(),
        }
    }

    fn ui(&self, update: Ui) {
        (self.sink)(update);
    }

    async fn run(mut self, mut rx: UnboundedReceiver<Command>) {
        while let Some(command) = rx.recv().await {
            if let Err(error) = self.handle(command).await {
                if api::is_unauthorized(&error) {
                    self.end_session(false).await;
                    self.ui(Ui::Status("Your session ended. Sign in again.".into()));
                } else {
                    self.ui(Ui::Status(format!("{error:#}")));
                }
            }
        }
    }

    async fn handle(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Start => match auth::load_token() {
                Some(token) => self.begin_session(token).await?,
                None => self.ui(Ui::Phase("signedOut")),
            },
            Command::StartSignIn => self.start_sign_in(),
            Command::CancelSignIn => {
                if let Some(task) = self.sign_in.take() {
                    task.abort();
                }
                self.ui(Ui::Phase("signedOut"));
            }
            Command::UseToken(token) => {
                let token = token.trim().to_owned();
                if !token.is_empty() {
                    self.begin_session(token).await?;
                }
            }
            Command::SignOut => self.end_session(true).await,
            Command::SelectServer(id) => self.select_server(id).await?,
            Command::SelectChannel(id) => self.select_channel(id).await?,
            Command::Send(text) => self.send(text).await?,
            Command::ReloadServers => self.load_servers().await?,
            Command::Event(event) => self.on_event(event).await?,
        }
        Ok(())
    }

    fn start_sign_in(&mut self) {
        if let Some(task) = self.sign_in.take() {
            task.abort();
        }
        self.ui(Ui::Phase("deviceCode"));
        self.ui(Ui::DeviceCode {
            user_code: String::new(),
            url: String::new(),
        });
        let http = self.http.clone();
        let tx = self.tx.clone();
        self.sign_in = Some(tokio::spawn(async move {
            let started = match auth::start(&http).await {
                Ok(started) => started,
                Err(error) => {
                    let _ = tx.send(Command::Event(Event::SignInFailed(error.to_string())));
                    return;
                }
            };
            let _ = tx.send(Command::Event(Event::DeviceCode {
                user_code: started.user_code.clone(),
                url: started.verification_uri_complete.clone(),
            }));
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
            let _ = tx.send(Command::Event(match result {
                Ok(token) => Event::SignedIn(token),
                Err(error) => Event::SignInFailed(error.to_string()),
            }));
        }));
    }

    async fn begin_session(&mut self, token: String) -> Result<()> {
        self.end_session(false).await;
        self.ui(Ui::Phase("loading"));
        self.ui(Ui::Status("Signing in…".into()));
        let api = Api::new(self.http.clone(), &token);
        // Independent requests overlap: each one costs a Kova session check.
        let (me, servers) = tokio::join!(api.get("/api/users/me"), api.get("/api/servers"));
        let me = match me {
            Ok(me) => me,
            Err(error) => {
                if api::is_unauthorized(&error) {
                    auth::clear_token();
                }
                self.ui(Ui::Phase("signedOut"));
                return Err(error);
            }
        };
        auth::store_token(&token);
        let user_id = str_field(&me, "id").to_owned();
        self.names.insert(user_id.clone(), display_name(&me));
        self.ui(Ui::Me(json!({
            "id": user_id,
            "name": display_name(&me),
            "username": str_field(&me, "username"),
            "avatar": absolute_url(str_field(&me, "avatar_url")),
        })));

        let (gateway_tx, gateway_rx) = mpsc::unbounded_channel();
        let gateway = tokio::spawn(gateway::run(
            api.clone(),
            user_id,
            gateway_rx,
            self.tx.clone(),
        ));
        self.session = Some(Session {
            api,
            token,
            me,
            gateway_tx,
            gateway,
            gateway_ready: false,
        });

        self.ui(Ui::Status("Loading servers…".into()));
        self.apply_servers(servers?).await?;
        self.ui(Ui::Phase("ready"));
        self.ui(Ui::Status(String::new()));
        Ok(())
    }

    async fn end_session(&mut self, revoke: bool) {
        if let Some(session) = self.session.take() {
            session.gateway.abort();
            if revoke {
                auth::revoke(&self.http, &session.token).await;
            }
        }
        if revoke {
            auth::clear_token();
        }
        self.servers.clear();
        self.channels.clear();
        self.active_server = None;
        self.active_channel = None;
        self.ui(Ui::Connected(false));
        self.ui(Ui::Phase("signedOut"));
    }

    fn api(&self) -> Option<Api> {
        self.session.as_ref().map(|s| s.api.clone())
    }

    fn gateway_send(&self, payload: Value) {
        if let Some(session) = &self.session {
            if session.gateway_ready {
                let _ = session.gateway_tx.send(payload.to_string());
            }
        }
    }

    /// (Re)subscribes the gateway to what is on screen.
    fn sync_subscriptions(&self) {
        if let Some(server) = &self.active_server {
            self.gateway_send(json!({ "op": 35, "d": { "server_id": server } }));
        }
        if let Some(channel) = &self.active_channel {
            self.gateway_send(json!({ "op": 27, "d": { "channel_id": channel } }));
        }
    }

    async fn load_servers(&mut self) -> Result<()> {
        let Some(api) = self.api() else { return Ok(()) };
        let servers = api.get("/api/servers").await?;
        self.apply_servers(servers).await
    }

    async fn apply_servers(&mut self, servers: Value) -> Result<()> {
        self.servers = servers.as_array().cloned().unwrap_or_default();
        let list: Vec<Value> = self
            .servers
            .iter()
            .map(|s| {
                json!({
                    "id": str_field(s, "id"),
                    "name": str_field(s, "name"),
                    "icon": absolute_url(str_field(s, "icon_url")),
                })
            })
            .collect();
        self.ui(Ui::Servers(Value::Array(list)));
        let keep = self
            .active_server
            .clone()
            .filter(|id| self.servers.iter().any(|s| str_field(s, "id") == id));
        let first = self
            .servers
            .first()
            .map(|s| str_field(s, "id").to_owned());
        if let Some(id) = keep.or(first) {
            self.select_server(id).await?;
        }
        Ok(())
    }

    async fn select_server(&mut self, id: String) -> Result<()> {
        let Some(api) = self.api() else { return Ok(()) };
        self.active_server = Some(id.clone());
        self.ui(Ui::ActiveServer(id.clone()));
        let data = api.get(&format!("/api/servers/{id}/channels")).await?;
        if self.active_server.as_deref() != Some(id.as_str()) {
            return Ok(());
        }
        let categories = data.get("categories").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut channels = data.get("channels").and_then(Value::as_array).cloned().unwrap_or_default();
        let category_rank: HashMap<&str, (i64, &str)> = categories
            .iter()
            .map(|c| {
                (
                    str_field(c, "id"),
                    (c.get("rank").and_then(Value::as_i64).unwrap_or(0), str_field(c, "name")),
                )
            })
            .collect();
        channels.sort_by_key(|c| {
            let rank = category_rank
                .get(str_field(c, "category_id"))
                .map(|(r, _)| *r)
                .unwrap_or(-1);
            (rank, c.get("position").and_then(Value::as_i64).unwrap_or(0))
        });
        let list: Vec<Value> = channels
            .iter()
            .map(|c| {
                let category = category_rank
                    .get(str_field(c, "category_id"))
                    .map(|(_, name)| *name)
                    .unwrap_or("");
                json!({
                    "id": str_field(c, "id"),
                    "name": str_field(c, "name"),
                    "type": str_field(c, "channel_type"),
                    "category": category,
                })
            })
            .collect();
        self.channels = channels;
        self.ui(Ui::Channels(Value::Array(list)));
        self.sync_subscriptions();

        let first_text = self
            .channels
            .iter()
            .find(|c| str_field(c, "channel_type") == "text")
            .map(|c| str_field(c, "id").to_owned());
        if let Some(channel) = first_text {
            self.select_channel(channel).await?;
        } else {
            self.active_channel = None;
            self.ui(Ui::ActiveChannel {
                id: String::new(),
                name: String::new(),
            });
            self.ui(Ui::MessagesReset(json!([])));
        }
        Ok(())
    }

    async fn select_channel(&mut self, id: String) -> Result<()> {
        let Some(api) = self.api() else { return Ok(()) };
        let Some(channel) = self.channels.iter().find(|c| str_field(c, "id") == id) else {
            return Ok(());
        };
        if str_field(channel, "channel_type") != "text" {
            self.ui(Ui::Status("Voice channels are not in the prototype yet.".into()));
            return Ok(());
        }
        let name = str_field(channel, "name").to_owned();
        if let Some(previous) = self.active_channel.replace(id.clone()) {
            if previous != id {
                self.gateway_send(json!({ "op": 28, "d": { "channel_id": previous } }));
            }
        }
        self.ui(Ui::ActiveChannel { id: id.clone(), name });
        self.ui(Ui::MessagesReset(json!([])));
        self.gateway_send(json!({ "op": 27, "d": { "channel_id": id } }));

        let page = api.get(&format!("/api/channels/{id}/messages?limit=50")).await?;
        if self.active_channel.as_deref() != Some(id.as_str()) {
            return Ok(());
        }
        let messages = page
            .get("messages")
            .or(Some(&page))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for message in &messages {
            if let Some(author) = message.get("author") {
                self.names.insert(str_field(author, "id").to_owned(), display_name(author));
            }
        }
        let list: Vec<Value> = messages.iter().map(ui_message).collect();
        self.ui(Ui::MessagesReset(Value::Array(list)));
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| {
            eprintln!("[startup] messages shown after {} ms", started_at().elapsed().as_millis());
        });
        Ok(())
    }

    async fn send(&mut self, text: String) -> Result<()> {
        let text = text.trim().to_owned();
        let (Some(api), Some(channel)) = (self.api(), self.active_channel.clone()) else {
            return Ok(());
        };
        if text.is_empty() {
            return Ok(());
        }
        let message = api
            .post(
                &format!("/api/channels/{channel}/messages"),
                &json!({ "content": text }),
            )
            .await?;
        if str_field(&message, "channel_id") == channel {
            self.ui(Ui::MessageUpsert(ui_message(&message)));
        }
        Ok(())
    }

    async fn on_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::DeviceCode { user_code, url } => self.ui(Ui::DeviceCode { user_code, url }),
            Event::SignedIn(token) => {
                self.sign_in = None;
                self.begin_session(token).await?;
            }
            Event::SignInFailed(message) => {
                self.sign_in = None;
                self.ui(Ui::Phase("signedOut"));
                self.ui(Ui::Status(message));
            }
            Event::GatewayReady => {
                if let Some(session) = &mut self.session {
                    session.gateway_ready = true;
                }
                self.ui(Ui::Connected(true));
                self.sync_subscriptions();
            }
            Event::GatewayClosed => {
                if let Some(session) = &mut self.session {
                    session.gateway_ready = false;
                }
                self.ui(Ui::Connected(false));
            }
            Event::SessionExpired => {
                self.end_session(false).await;
                auth::clear_token();
                self.ui(Ui::Status("Your session ended. Sign in again.".into()));
            }
            Event::Dispatch(name, data) => self.on_dispatch(&name, data),
        }
        Ok(())
    }

    fn on_dispatch(&mut self, name: &str, data: Value) {
        let active = self.active_channel.as_deref().unwrap_or_default();
        match name {
            "MESSAGE_CREATE" | "MESSAGE_UPDATE" => {
                if let Some(author) = data.get("author") {
                    self.names.insert(str_field(author, "id").to_owned(), display_name(author));
                }
                if str_field(&data, "channel_id") == active {
                    if name == "MESSAGE_CREATE" || data.get("author").is_some() {
                        self.ui(Ui::MessageUpsert(ui_message(&data)));
                    } else {
                        // Partial update (an edit, or embeds arriving after a
                        // link unfurl): pass on only the fields it carries.
                        let mut patch = json!({ "id": str_field(&data, "id"), "partial": true });
                        if let Some(content) = data.get("content").and_then(Value::as_str) {
                            patch["content"] = json!(content);
                        }
                        if data.get("updated_at").is_some_and(Value::is_string) {
                            patch["edited"] = json!(true);
                        }
                        self.ui(Ui::MessageUpsert(patch));
                    }
                }
            }
            "MESSAGE_DELETE" => self.ui(Ui::MessageRemoved(str_field(&data, "id").to_owned())),
            "TYPING_START" => {
                let me = self
                    .session
                    .as_ref()
                    .map(|s| str_field(&s.me, "id").to_owned())
                    .unwrap_or_default();
                let user = str_field(&data, "user_id");
                if str_field(&data, "channel_id") == active && user != me {
                    let name = self
                        .names
                        .get(user)
                        .cloned()
                        .unwrap_or_else(|| "Someone".to_owned());
                    self.ui(Ui::Typing(name));
                }
            }
            "USER_SERVERS_UPDATE" | "GUILD_DELETE" | "GUILD_UPDATE" => {
                let _ = self.tx.send(Command::ReloadServers);
            }
            _ => {}
        }
    }
}
