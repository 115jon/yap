//! `Apps`: the voice apps in a call for QML (the soundboard picker and its
//! playing rows, join and leave sounds in settings, the activities dialog
//! and the Wordle stage, Listen Together's panel and now-playing card).
//! Everything shown is one JSON `state` ([`yap_core::store::apps::AppsView`]);
//! `level` is the local player's output meter.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// The apps view as JSON (`AppsView`).
        #[qproperty(QString, state)]
        /// The local player's output peak, 0..1 (soundboard and Listen
        /// Together), while something plays.
        #[qproperty(f64, level)]
        type Apps = super::AppsRust;

        /// The picker opened: the call server's sounds load.
        #[qinvokable]
        #[cxx_name = "loadSounds"]
        fn load_sounds(self: &Apps);

        /// A server's sounds (the settings chooser).
        #[qinvokable]
        #[cxx_name = "loadCatalog"]
        fn load_catalog(self: &Apps, server_id: &QString);

        /// Play a tone or server sound for everyone in the call.
        #[qinvokable]
        fn play(self: &Apps, sound_id: &QString);

        /// Hear a sound alone.
        #[qinvokable]
        fn preview(self: &Apps, sound_id: &QString);

        #[qinvokable]
        #[cxx_name = "stopPlayback"]
        fn stop_playback(self: &Apps, playback_id: &QString);

        #[qinvokable]
        #[cxx_name = "pausePlayback"]
        fn pause_playback(self: &Apps, playback_id: &QString, paused: bool);

        /// 0..1.
        #[qinvokable]
        #[cxx_name = "playbackVolume"]
        fn playback_volume(self: &Apps, playback_id: &QString, volume: f64);

        /// The picker's volume, 0..1.
        #[qinvokable]
        #[cxx_name = "setSendVolume"]
        fn set_send_volume(self: &Apps, volume: f64);

        /// Upload an audio file (a path from `Files.pickFiles`) as a sound.
        #[qinvokable]
        fn upload(self: &Apps, path: &QString, name: &QString, emoji: &QString, volume: f64);

        #[qinvokable]
        #[cxx_name = "deleteSound"]
        fn delete_sound(self: &Apps, sound_id: &QString);

        #[qinvokable]
        #[cxx_name = "clearError"]
        fn clear_error(self: &Apps);

        /// MyInstants: trending (empty) or a search.
        #[qinvokable]
        #[cxx_name = "searchInstants"]
        fn search_instants(self: &Apps, query: &QString);

        /// radio-browser: top stations (empty) or a search.
        #[qinvokable]
        #[cxx_name = "searchRadio"]
        fn search_radio(self: &Apps, query: &QString);

        /// The favourites, once per session.
        #[qinvokable]
        #[cxx_name = "loadFavorites"]
        fn load_favorites(self: &Apps);

        /// Favourite or unfavourite a tile (its `id`).
        #[qinvokable]
        #[cxx_name = "toggleFavorite"]
        fn toggle_favorite(self: &Apps, sound: &QString);

        /// Name, emoji and volume (0..1) of a server or custom sound.
        #[qinvokable]
        #[cxx_name = "editSound"]
        fn edit_sound(self: &Apps, sound: &QString, name: &QString, emoji: &QString, volume: f64);

        /// Keep an audio file (128 KiB at most) on this device as a custom
        /// sound for the call's server.
        #[qinvokable]
        #[cxx_name = "addLocalSound"]
        fn add_local_sound(
            self: &Apps,
            path: &QString,
            name: &QString,
            emoji: &QString,
            volume: f64,
        );

        /// `track` or `collection`.
        #[qinvokable]
        #[cxx_name = "ltFilter"]
        fn lt_filter(self: &Apps, filter: &QString);

        /// Queue a resolved link (all of a collection): `append` or `play-now`.
        #[qinvokable]
        #[cxx_name = "ltEnqueueAll"]
        fn lt_enqueue_all(self: &Apps, mode: &QString);

        /// Resolve collection result `index` and queue it.
        #[qinvokable]
        #[cxx_name = "ltQueueCollection"]
        fn lt_queue_collection(self: &Apps, index: i32);

        /// Queue a recently played entry again.
        #[qinvokable]
        #[cxx_name = "ltRequeue"]
        fn lt_requeue(self: &Apps, history_id: &QString);

        /// A join or leave sound for a scope (`*` or a server id). `enabled`
        /// -1 leaves it, 0 off, 1 on; `sound` `default:<id>` or
        /// `server:<server>:<id>` (empty leaves it); `clear` removes it.
        #[qinvokable]
        #[cxx_name = "setTrigger"]
        fn set_trigger(
            self: &Apps,
            trigger: &QString,
            scope: &QString,
            enabled: i32,
            sound: &QString,
            clear: bool,
        );

        #[qinvokable]
        #[cxx_name = "previewTrigger"]
        fn preview_trigger(self: &Apps, trigger: &QString, scope: &QString);

        /// `wordle` (or `warp-rush`) for the call.
        #[qinvokable]
        #[cxx_name = "startActivity"]
        fn start_activity(self: &Apps, activity: &QString);

        #[qinvokable]
        #[cxx_name = "leaveActivity"]
        fn leave_activity(self: &Apps);

        /// The stage opened (or a minute passed): today's puzzle in New York.
        #[qinvokable]
        #[cxx_name = "wordleOpen"]
        fn wordle_open(self: &Apps);

        /// A letter, `enter` or `backspace`.
        #[qinvokable]
        #[cxx_name = "wordleKey"]
        fn wordle_key(self: &Apps, key: &QString);

        #[qinvokable]
        #[cxx_name = "wordleHint"]
        fn wordle_hint(self: &Apps);

        #[qinvokable]
        #[cxx_name = "wordleReveal"]
        fn wordle_reveal(self: &Apps);

        #[qinvokable]
        #[cxx_name = "wordleHardMode"]
        fn wordle_hard_mode(self: &Apps, on: bool);

        /// A search, or a YouTube or Spotify link to resolve.
        #[qinvokable]
        #[cxx_name = "ltSearch"]
        fn lt_search(self: &Apps, query: &QString);

        /// Queue result `index`: `append`, `play-next` or `play-now`.
        #[qinvokable]
        #[cxx_name = "ltEnqueue"]
        fn lt_enqueue(self: &Apps, index: i32, mode: &QString);

        /// Play an entry (empty: the current one).
        #[qinvokable]
        #[cxx_name = "ltPlay"]
        fn lt_play(self: &Apps, entry_id: &QString);

        #[qinvokable]
        #[cxx_name = "ltPause"]
        fn lt_pause(self: &Apps, paused: bool);

        #[qinvokable]
        #[cxx_name = "ltSeek"]
        fn lt_seek(self: &Apps, position_ms: f64);

        #[qinvokable]
        #[cxx_name = "ltSkip"]
        fn lt_skip(self: &Apps);

        #[qinvokable]
        #[cxx_name = "ltRemove"]
        fn lt_remove(self: &Apps, entry_id: &QString);

        #[qinvokable]
        #[cxx_name = "ltClear"]
        fn lt_clear(self: &Apps);

        /// 0..1.
        #[qinvokable]
        #[cxx_name = "ltVolume"]
        fn lt_volume(self: &Apps, volume: f64);

        #[qinvokable]
        #[cxx_name = "ltLoudness"]
        fn lt_loudness(self: &Apps, enabled: bool, preset: &QString);

        #[qinvokable]
        #[cxx_name = "ltClearError"]
        fn lt_clear_error(self: &Apps);

        /// The wall clock in epoch ms (the now-playing card's clock).
        #[qinvokable]
        #[cxx_name = "nowMs"]
        fn now_ms(self: &Apps) -> f64;

        /// Where the room's track is now, in ms (`getListenTogetherLivePositionMs`:
        /// run on from the anchor while it plays, clamped to the track, 0
        /// for a track without a length). The now-playing clock reads it
        /// each tick.
        #[qinvokable]
        #[cxx_name = "ltPositionMs"]
        fn lt_position_ms(self: &Apps) -> f64;

        /// Reads the player's meter into `level` (a timer calls it while
        /// the meter shows).
        #[qinvokable]
        fn sample(self: Pin<&mut Apps>);
    }

    impl cxx_qt::Initialize for Apps {}
}

