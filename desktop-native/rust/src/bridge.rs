//! The QML-facing object. It forwards calls to the core as [`Command`]s and
//! applies [`Ui`] updates on the Qt thread.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, phase)]
        #[qproperty(QString, status)]
        #[qproperty(bool, connected)]
        #[qproperty(QString, user_code, cxx_name = "userCode")]
        #[qproperty(QString, verification_url, cxx_name = "verificationUrl")]
        #[qproperty(QString, me_json, cxx_name = "meJson")]
        #[qproperty(QString, servers_json, cxx_name = "serversJson")]
        #[qproperty(QString, channels_json, cxx_name = "channelsJson")]
        #[qproperty(QString, active_server_id, cxx_name = "activeServerId")]
        #[qproperty(QString, active_channel_id, cxx_name = "activeChannelId")]
        #[qproperty(QString, active_channel_name, cxx_name = "activeChannelName")]
        #[qproperty(QString, voice_channel_id, cxx_name = "voiceChannelId")]
        #[qproperty(QString, voice_status, cxx_name = "voiceStatus")]
        #[qproperty(QString, voice_roster_json, cxx_name = "voiceRosterJson")]
        #[qproperty(bool, voice_muted, cxx_name = "voiceMuted")]
        type YapClient = super::YapClientRust;

        #[qinvokable]
        fn start(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "startSignIn"]
        fn start_sign_in(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "cancelSignIn"]
        fn cancel_sign_in(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "useToken"]
        fn use_token(self: Pin<&mut Self>, token: &QString);

        #[qinvokable]
        #[cxx_name = "signOut"]
        fn sign_out(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "selectServer"]
        fn select_server(self: Pin<&mut Self>, id: &QString);

        #[qinvokable]
        #[cxx_name = "selectChannel"]
        fn select_channel(self: Pin<&mut Self>, id: &QString);

        #[qinvokable]
        #[cxx_name = "sendMessage"]
        fn send_message(self: Pin<&mut Self>, text: &QString);

        #[qinvokable]
        #[cxx_name = "joinVoice"]
        fn join_voice(self: Pin<&mut Self>, channel_id: &QString);

        #[qinvokable]
        #[cxx_name = "leaveVoice"]
        fn leave_voice(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "muteVoice"]
        fn set_voice_muted_from_ui(self: Pin<&mut Self>, muted: bool);

        /// The whole list for a newly opened channel (JSON array).
        #[qsignal]
        #[cxx_name = "messagesReset"]
        fn messages_reset(self: Pin<&mut Self>, json: QString);

        /// One message, new or changed (JSON object, may be partial).
        #[qsignal]
        #[cxx_name = "messageUpserted"]
        fn message_upserted(self: Pin<&mut Self>, json: QString);

        #[qsignal]
        #[cxx_name = "messageRemoved"]
        fn message_removed(self: Pin<&mut Self>, id: QString);

        #[qsignal]
        fn typing(self: Pin<&mut Self>, name: QString);
    }

    impl cxx_qt::Threading for YapClient {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use tokio::sync::mpsc::UnboundedSender;

use crate::client::{self, Command, Ui};

#[derive(Default)]
pub struct YapClientRust {
    phase: QString,
    status: QString,
    connected: bool,
    user_code: QString,
    verification_url: QString,
    me_json: QString,
    servers_json: QString,
    channels_json: QString,
    active_server_id: QString,
    active_channel_id: QString,
    active_channel_name: QString,
    voice_channel_id: QString,
    voice_status: QString,
    voice_roster_json: QString,
    voice_muted: bool,
    commands: Option<UnboundedSender<Command>>,
}

fn q(value: impl AsRef<str>) -> QString {
    QString::from(value.as_ref())
}

impl qobject::YapClient {
    fn command(&self, command: Command) {
        if let Some(tx) = &self.rust().commands {
            let _ = tx.send(command);
        }
    }

    pub fn start(mut self: Pin<&mut Self>) {
        if self.rust().commands.is_some() {
            return;
        }
        self.as_mut().set_phase(q("boot"));
        let qt = self.qt_thread();
        let sink: client::Sink = Box::new(move |update| {
            let _ = qt.queue(move |this| apply(this, update));
        });
        let tx = client::spawn(sink);
        let _ = tx.send(Command::Start);
        self.rust_mut().commands = Some(tx);
    }

    pub fn start_sign_in(self: Pin<&mut Self>) {
        self.command(Command::StartSignIn);
    }

    pub fn cancel_sign_in(self: Pin<&mut Self>) {
        self.command(Command::CancelSignIn);
    }

    pub fn use_token(self: Pin<&mut Self>, token: &QString) {
        self.command(Command::UseToken(token.to_string()));
    }

    pub fn sign_out(self: Pin<&mut Self>) {
        self.command(Command::SignOut);
    }

    pub fn select_server(self: Pin<&mut Self>, id: &QString) {
        self.command(Command::SelectServer(id.to_string()));
    }

    pub fn select_channel(self: Pin<&mut Self>, id: &QString) {
        self.command(Command::SelectChannel(id.to_string()));
    }

    pub fn send_message(self: Pin<&mut Self>, text: &QString) {
        self.command(Command::Send(text.to_string()));
    }

    pub fn join_voice(self: Pin<&mut Self>, channel_id: &QString) {
        self.command(Command::JoinVoice(channel_id.to_string()));
    }

    pub fn leave_voice(self: Pin<&mut Self>) {
        self.command(Command::LeaveVoice);
    }

    pub fn set_voice_muted_from_ui(self: Pin<&mut Self>, muted: bool) {
        self.command(Command::SetVoiceMuted(muted));
    }
}

fn apply(mut this: Pin<&mut qobject::YapClient>, update: Ui) {
    match update {
        Ui::Phase(phase) => this.set_phase(q(phase)),
        Ui::Status(text) => this.set_status(q(text)),
        Ui::Connected(connected) => this.set_connected(connected),
        Ui::DeviceCode { user_code, url } => {
            this.as_mut().set_user_code(q(user_code));
            this.set_verification_url(q(url));
        }
        Ui::Me(me) => this.set_me_json(q(me.to_string())),
        Ui::Servers(list) => this.set_servers_json(q(list.to_string())),
        Ui::ActiveServer(id) => this.set_active_server_id(q(id)),
        Ui::Channels(list) => this.set_channels_json(q(list.to_string())),
        Ui::ActiveChannel { id, name } => {
            this.as_mut().set_active_channel_name(q(name));
            this.set_active_channel_id(q(id));
        }
        Ui::MessagesReset(list) => this.messages_reset(q(list.to_string())),
        Ui::MessageUpsert(message) => this.message_upserted(q(message.to_string())),
        Ui::MessageRemoved(id) => this.message_removed(q(id)),
        Ui::Typing(name) => this.typing(q(name)),
        Ui::Voice {
            channel_id,
            status,
            roster,
            muted,
        } => {
            this.as_mut().set_voice_status(q(status));
            this.as_mut().set_voice_roster_json(q(roster.to_string()));
            this.as_mut().set_voice_muted(muted);
            this.set_voice_channel_id(q(channel_id));
        }
    }
}
