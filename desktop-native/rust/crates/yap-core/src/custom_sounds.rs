//! Custom UI sounds (custom-sounds.ts, sound-catalog.ts,
//! CustomSoundsCard.tsx): any UI sound can be replaced by an audio file of
//! the user's, kept on this device only. These are the rules; the app keeps
//! the files under `%LOCALAPPDATA%\YapNative\custom-sounds` and decodes
//! them for playback.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::shell::Sound;

/// Every UI sound the web can play, in the settings' order
/// (`SOUND_IDS`).
pub const SOUND_IDS: [&str; 18] = [
    "voiceJoin",
    "voiceLeave",
    "mute",
    "unmute",
    "deafen",
    "undeafen",
    "notification",
    "message",
    "connect",
    "disconnect",
    "ring",
    "outgoingRing",
    "callConnect",
    "callEnd",
    "screenShareStart",
    "screenShareStop",
    "streamWatcherStart",
    "streamWatcherStop",
];

/// A file's largest size (1 MB).
pub const MAX_BYTES: u64 = 1024 * 1024;
/// The longest sound, in seconds; rings may run to 15.
pub const MAX_SECONDS: f64 = 5.0;
pub const MAX_RING_SECONDS: f64 = 15.0;

/// What the import takes (`CUSTOM_SOUND_EXTENSIONS`).
pub const EXTENSIONS: [&str; 9] = [
    ".mp3", ".wav", ".ogg", ".oga", ".opus", ".webm", ".m4a", ".aac", ".flac",
];

/// `SOUND_LABELS`: what each sound means, in settings copy.
pub fn label(id: &str) -> &'static str {
    match id {
        "voiceJoin" => "Someone joins voice",
        "voiceLeave" => "Someone leaves voice",
        "mute" => "Mute",
        "unmute" => "Unmute",
        "deafen" => "Deafen",
        "undeafen" => "Undeafen",
        "notification" => "Notification",
        "message" => "New message",
        "connect" => "You connect to voice",
        "disconnect" => "You disconnect from voice",
        "ring" => "Incoming call ring",
        "outgoingRing" => "Outgoing call ring",
        "callConnect" => "Call connected",
        "callEnd" => "Call ended",
        "screenShareStart" => "Screen share starts",
        "screenShareStop" => "Screen share stops",
        "streamWatcherStart" => "Viewer starts watching",
        "streamWatcherStop" => "Viewer stops watching",
        _ => "",
    }
}

/// `soundFileStem`: `voiceJoin` → `voice-join`.
pub fn file_stem(id: &str) -> String {
    let mut out = String::new();
    for c in id.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `soundIdForFilename`: `voice-join.mp3`, `voiceJoin.wav` and
/// `Voice_Join.ogg` all name `voiceJoin`.
pub fn id_for_filename(name: &str) -> Option<&'static str> {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let key: String = stem
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    SOUND_IDS
        .into_iter()
        .find(|id| id.to_ascii_lowercase() == key)
}

/// `maxSecondsFor`.
pub fn max_seconds(id: &str) -> f64 {
    if matches!(id, "ring" | "outgoingRing") {
        MAX_RING_SECONDS
    } else {
        MAX_SECONDS
    }
}

/// The id a native UI sound plays under.
pub fn id_of(sound: Sound) -> &'static str {
    match sound {
        Sound::VoiceJoin => "voiceJoin",
        Sound::VoiceLeave => "voiceLeave",
        Sound::Mute => "mute",
        Sound::Unmute => "unmute",
        Sound::Deafen => "deafen",
        Sound::Undeafen => "undeafen",
        Sound::Notification => "notification",
        Sound::Message => "message",
        Sound::Connect => "connect",
        Sound::Disconnect => "disconnect",
        Sound::ScreenShareStart => "screenShareStart",
        Sound::ScreenShareStop => "screenShareStop",
    }
}

/// Why a file was refused (`CustomSoundError`), with the web's sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Type(String),
    Size(String),
    Empty(String),
    Decode(String),
    Duration(String),
    Unmatched(String),
}

impl Refusal {
    pub fn message(&self) -> &str {
        match self {
            Self::Type(m)
            | Self::Size(m)
            | Self::Empty(m)
            | Self::Decode(m)
            | Self::Duration(m)
            | Self::Unmatched(m) => m,
        }
    }
}