use core::pin::Pin;

use cxx_qt_lib::QString;
use std::io::Read as _;

use yap_core::store::apps::{
    AppsCommand, AppsView, LtClock, Trigger, MAX_LOCAL_SOUND_BYTES, SOUNDS_FILE,
};
use yap_core::Command;

use crate::client;
use crate::gui;

#[derive(Default)]
pub struct AppsRust {
    state: QString,
    level: f64,
}

/// Plays a soundboard sound in the call, as a click on its cell does (a
/// keybind; nothing happens outside a call).
pub fn play_sound(sound_id: &str) {
    send(AppsCommand::Play(sound_id.to_owned()));
}

fn send(command: AppsCommand) {
    client::command(Command::Apps(command));
}

fn text(value: &QString) -> String {
    value.to_string()
}

fn apply(object: Pin<&mut qobject::Apps>, view: &AppsView) {
    let state = serde_json::to_string(view).unwrap_or_else(|_| "{}".into());
    object.set_state(QString::from(&state));
}

thread_local! {
    /// The Listen Together clock of the last view (the GUI thread's).
    static LT_CLOCK: core::cell::Cell<LtClock> = core::cell::Cell::new(LtClock::default());
}

/// The store's apps view changed.
pub(crate) fn on_view(view: AppsView) {
    LT_CLOCK.with(|clock| clock.set(LtClock::of(&view.listen)));
    if let Some(object) = gui::object::<qobject::Apps>() {
        apply(object, &view);
    }
    gui::keep_apps(view);
}

