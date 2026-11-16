//! Writes yap-protocol's wire types as TypeScript, one file per Rust module
//! (`export_to` on each type), into packages/protocol/src/generated.
//!
//! Run through `pnpm protocol:generate`, which formats the output; CI
//! regenerates and fails when the committed files differ. A type gets here
//! when it derives `ts_rs::TS` under the `ts` feature and is listed below
//! (its dependencies come along on their own).

use std::path::PathBuf;

use ts_rs::{Config, ExportError, TS};
use yap_protocol::{
    collectibles, frame, game_activity, payload, profile_board, rest, tiers, voice_app,
};

fn export<T: TS + 'static>(cfg: &Config, written: &mut usize) -> Result<(), ExportError> {
    T::export_all(cfg)?;
    *written += 1;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out: PathBuf = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: protocol-ts <out dir>")?;
    // Stale files from renamed or removed modules must not survive.
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    std::fs::create_dir_all(&out)?;
    // JSON numbers: ids are strings on this wire, and counts and
    // timestamps fit in an f64.
    let cfg = Config::new().with_large_int("number").with_out_dir(&out);
    let mut written = 0;
    // frame.rs
    export::<frame::Frame>(&cfg, &mut written)?;
    // tiers.rs
    export::<tiers::Tier>(&cfg, &mut written)?;
    export::<tiers::EntitlementSource>(&cfg, &mut written)?;
    export::<tiers::Entitlement>(&cfg, &mut written)?;
    export::<tiers::UserBadge>(&cfg, &mut written)?;
    // game_activity.rs
    export::<game_activity::GameActivity>(&cfg, &mut written)?;
    export::<game_activity::DeadlockLiveState>(&cfg, &mut written)?;
    export::<game_activity::ActivityUpdate>(&cfg, &mut written)?;
    // collectibles.rs
    export::<collectibles::CatalogResponse>(&cfg, &mut written)?;
    export::<collectibles::Catalog>(&cfg, &mut written)?;
    export::<collectibles::Category>(&cfg, &mut written)?;
    export::<collectibles::Price>(&cfg, &mut written)?;
    export::<collectibles::EffectLayer>(&cfg, &mut written)?;
    export::<collectibles::Position>(&cfg, &mut written)?;
    export::<collectibles::Source>(&cfg, &mut written)?;
    export::<collectibles::ProfileEffect>(&cfg, &mut written)?;
    export::<collectibles::FrameLayer>(&cfg, &mut written)?;
    export::<collectibles::Frame>(&cfg, &mut written)?;
    export::<collectibles::Item>(&cfg, &mut written)?;
    export::<collectibles::AppliedUser>(&cfg, &mut written)?;
    export::<collectibles::ApplyResponse>(&cfg, &mut written)?;
    // profile_board.rs
    export::<profile_board::Board>(&cfg, &mut written)?;
    export::<profile_board::Widget>(&cfg, &mut written)?;
    export::<profile_board::BoardUserRef>(&cfg, &mut written)?;
    export::<profile_board::GuestbookNote>(&cfg, &mut written)?;
    export::<profile_board::GuestbookPreview>(&cfg, &mut written)?;
    export::<profile_board::GameTotal>(&cfg, &mut written)?;
    export::<profile_board::LastGame>(&cfg, &mut written)?;
    export::<profile_board::TopGames>(&cfg, &mut written)?;
    export::<profile_board::VoiceHours>(&cfg, &mut written)?;
    export::<profile_board::LastListen>(&cfg, &mut written)?;
    export::<profile_board::BoardBadge>(&cfg, &mut written)?;
    export::<profile_board::BoardServer>(&cfg, &mut written)?;
    export::<profile_board::DeadlockData>(&cfg, &mut written)?;
    export::<profile_board::BoardData>(&cfg, &mut written)?;
    export::<profile_board::GameWeek>(&cfg, &mut written)?;
    export::<profile_board::BoardResponse>(&cfg, &mut written)?;
    export::<profile_board::DroppedWidget>(&cfg, &mut written)?;
    export::<profile_board::BoardSaveResponse>(&cfg, &mut written)?;
    export::<profile_board::GuestbookPage>(&cfg, &mut written)?;
    export::<profile_board::ActivityData>(&cfg, &mut written)?;
    export::<profile_board::ActivityEvent>(&cfg, &mut written)?;
    export::<profile_board::ActivitySettings>(&cfg, &mut written)?;
    export::<profile_board::ActivityPage>(&cfg, &mut written)?;
    export::<profile_board::WishlistItem>(&cfg, &mut written)?;
    export::<profile_board::WishlistResponse>(&cfg, &mut written)?;
    export::<profile_board::ProfilePrefs>(&cfg, &mut written)?;
    export::<profile_board::BoardUpdate>(&cfg, &mut written)?;
    // voice_app.rs
    export::<voice_app::AutomaticEvent>(&cfg, &mut written)?;
    export::<voice_app::CatalogSound>(&cfg, &mut written)?;
    export::<voice_app::LtTrack>(&cfg, &mut written)?;
    export::<voice_app::LtRequester>(&cfg, &mut written)?;
    export::<voice_app::LtEntry>(&cfg, &mut written)?;
    export::<voice_app::LtSnapshot>(&cfg, &mut written)?;
    export::<voice_app::LtHistory>(&cfg, &mut written)?;
    export::<voice_app::WordleProgress>(&cfg, &mut written)?;
    export::<voice_app::LtSeed>(&cfg, &mut written)?;
    // rest.rs
    export::<rest::TicketAudience>(&cfg, &mut written)?;
    export::<rest::SocketTicketRequest>(&cfg, &mut written)?;
    export::<rest::IceServer>(&cfg, &mut written)?;
    export::<rest::MediaBootstrap>(&cfg, &mut written)?;
    export::<rest::CallSocket>(&cfg, &mut written)?;
    export::<rest::VoiceAdmissionRequest>(&cfg, &mut written)?;
    export::<rest::VoiceAdmission>(&cfg, &mut written)?;
    export::<rest::SocketTicket>(&cfg, &mut written)?;
    export::<rest::User>(&cfg, &mut written)?;
    export::<rest::Server>(&cfg, &mut written)?;
    export::<rest::ChannelType>(&cfg, &mut written)?;
    export::<rest::Channel>(&cfg, &mut written)?;
    export::<rest::VoiceStatus>(&cfg, &mut written)?;
    export::<rest::Category>(&cfg, &mut written)?;
    export::<rest::ChannelsResponse>(&cfg, &mut written)?;
    export::<rest::Attachment>(&cfg, &mut written)?;
    export::<rest::Reaction>(&cfg, &mut written)?;
    export::<rest::EmbedAuthor>(&cfg, &mut written)?;
    export::<rest::EmbedProvider>(&cfg, &mut written)?;
    export::<rest::EmbedImage>(&cfg, &mut written)?;
    export::<rest::EmbedMedia>(&cfg, &mut written)?;
    export::<rest::EmbedVideo>(&cfg, &mut written)?;
    export::<rest::EmbedMetrics>(&cfg, &mut written)?;
    export::<rest::EmbedAudio>(&cfg, &mut written)?;
    export::<rest::EmbedExternalCard>(&cfg, &mut written)?;
    export::<rest::ReferencedTweet>(&cfg, &mut written)?;
    export::<rest::EmbedFooter>(&cfg, &mut written)?;
    export::<rest::Embed>(&cfg, &mut written)?;
    export::<rest::UploadedFile>(&cfg, &mut written)?;
    export::<rest::GifAsset>(&cfg, &mut written)?;
    export::<rest::GifItem>(&cfg, &mut written)?;
    export::<rest::GifCategory>(&cfg, &mut written)?;
    export::<rest::GifCategories>(&cfg, &mut written)?;
    export::<rest::GifTerms>(&cfg, &mut written)?;
    export::<rest::GifPage>(&cfg, &mut written)?;
    export::<rest::ReplyTo>(&cfg, &mut written)?;
    export::<rest::Message>(&cfg, &mut written)?;
    export::<rest::CallRecord>(&cfg, &mut written)?;
    export::<rest::ThreadSummary>(&cfg, &mut written)?;
    export::<rest::MessagePatch>(&cfg, &mut written)?;
    export::<rest::MessagePage>(&cfg, &mut written)?;
    export::<rest::ReadState>(&cfg, &mut written)?;
    export::<rest::LastMessage>(&cfg, &mut written)?;
    export::<rest::ReadStates>(&cfg, &mut written)?;
    export::<rest::Notification>(&cfg, &mut written)?;
    export::<rest::NotificationUser>(&cfg, &mut written)?;
    export::<rest::Role>(&cfg, &mut written)?;
    export::<rest::Member>(&cfg, &mut written)?;
    export::<rest::DmChannel>(&cfg, &mut written)?;
    export::<rest::Relationship>(&cfg, &mut written)?;
    export::<rest::FriendRequestSent>(&cfg, &mut written)?;
    export::<rest::ProfileResponse>(&cfg, &mut written)?;
    export::<rest::MutualFriends>(&cfg, &mut written)?;
    export::<rest::MutualFriend>(&cfg, &mut written)?;
    export::<rest::MutualServers>(&cfg, &mut written)?;
    export::<rest::MutualServer>(&cfg, &mut written)?;
    export::<rest::Invite>(&cfg, &mut written)?;
    export::<rest::CreatedInvite>(&cfg, &mut written)?;
    export::<rest::InvitePreview>(&cfg, &mut written)?;
    export::<rest::InviteServer>(&cfg, &mut written)?;
    export::<rest::InviteInviter>(&cfg, &mut written)?;
    export::<rest::JoinedInvite>(&cfg, &mut written)?;
    export::<rest::Ban>(&cfg, &mut written)?;
    export::<rest::AuditLog>(&cfg, &mut written)?;
    export::<rest::PermissionOverride>(&cfg, &mut written)?;
    export::<rest::SessionInfo>(&cfg, &mut written)?;
    export::<rest::SessionActivity>(&cfg, &mut written)?;
    export::<rest::ShareItem>(&cfg, &mut written)?;
    export::<rest::ShareAuthor>(&cfg, &mut written)?;
    export::<rest::KovaSession>(&cfg, &mut written)?;
    export::<rest::KovaUser>(&cfg, &mut written)?;
    export::<rest::KovaAccount>(&cfg, &mut written)?;
    export::<rest::DeviceStartRequest>(&cfg, &mut written)?;
    export::<rest::DeviceStart>(&cfg, &mut written)?;
    export::<rest::DevicePollRequest>(&cfg, &mut written)?;
    export::<rest::DevicePollStatus>(&cfg, &mut written)?;
    export::<rest::DevicePollResponse>(&cfg, &mut written)?;
    // payload.rs
    export::<payload::Hello>(&cfg, &mut written)?;
    export::<payload::GatewayError>(&cfg, &mut written)?;
    export::<payload::Identify>(&cfg, &mut written)?;
    export::<payload::Resume>(&cfg, &mut written)?;
    export::<payload::ChatReady>(&cfg, &mut written)?;
    export::<payload::Dispatch>(&cfg, &mut written)?;
    export::<payload::MessageDeleteEvent>(&cfg, &mut written)?;
    export::<payload::MessageUnpinEvent>(&cfg, &mut written)?;
    export::<payload::ReactionEvent>(&cfg, &mut written)?;
    export::<payload::TypingStartEvent>(&cfg, &mut written)?;
    export::<payload::NotificationsRead>(&cfg, &mut written)?;
    export::<payload::ChannelRef>(&cfg, &mut written)?;
    export::<payload::PresenceStatus>(&cfg, &mut written)?;
    export::<payload::PresenceUpdateEvent>(&cfg, &mut written)?;
    export::<payload::PresenceEntry>(&cfg, &mut written)?;
    export::<payload::PresenceList>(&cfg, &mut written)?;
    export::<payload::GuildMemberAdd>(&cfg, &mut written)?;
    export::<payload::GuildMemberRemove>(&cfg, &mut written)?;
    export::<payload::RelationshipRemove>(&cfg, &mut written)?;
    export::<payload::GuildMemberUpdate>(&cfg, &mut written)?;
    export::<payload::ChannelUpdateEvent>(&cfg, &mut written)?;
    export::<payload::ChannelDeleteEvent>(&cfg, &mut written)?;
    export::<payload::GuildDelete>(&cfg, &mut written)?;
    export::<payload::UserProfileUpdate>(&cfg, &mut written)?;
    export::<payload::ServerSubscribe>(&cfg, &mut written)?;
    export::<payload::VoiceChannelJoin>(&cfg, &mut written)?;
    export::<payload::VoiceChannelLeave>(&cfg, &mut written)?;
    export::<payload::VoiceChannelMember>(&cfg, &mut written)?;
    export::<payload::VoiceChannelStates>(&cfg, &mut written)?;
    export::<payload::VoiceChannelStateUpdate>(&cfg, &mut written)?;
    export::<payload::VoiceState>(&cfg, &mut written)?;
    export::<payload::VoiceAction>(&cfg, &mut written)?;
    export::<payload::VoiceStateUpdate>(&cfg, &mut written)?;
    export::<payload::ClientVoiceState>(&cfg, &mut written)?;
    export::<payload::StreamPreviewUpdate>(&cfg, &mut written)?;
    export::<payload::MediaReady>(&cfg, &mut written)?;
    export::<payload::RoomReady>(&cfg, &mut written)?;
    export::<payload::Resumed>(&cfg, &mut written)?;
    export::<payload::VoiceIdentify>(&cfg, &mut written)?;
    export::<payload::TrackKind>(&cfg, &mut written)?;
    export::<payload::TrackInfo>(&cfg, &mut written)?;
    export::<payload::Video>(&cfg, &mut written)?;
    export::<payload::StopTracks>(&cfg, &mut written)?;
    export::<payload::ClientDisconnect>(&cfg, &mut written)?;
    export::<payload::Speaking>(&cfg, &mut written)?;
    export::<payload::VoiceReady>(&cfg, &mut written)?;
    export::<payload::CallInitiate>(&cfg, &mut written)?;
    export::<payload::CallRef>(&cfg, &mut written)?;
    export::<payload::CallRing>(&cfg, &mut written)?;
    export::<payload::CallRinging>(&cfg, &mut written)?;
    export::<payload::CallRingStop>(&cfg, &mut written)?;
    export::<payload::VoiceMove>(&cfg, &mut written)?;
    export::<payload::VoiceMoveRequest>(&cfg, &mut written)?;
    export::<payload::TrackUnavailable>(&cfg, &mut written)?;
    export::<payload::RepublishRequest>(&cfg, &mut written)?;
    unknown_json(&out)?;
    eprintln!("protocol-ts: {written} types into {}", out.display());
    Ok(())
}

/// `serde_json::Value` takes any JSON, so on the reading side it is
/// `unknown`, not ts-rs's recursive `JsonValue` (which no interface with
/// declared fields is assignable to). Rewrites the references and drops
/// the `serde_json/JsonValue.ts` file ts-rs wrote.
fn unknown_json(out: &std::path::Path) -> std::io::Result<()> {
    const IMPORT: &str = "import type { JsonValue } from \"./serde_json/JsonValue\";
";
    for entry in std::fs::read_dir(out)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "ts") {
            let text = std::fs::read_to_string(&path)?;
            if text.contains("JsonValue") {
                let text = text.replace(IMPORT, "").replace("JsonValue", "unknown");
                std::fs::write(&path, text)?;
            }
        }
    }
    let dir = out.join("serde_json");
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