/// `looksLikeAudio`: an `audio/` type, or a known extension (some
/// platforms leave the type empty or say video/webm for Opus).
pub fn looks_like_audio(name: &str, content_type: &str) -> bool {
    if content_type.starts_with("audio/") {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// The checks before decoding (`validateCustomSound`): type, emptiness,
/// size.
pub fn check_file(name: &str, content_type: &str, size: u64) -> Result<(), Refusal> {
    if !looks_like_audio(name, content_type) {
        return Err(Refusal::Type(format!("{name} isn't an audio file.")));
    }
    if size == 0 {
        return Err(Refusal::Empty(format!("{name} is empty.")));
    }
    if size > MAX_BYTES {
        return Err(Refusal::Size(format!("{name} is larger than 1 MB.")));
    }
    Ok(())
}

/// The check after decoding: a real length, under the sound's limit.
pub fn check_duration(id: &str, name: &str, seconds: f64) -> Result<(), Refusal> {
    let limit = max_seconds(id);
    if !seconds.is_finite() || seconds <= 0.0 || seconds > limit {
        return Err(Refusal::Duration(format!(
            "{name} is longer than {limit} seconds."
        )));
    }
    Ok(())
}

/// The sentence for a file that would not decode.
pub fn decode_refusal(name: &str) -> Refusal {
    Refusal::Decode(format!("{name} couldn't be read as audio on this device."))
}

/// The sentence for a bulk file no sound is named after.
pub fn unmatched(name: &str) -> Refusal {
    Refusal::Unmatched(format!("{name} doesn't match a sound name."))
}

/// One imported file, as `custom-sounds.json` keeps it
/// (`CustomSoundMeta`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomSound {
    /// The original file name, shown in settings.
    pub name: String,
    /// The copy in the custom sounds folder.
    pub file: String,
    pub size: u64,
    /// Seconds, from the decoded audio.
    pub duration: f64,
    pub updated_at: i64,
}

/// Reads `custom-sounds.json` leniently: every known sound whose row
/// decodes is kept, and a row that does not is dropped on its own. `None`
/// when the text is not a JSON object at all.
pub fn parse_index(text: &str) -> Option<BTreeMap<String, CustomSound>> {
    let rows: serde_json::Map<String, serde_json::Value> = serde_json::from_str(text).ok()?;
    Some(
        rows.into_iter()
            .filter(|(id, _)| SOUND_IDS.contains(&id.as_str()))
            .filter_map(|(id, row)| Some((id, serde_json::from_value(row).ok()?)))
            .collect(),
    )
}

/// An index rebuilt from the files in the custom sounds folder, for when
/// `custom-sounds.json` cannot be read: `(file name, size, seconds,
/// modified ms)` per file, each matched to its sound by name.
pub fn rebuild_index(
    files: impl IntoIterator<Item = (String, u64, f64, i64)>,
) -> BTreeMap<String, CustomSound> {
    files
        .into_iter()
        .filter_map(|(file, size, duration, updated_at)| {
            let id = id_for_filename(&file)?;
            let sound = CustomSound {
                name: file.clone(),
                file,
                size,
                duration,
                updated_at,
            };
            Some((id.to_owned(), sound))
        })
        .collect()
}

/// "1.5 s" or "12 s" (CustomSoundsCard.tsx `seconds`).
pub fn seconds_label(duration: f64) -> String {
    if duration < 10.0 {
        format!("{duration:.1} s")
    } else {
        format!("{duration:.0} s")
    }
}

/// The bulk import's summary line.
pub fn imported_line(count: usize) -> String {
    match count {
        0 => "No sounds imported.".into(),
        1 => "Imported 1 sound.".into(),
        n => format!("Imported {n} sounds."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_should_be_kebab_case() {
        assert_eq!(file_stem("streamWatcherStart"), "stream-watcher-start");
    }

    #[test]
    fn file_names_should_match_their_sound_in_any_spelling() {
        let got = [
            "voice-join.mp3",
            "voiceJoin.wav",
            "Voice_Join.ogg",
            "nope.mp3",
        ]
        .map(id_for_filename);
        assert_eq!(
            got,
            [
                Some("voiceJoin"),
                Some("voiceJoin"),
                Some("voiceJoin"),
                None
            ]
        );
    }

    #[test]
    fn rings_should_get_fifteen_seconds() {
        assert_eq!((max_seconds("ring"), max_seconds("mute")), (15.0, 5.0));
    }

    #[test]
    fn a_text_file_should_be_refused() {
        assert_eq!(
            check_file("notes.txt", "text/plain", 10),
            Err(Refusal::Type("notes.txt isn't an audio file.".into()))
        );
    }

    #[test]
    fn an_opus_file_without_a_type_should_pass() {
        assert_eq!(check_file("ping.opus", "", 10), Ok(()));
    }

    #[test]
    fn a_large_file_should_be_refused() {
        assert!(matches!(
            check_file("a.mp3", "audio/mpeg", MAX_BYTES + 1),
            Err(Refusal::Size(_))
        ));
    }

    #[test]
    fn a_long_sound_should_be_refused() {
        assert_eq!(
            check_duration("mute", "a.mp3", 6.0),
            Err(Refusal::Duration("a.mp3 is longer than 5 seconds.".into()))
        );
    }

    #[test]
    fn seconds_should_read_as_the_card_does() {
        assert_eq!(
            (seconds_label(1.54), seconds_label(12.4)),
            ("1.5 s".into(), "12 s".into())
        );
    }

    #[test]
    fn every_native_sound_should_have_an_id_in_the_catalog() {
        assert!(Sound::ALL.iter().all(|s| SOUND_IDS.contains(&id_of(*s))));
    }

    fn row(file: &str) -> String {
        format!(r#"{{"name":"{file}","file":"{file}","size":10,"duration":1.5,"updatedAt":7}}"#)
    }

    #[test]
    fn a_broken_row_should_not_drop_the_others() {
        let text = format!(
            r#"{{"mute":{},"deafen":{{"name":3}},"nope":{},"ring":{}}}"#,
            row("mute.mp3"),
            row("nope.mp3"),
            row("ring.wav")
        );
        let index = parse_index(&text).expect("an object");
        assert_eq!(
            index.keys().map(String::as_str).collect::<Vec<_>>(),
            ["mute", "ring"]
        );
    }

    #[test]
    fn text_that_is_not_an_object_should_not_parse() {
        assert_eq!(parse_index("{\"mute\": "), None);
        assert_eq!(parse_index("[]"), None);
    }

    #[test]
    fn the_folder_should_rebuild_the_index() {
        let index = rebuild_index([
            ("voice-join.mp3".into(), 5, 1.0, 9),
            ("stray.txt".into(), 1, 0.0, 0),
        ]);
        assert_eq!(index.len(), 1);
        let sound = &index["voiceJoin"];
        assert_eq!(
            (sound.file.as_str(), sound.size, sound.updated_at),
            ("voice-join.mp3", 5, 9)
        );
    }
}