/// The device's sound settings, for join and leave sounds and the
/// automatic clip volume.
pub(crate) fn send_local_settings() {
    let s = crate::desktop::current();
    send(AppsCommand::LocalSettings {
        sounds_enabled: s.sounds,
        sound_volume: s.sound_settings.sound_volume,
        soundboard_volume: s.sound_settings.soundboard_volume,
    });
}

impl gui::Slot for qobject::Apps {
    crate::slot_cell!(qobject::Apps);

    fn restore(self: Pin<&mut Self>) {
        apply(self, &gui::apps_view());
    }
}

impl cxx_qt::Initialize for qobject::Apps {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
        send_local_settings();
        send_local_sounds();
    }
}

/// The custom sounds kept in `%LOCALAPPDATA%\Yap.Data`.
fn send_local_sounds() {
    let sounds = yap_cache::settings::app_dir()
        .and_then(|dir| yap_cache::settings::load_app_file(&dir, SOUNDS_FILE))
        .map(yap_core::store::apps::parse_local_sounds)
        .unwrap_or_default();
    send(AppsCommand::LocalData { sounds });
}

impl Drop for AppsRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Apps>();
    }
}

impl qobject::Apps {
    pub fn load_sounds(&self) {
        send(AppsCommand::LoadSounds);
    }

    pub fn load_catalog(&self, server_id: &QString) {
        send(AppsCommand::LoadCatalog(text(server_id)));
    }

    pub fn play(&self, sound_id: &QString) {
        send(AppsCommand::Play(text(sound_id)));
    }

    pub fn preview(&self, sound_id: &QString) {
        send(AppsCommand::Preview(text(sound_id)));
    }

    pub fn stop_playback(&self, playback_id: &QString) {
        send(AppsCommand::StopPlayback(text(playback_id)));
    }

    pub fn pause_playback(&self, playback_id: &QString, paused: bool) {
        send(AppsCommand::PausePlayback {
            playback_id: text(playback_id),
            paused,
        });
    }

    pub fn playback_volume(&self, playback_id: &QString, volume: f64) {
        send(AppsCommand::PlaybackVolume {
            playback_id: text(playback_id),
            volume,
        });
    }

    pub fn set_send_volume(&self, volume: f64) {
        send(AppsCommand::SendVolume(volume));
    }

    pub fn upload(&self, path: &QString, name: &QString, emoji: &QString, volume: f64) {
        let path = text(path);
        let file = std::path::Path::new(&path);
        let size = std::fs::metadata(file).map_or(0, |m| m.len());
        let file_name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        send(AppsCommand::Upload {
            path,
            file_name,
            size,
            content_type: String::new(),
            sound_name: text(name),
            emoji: text(emoji),
            volume,
        });
    }

    pub fn delete_sound(&self, sound_id: &QString) {
        send(AppsCommand::Delete(text(sound_id)));
    }

    pub fn clear_error(&self) {
        send(AppsCommand::ClearError);
    }

    pub fn search_instants(&self, query: &QString) {
        send(AppsCommand::InstantsSearch(text(query)));
    }

    pub fn search_radio(&self, query: &QString) {
        send(AppsCommand::RadioSearch(text(query)));
    }

    pub fn load_favorites(&self) {
        send(AppsCommand::LoadFavorites);
    }

    pub fn toggle_favorite(&self, sound: &QString) {
        send(AppsCommand::ToggleFavorite(text(sound)));
    }

    pub fn edit_sound(&self, sound: &QString, name: &QString, emoji: &QString, volume: f64) {
        send(AppsCommand::EditSound {
            sound: text(sound),
            name: text(name),
            emoji: text(emoji),
            volume,
        });
    }

    pub fn add_local_sound(&self, path: &QString, name: &QString, emoji: &QString, volume: f64) {
        let path = text(path);
        let file = std::path::Path::new(&path);
        let file_name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // One byte past the limit is enough for the store to refuse it.
        let mut bytes = Vec::new();
        let read = std::fs::File::open(file).and_then(|f| {
            f.take(MAX_LOCAL_SOUND_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        });
        if let Err(error) = read {
            eprintln!("[soundboard] custom sound: {error}");
            return;
        }
        send(AppsCommand::AddLocalSound {
            id: uuid::Uuid::new_v4().to_string(),
            file_name,
            bytes,
            name: text(name),
            emoji: text(emoji),
            volume,
        });
    }

    pub fn lt_filter(&self, filter: &QString) {
        send(AppsCommand::LtFilter(text(filter)));
    }

    pub fn lt_enqueue_all(&self, mode: &QString) {
        send(AppsCommand::LtEnqueueAll { mode: text(mode) });
    }

    pub fn lt_queue_collection(&self, index: i32) {
        if let Ok(index) = usize::try_from(index) {
            send(AppsCommand::LtQueueCollection(index));
        }
    }

    pub fn lt_requeue(&self, history_id: &QString) {
        send(AppsCommand::LtRequeue(text(history_id)));
    }

    pub fn set_trigger(
        &self,
        trigger: &QString,
        scope: &QString,
        enabled: i32,
        sound: &QString,
        clear: bool,
    ) {
        let Some(trigger) = Trigger::parse(&text(trigger)) else {
            return;
        };
        let sound = text(sound);
        send(AppsCommand::SetTrigger {
            trigger,
            scope: text(scope),
            enabled: (enabled >= 0).then_some(enabled > 0),
            sound: (!sound.is_empty()).then_some(sound),
            clear,
        });
    }

    pub fn preview_trigger(&self, trigger: &QString, scope: &QString) {
        if let Some(trigger) = Trigger::parse(&text(trigger)) {
            send(AppsCommand::PreviewTrigger {
                trigger,
                scope: text(scope),
            });
        }
    }

    pub fn start_activity(&self, activity: &QString) {
        send(AppsCommand::StartActivity {
            activity: text(activity),
            now_ms: client::voice::now_ms(),
        });
    }

    pub fn leave_activity(&self) {
        send(AppsCommand::LeaveActivity);
    }

    pub fn wordle_open(&self) {
        send(AppsCommand::WordleOpen {
            date: yap_core::wordle::new_york_date(client::voice::now_ms()),
        });
    }

    pub fn wordle_key(&self, key: &QString) {
        let key = text(key);
        match key.as_str() {
            "enter" => send(AppsCommand::WordleEnter),
            "backspace" => send(AppsCommand::WordleBackspace),
            _ => {
                if let Some(letter) = key.chars().next().filter(char::is_ascii_alphabetic) {
                    send(AppsCommand::WordleKey(letter));
                }
            }
        }
    }

    pub fn wordle_hint(&self) {
        send(AppsCommand::WordleHint);
    }

    pub fn wordle_reveal(&self) {
        send(AppsCommand::WordleReveal);
    }

    pub fn wordle_hard_mode(&self, on: bool) {
        send(AppsCommand::WordleHardMode(on));
    }

    pub fn lt_search(&self, query: &QString) {
        send(AppsCommand::LtSearch(text(query)));
    }

    pub fn lt_enqueue(&self, index: i32, mode: &QString) {
        if let Ok(index) = usize::try_from(index) {
            send(AppsCommand::LtEnqueue {
                index,
                mode: text(mode),
            });
        }
    }

    pub fn lt_play(&self, entry_id: &QString) {
        let id = text(entry_id);
        send(AppsCommand::LtPlay((!id.is_empty()).then_some(id)));
    }

    pub fn lt_pause(&self, paused: bool) {
        send(AppsCommand::LtPause(paused));
    }

    pub fn lt_seek(&self, position_ms: f64) {
        send(AppsCommand::LtSeek(position_ms.max(0.0)));
    }

    pub fn lt_skip(&self) {
        send(AppsCommand::LtSkip);
    }

    pub fn lt_remove(&self, entry_id: &QString) {
        send(AppsCommand::LtRemove(text(entry_id)));
    }

    pub fn lt_clear(&self) {
        send(AppsCommand::LtClear);
    }

    pub fn lt_volume(&self, volume: f64) {
        send(AppsCommand::LtVolume(volume));
    }

    pub fn lt_loudness(&self, enabled: bool, preset: &QString) {
        send(AppsCommand::LtLoudness {
            enabled,
            preset: text(preset),
        });
    }

    pub fn lt_clear_error(&self) {
        send(AppsCommand::LtClearError);
    }

    pub fn sample(self: Pin<&mut Self>) {
        self.set_level(crate::apps_audio::level());
    }

    pub fn now_ms(&self) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "epoch ms fit f64 exactly")]
        let now = client::voice::now_ms() as f64;
        now
    }

    pub fn lt_position_ms(&self) -> f64 {
        let now = client::voice::now_ms();
        LT_CLOCK.with(|clock| clock.get().live_ms(now))
    }
}
