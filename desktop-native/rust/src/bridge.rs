//! The QML-facing domain objects, one QML singleton per domain: `Session`,
//! `Nav`, `Chat` and `Voice`. Each holds typed properties and forwards
//! intents to the core as [`Command`]s. The list models are their own
//! singletons in [`crate::models`]: cxx-qt 0.10 has no clean way to expose
//! a Rust-owned child QObject as a property, and a singleton needs no
//! ownership at all.
//!
//! `Session.start()` starts the core. Its view updates arrive in batches;
//! the bridge keeps at most one queued Qt-thread closure, which drains every
//! batch that arrived meanwhile and applies them in order ([`gui::apply`]).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    /// Which top-level screen shows.
    #[qenum(Session)]
    enum Phase {
        Boot,
        SignedOut,
        DeviceCode,
        Loading,
        Ready,
    }

    extern "RustQt" {
        /// Sign-in and the signed-in user.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(Phase, phase)]
        #[qproperty(QString, status)]
        #[qproperty(QString, user_code, cxx_name = "userCode")]
        #[qproperty(QString, verification_url, cxx_name = "verificationUrl")]
        #[qproperty(QString, me_id, cxx_name = "meId")]
        #[qproperty(QString, me_name, cxx_name = "meName")]
        #[qproperty(QString, me_username, cxx_name = "meUsername")]
        #[qproperty(QString, me_avatar, cxx_name = "meAvatar")]
        #[qproperty(QString, me_initials, cxx_name = "meInitials")]
        /// `online`, `idle`, `dnd` or `offline`.
        #[qproperty(QString, me_status, cxx_name = "meStatus")]
        #[qproperty(QString, me_custom_status, cxx_name = "meCustomStatus")]
        /// What we are playing as others see it ("Playing X" in the
        /// account row), or empty; its icon (absolute) or empty.
        #[qproperty(QString, me_game, cxx_name = "meGame")]
        #[qproperty(QString, me_game_icon, cxx_name = "meGameIcon")]
        /// Our avatar decoration (APNG URL), or empty.
        #[qproperty(QString, me_deco, cxx_name = "meDeco")]
        /// Our nameplate as JSON `{still, motion, border}`, or empty.
        #[qproperty(QString, me_plate, cxx_name = "mePlate")]
        /// Our profile effect and frame (JSON, as Profile has them), or empty.
        #[qproperty(QString, me_effect, cxx_name = "meEffect")]
        #[qproperty(QString, me_frame, cxx_name = "meFrame")]
        /// Our display-name style JSON, or empty.
        #[qproperty(QString, me_name_style, cxx_name = "meNameStyle")]
        /// Our stored avatar display (JSON), or empty.
        #[qproperty(QString, me_display, cxx_name = "meDisplay")]
        type Session = super::SessionRust;

        /// Starts the core (once).
        #[qinvokable]
        fn start(self: Pin<&mut Session>);

        #[qinvokable]
        #[cxx_name = "startSignIn"]
        fn start_sign_in(self: &Session);

        #[qinvokable]
        #[cxx_name = "cancelSignIn"]
        fn cancel_sign_in(self: &Session);

        #[qinvokable]
        #[cxx_name = "useToken"]
        fn use_token(self: &Session, token: &QString);

        #[qinvokable]
        #[cxx_name = "signOut"]
        fn sign_out(self: &Session);

        /// The status picker: `online`, `idle`, `dnd` or `offline`
        /// (Invisible), and the custom status (empty clears it).
        #[qinvokable]
        #[cxx_name = "setStatus"]
        fn choose_status(self: &Session, status: &QString, custom_status: &QString);
    }

    impl cxx_qt::Threading for Session {}

    extern "RustQt" {
        /// Where the user is: server, channel, and the gateway link.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, connected)]
        /// The link's retry since the last ready (0: none), and whether the
        /// last try found no network (ConnectionStrip, BootView).
        #[qproperty(i32, link_attempt, cxx_name = "linkAttempt")]
        #[qproperty(bool, link_offline, cxx_name = "linkOffline")]
        #[qproperty(QString, active_server_id, cxx_name = "activeServerId")]
        #[qproperty(QString, active_server_name, cxx_name = "activeServerName")]
        #[qproperty(QString, active_channel_id, cxx_name = "activeChannelId")]
        #[qproperty(QString, active_channel_name, cxx_name = "activeChannelName")]
        /// The open voice channel's view (landing or call); empty when none.
        #[qproperty(QString, voice_panel_id, cxx_name = "voicePanelId")]
        #[qproperty(QString, voice_panel_name, cxx_name = "voicePanelName")]
        #[qproperty(i32, voice_panel_count, cxx_name = "voicePanelCount")]
        /// Its text chat is open beside it (it is then the active channel).
        #[qproperty(bool, voice_panel_chat, cxx_name = "voicePanelChat")]
        /// Home (the DM list) instead of a server.
        #[qproperty(bool, home)]
        /// The open DM's recipient; empty in a server channel.
        #[qproperty(QString, dm_user_id, cxx_name = "dmUserId")]
        #[qproperty(QString, dm_username, cxx_name = "dmUsername")]
        #[qproperty(QString, dm_avatar, cxx_name = "dmAvatar")]
        #[qproperty(QString, dm_deco, cxx_name = "dmDeco")]
        /// Unread DMs: the rail shows three, the Home key counts the rest.
        #[qproperty(i32, dm_unread, cxx_name = "dmUnread")]
        type Nav = super::NavRust;

        #[qinvokable]
        #[cxx_name = "selectServer"]
        fn select_server(self: &Nav, id: &QString);

        /// Opens a text channel, or joins a voice channel.
        #[qinvokable]
        #[cxx_name = "selectChannel"]
        fn select_channel(self: &Nav, id: &QString);

        /// Home with no conversation open.
        #[qinvokable]
        #[cxx_name = "selectHome"]
        fn select_home(self: &Nav);

        /// The rail's Home key: the DM opened last, or Home.
        #[qinvokable]
        #[cxx_name = "selectHomeLast"]
        fn select_home_last(self: &Nav);

        /// Alt+Up/Down: the previous or next text channel in the sidebar
        /// (DMs at Home); `unread`: the previous or next unread one.
        #[qinvokable]
        #[cxx_name = "navigateChannel"]
        fn navigate_channel(self: &Nav, forward: bool, unread: bool);

        /// Ctrl+Alt+Up/Down: the previous or next server, Home first.
        #[qinvokable]
        #[cxx_name = "navigateServer"]
        fn navigate_server(self: &Nav, forward: bool);

        /// The pointer rests on a channel row (or the switcher highlights
        /// it): its newest page is fetched into the warm cache.
        #[qinvokable]
        #[cxx_name = "prefetchChannel"]
        fn prefetch_channel(self: &Nav, id: &QString);

        /// The DM with a user, created when there is none.
        #[qinvokable]
        #[cxx_name = "openDm"]
        fn open_dm(self: &Nav, user_id: &QString);

        /// The open voice channel's text chat beside its view, or not.
        #[qinvokable]
        #[cxx_name = "showVoiceChat"]
        fn show_voice_chat(self: &Nav, show: bool);

        /// The connection strip's Retry: the gateway connects again now.
        #[qinvokable]
        #[cxx_name = "retryLink"]
        fn retry_link(self: &Nav);
    }

    impl cxx_qt::Initialize for Nav {}

    extern "RustQt" {
        /// The open channel. Its rows are `MessageListModel`.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, typing_text, cxx_name = "typingText")]
        // The composer's hovered `@name`: `{name, avatar, deco}` JSON of
        // the member it names, or empty (`hoverMention`).
        #[qproperty(QString, hovered_mention, cxx_name = "hoveredMention")]
        // The `@` candidates' lowercased usernames (JSON array): a mention
        // of one edits as a unit in the composer.
        #[qproperty(QString, mention_names, cxx_name = "mentionNames")]
        // Today's local day number, to compare with a row's `day`.
        #[qproperty(i32, today)]
        // The "New" divider: the message it sits above (empty: none), the
        // banner, its count and when the first unread message was sent.
        #[qproperty(QString, divider_id, cxx_name = "dividerId")]
        // Where the next landing goes (the channel reopens where it was
        // left): this message at the top, or empty for the divider or end.
        #[qproperty(QString, restore_id, cxx_name = "restoreId")]
        #[qproperty(bool, unread_banner, cxx_name = "unreadBanner")]
        #[qproperty(i32, unread_count, cxx_name = "unreadCount")]
        #[qproperty(QString, unread_since_time, cxx_name = "unreadSinceTime")]
        #[qproperty(QString, unread_since_date, cxx_name = "unreadSinceDate")]
        #[qproperty(i32, unread_since_day, cxx_name = "unreadSinceDay")]
        // Older pages: one is loading; there are more to load.
        #[qproperty(bool, older_loading, cxx_name = "olderLoading")]
        #[qproperty(bool, has_older, cxx_name = "hasOlder")]
        // A jump's window short of the newest end (detached): newer pages
        // exist; one is loading. `hasNewer` shows Jump to present.
        #[qproperty(bool, has_newer, cxx_name = "hasNewer")]
        #[qproperty(bool, newer_loading, cxx_name = "newerLoading")]
        // Delete is offered on others' messages too (MANAGE_MESSAGES).
        #[qproperty(bool, can_manage_messages, cxx_name = "canManageMessages")]
        // The open channel refuses our messages: the composer's lock line.
        #[qproperty(bool, read_only, cxx_name = "readOnly")]
        // The newest page is loading (the skeleton while the list is empty).
        #[qproperty(bool, page_loading, cxx_name = "pageLoading")]
        // The composer tray: finished uploads (sent with the next message)
        // and ones still going.
        #[qproperty(i32, uploads_ready, cxx_name = "uploadsReady")]
        #[qproperty(i32, uploads_busy, cxx_name = "uploadsBusy")]
        // The GIF picker: a page is loading, more exist, the error line,
        // and each column's height (thousandths of the width) and tiles.
        #[qproperty(bool, gif_loading, cxx_name = "gifLoading")]
        #[qproperty(bool, gif_more, cxx_name = "gifMore")]
        #[qproperty(QString, gif_error, cxx_name = "gifError")]
        #[qproperty(i32, gif_units0, cxx_name = "gifUnits0")]
        #[qproperty(i32, gif_units1, cxx_name = "gifUnits1")]
        #[qproperty(i32, gif_count0, cxx_name = "gifCount0")]
        #[qproperty(i32, gif_count1, cxx_name = "gifCount1")]
        // The saved GIF and clip favourites, a JSON array of `provider:id`.
        #[qproperty(QString, gif_favorites, cxx_name = "gifFavorites")]
        // The rest of the picker as JSON (yap-core `gif_rows`): provider,
        // favourites mode, rate limit, the start screen's categories and
        // Favorites/Trending media, and the search terms.
        #[qproperty(QString, gif_browse, cxx_name = "gifBrowse")]
        type Chat = super::ChatRust;

        /// A download landed at `path`; `open` it (else reveal it).
        #[qsignal]
        fn saved(self: Pin<&mut Chat>, path: QString, open: bool);

        /// Adds the user's reaction, or removes it when it is there.
        #[qinvokable]
        #[cxx_name = "toggleReaction"]
        fn toggle_reaction(self: &Chat, message_id: &QString, emoji: &QString);

        /// Local files for the open channel's tray (picked, pasted, dropped).
        #[qinvokable]
        #[cxx_name = "addFiles"]
        fn add_files(self: &Chat, paths: &QStringList);

        /// Cancels an upload or removes its tile.
        #[qinvokable]
        #[cxx_name = "removeUpload"]
        fn remove_upload(self: &Chat, id: &QString);

        #[qinvokable]
        #[cxx_name = "retryUpload"]
        fn retry_upload(self: &Chat, id: &QString);

        #[qinvokable]
        #[cxx_name = "setUploadSensitive"]
        fn set_upload_sensitive(self: &Chat, id: &QString, nsfw: bool);

        /// A GIF picker query (empty: trending) for `gifs`, `stickers`,
        /// `clips` or `memes`, from `klipy` or `tenor`.
        #[qinvokable]
        #[cxx_name = "searchGifs"]
        fn search_gifs(self: &Chat, query: &QString, media_type: &QString, provider: &QString);

        /// The picker opened: results kept from its last open are dropped.
        #[qinvokable]
        #[cxx_name = "resetGifs"]
        fn reset_gifs(self: &Chat);

        /// A small UI memory kept on this device (the web's localStorage),
        /// as JSON: `<name>.json` in the app folder; empty when missing.
        /// `name` is `[a-z0-9-]`.
        #[qinvokable]
        #[cxx_name = "uiMemory"]
        fn ui_memory(self: &Chat, name: &QString) -> QString;

        /// Replaces that memory with `json`.
        #[qinvokable]
        #[cxx_name = "setUiMemory"]
        fn set_ui_memory(self: &Chat, name: &QString, json: &QString);

        /// The picker's start screen: categories and the Trending tile.
        #[qinvokable]
        #[cxx_name = "startGifs"]
        fn start_gifs(self: &Chat, media_type: &QString, provider: &QString);

        /// The picker shows the saved favourites of a media type.
        #[qinvokable]
        #[cxx_name = "showGifFavorites"]
        fn show_gif_favorites(self: &Chat, media_type: &QString);

        /// Search terms for what is typed (`gifBrowse.terms`).
        #[qinvokable]
        #[cxx_name = "suggestGifs"]
        fn suggest_gifs(self: &Chat, query: &QString, provider: &QString);

        /// A tile's star (`GifListModel.gifId`).
        #[qinvokable]
        #[cxx_name = "toggleGifTileFavorite"]
        fn toggle_gif_tile_favorite(self: &Chat, gif_id: &QString);

        #[qinvokable]
        #[cxx_name = "moreGifs"]
        fn more_gifs(self: &Chat);

        /// Sends a GIF tile (`GifListModel.gifId`) to the open channel.
        #[qinvokable]
        #[cxx_name = "sendGif"]
        fn send_gif(self: &Chat, gif_id: &QString, reply_to_id: &QString);

        /// A star showed: the favourites load once.
        #[qinvokable]
        #[cxx_name = "loadGifFavorites"]
        fn load_gif_favorites(self: &Chat);

        /// A star was pressed with the item it saves (`GifPickerItem` JSON).
        #[qinvokable]
        #[cxx_name = "toggleGifFavorite"]
        fn toggle_gif_favorite(self: &Chat, item: &QString);

        /// Clears one of our messages' link previews for everyone.
        #[qinvokable]
        #[cxx_name = "removeEmbeds"]
        fn remove_embeds(self: &Chat, message_id: &QString);

        /// Saves `url` to `dest` (a path from `Files`); `open` afterwards.
        #[qinvokable]
        fn download(self: &Chat, url: &QString, dest: &QString, open: bool);

        /// Something failed that the user should hear about (a toast).
        #[qsignal]
        fn notice(self: Pin<&mut Chat>, text: QString);

        /// Someone in the open channel typed again (ChatTypingIndicator.tsx:
        /// its dots run until 6 s pass without one).
        #[qsignal]
        #[cxx_name = "typingPinged"]
        fn typing_pinged(self: Pin<&mut Chat>);

        /// The divider was placed after the list had landed: land again.
        #[qsignal]
        fn reland(self: Pin<&mut Chat>);

        /// Posts to the open channel; `reply_to_id` may be empty.
        #[qinvokable]
        #[cxx_name = "sendMessage"]
        fn send_message(self: &Chat, text: &QString, reply_to_id: &QString);

        #[qinvokable]
        #[cxx_name = "retrySend"]
        fn retry_send(self: &Chat, nonce: &QString);

        #[qinvokable]
        #[cxx_name = "discardSend"]
        fn discard_send(self: &Chat, nonce: &QString);

        #[qinvokable]
        #[cxx_name = "editMessage"]
        fn edit_message(self: &Chat, id: &QString, text: &QString);

        #[qinvokable]
        #[cxx_name = "deleteMessage"]
        fn delete_message(self: &Chat, id: &QString);

        /// The composer has text; call at most every 3 s.
        #[qinvokable]
        #[cxx_name = "sendTyping"]
        fn send_typing(self: &Chat);

        #[qinvokable]
        #[cxx_name = "loadOlder"]
        fn load_older(self: &Chat);

        #[qinvokable]
        #[cxx_name = "setAtBottom"]
        fn set_at_bottom(self: &Chat, at_bottom: bool);

        /// The top-most message on screen, reported with the end.
        #[qinvokable]
        #[cxx_name = "setSeenFrom"]
        fn set_seen_from(self: &Chat, message_id: &QString);

        /// The composer's pictures (InputMentionOverlay.tsx): a JSON array
        /// of `[start, end, url]`, UTF-16 offsets into `text`.
        #[qinvokable]
        #[cxx_name = "composerEmoji"]
        fn composer_emoji(self: &Chat, text: &QString) -> QString;

        /// The top-most message on screen while scrolled up (the channel
        /// reopens there).
        #[qinvokable]
        #[cxx_name = "scrollAt"]
        fn scroll_at(self: &Chat, message_id: &QString);

        #[qinvokable]
        #[cxx_name = "setFocused"]
        fn set_focused(self: &Chat, focused: bool);

        #[qinvokable]
        #[cxx_name = "markRead"]
        fn mark_read(self: &Chat);

        /// The rail menu's Mark as read: the server's channels and
        /// notifications.
        #[qinvokable]
        #[cxx_name = "markServerRead"]
        fn mark_server_read(self: &Chat, server_id: &QString);

        /// The channel menu's Mark as read.
        #[qinvokable]
        #[cxx_name = "markChannelRead"]
        fn mark_channel_read(self: &Chat, channel_id: &QString);

        /// A category label's click: collapse or expand it (kept per user).
        #[qinvokable]
        #[cxx_name = "toggleCategory"]
        fn toggle_category(self: &Chat, category_id: &QString);

        /// Filters `MentionListModel` by username; an empty query closes it.
        #[qinvokable]
        #[cxx_name = "queryMentions"]
        fn query_mentions(self: &Chat, query: &QString, open: bool);

        /// The composer's pointer is over `@username` (empty: over none).
        #[qinvokable]
        #[cxx_name = "hoverMention"]
        fn hover_mention(self: &Chat, username: &QString);

        /// The newest of our own sent messages (Up to edit), or empty.
        #[qinvokable]
        #[cxx_name = "lastEditableId"]
        fn last_editable_id(self: &Chat) -> QString;

        /// A message's row in `MessageListModel`, or -1.
        #[qinvokable]
        #[cxx_name = "indexOfMessage"]
        fn index_of_message(self: &Chat, id: &QString) -> i32;

        /// Shows a message: its channel (and server) opens first; a loaded
        /// message is brought into view, anything else loads the window
        /// around it. Then `jumped` names it. Search results, deep links,
        /// pins and reply quotes all come here.
        #[qinvokable]
        #[cxx_name = "jumpToMessage"]
        fn jump_to_message(self: &Chat, channel_id: &QString, message_id: &QString);

        /// The message to centre and highlight (accent tint, then a 300 ms
        /// fade), once its row is in `MessageListModel`.
        #[qsignal]
        fn jumped(self: Pin<&mut Chat>, message_id: QString);

        /// The detached list is near its end: load the page after it.
        #[qinvokable]
        #[cxx_name = "loadNewer"]
        fn load_newer(self: &Chat);

        /// "Jump to present": the newest page again.
        #[qinvokable]
        #[cxx_name = "jumpToPresent"]
        fn jump_to_present(self: &Chat);

        /// The web link to a message of the open channel ("Copy message
        /// link"): `{origin}/chat/{server or @me}/{channel}?message={id}`.
        #[qinvokable]
        #[cxx_name = "messageLink"]
        fn message_link(self: &Chat, message_id: &QString) -> QString;
    }

    impl cxx_qt::Initialize for Chat {}

    extern "RustQt" {
        /// The call. Its members are `VoiceRosterModel` (and member rows in
        /// `ChannelTreeModel`).
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, channel_id, cxx_name = "channelId")]
        #[qproperty(QString, channel_name, cxx_name = "channelName")]
        #[qproperty(QString, status)]
        // The media path is up.
        #[qproperty(bool, live)]
        #[qproperty(bool, muted)]
        #[qproperty(bool, deafened)]
        /// We are speaking (lights our avatar ring and LEDs).
        #[qproperty(bool, speaking)]
        /// Why the last call failed; empty once a join starts.
        #[qproperty(QString, error)]
        /// The camera is wanted (the capture runs) and on (published).
        #[qproperty(bool, camera_wanted, cxx_name = "cameraWanted")]
        #[qproperty(bool, camera_on, cxx_name = "cameraOn")]
        /// Why the camera could not go on: `missing`, `busy`, `unknown`, or
        /// the call's error; empty otherwise.
        #[qproperty(QString, camera_error, cxx_name = "cameraError")]
        /// A share was asked for and its capture is starting.
        #[qproperty(bool, share_pending, cxx_name = "sharePending")]
        /// Sharing (`self_stream`): the live share's source, kind, preset
        /// and whether it carries audio (`self_stream_audio`).
        #[qproperty(bool, sharing)]
        #[qproperty(QString, share_source_id, cxx_name = "shareSourceId")]
        #[qproperty(QString, share_source_name, cxx_name = "shareSourceName")]
        #[qproperty(bool, share_window, cxx_name = "shareWindow")]
        #[qproperty(QString, share_quality, cxx_name = "shareQuality")]
        #[qproperty(bool, share_audio, cxx_name = "shareAudio")]
        /// Why the last share failed; empty otherwise.
        #[qproperty(QString, share_error, cxx_name = "shareError")]
        /// What can be shared, as JSON: `[{id, name, kind, appName, ours}]`
        /// (`get_screen_sources`), monitors first, our own windows last, as
        /// the last `refreshShareSources` found it; the revision counts the
        /// refreshes that landed (changed or not).
        #[qproperty(QString, share_sources, cxx_name = "shareSources")]
        #[qproperty(i32, share_sources_revision, cxx_name = "shareSourcesRevision")]
        /// The spatial panel's room map as JSON, `{state, seats}`: the
        /// room's `SharedSpatialAudioState` and everyone's seat as the mix
        /// places them (`{userId, me, x, y, enabled, highFidelity}`, us
        /// first); empty outside a call.
        #[qproperty(QString, spatial)]
        type Voice = super::VoiceRust;

        #[qinvokable]
        fn join(self: &Voice, channel_id: &QString);

        #[qinvokable]
        fn leave(self: &Voice);

        #[qinvokable]
        #[cxx_name = "requestMute"]
        fn set_muted_from_ui(self: &Voice, muted: bool);

        #[qinvokable]
        #[cxx_name = "requestDeafen"]
        fn set_deafened_from_ui(self: &Voice, deafened: bool);

        /// Watch (or stop watching) a participant's share.
        #[qinvokable]
        fn watch(self: &Voice, participant_id: &QString, watch: bool);

        /// Our camera on or off in the call.
        #[qinvokable]
        #[cxx_name = "setCamera"]
        fn set_camera_from_ui(self: &Voice, on: bool);

        /// The capture failed (`missing`, `busy`, `unknown`): the camera
        /// goes off with that reason.
        #[qinvokable]
        #[cxx_name = "cameraFailed"]
        fn camera_failed(self: &Voice, reason: &QString);

        /// Lists the shareable sources again, off the GUI thread, into
        /// `shareSources`. One at a time: a call while one runs is dropped.
        #[qinvokable]
        #[cxx_name = "refreshShareSources"]
        fn refresh_share_sources(self: Pin<&mut Voice>);

        /// Thumbnails and capture only for our own windows
        /// (`YAP_SHARE_ONLY_OURS=1`, for tests).
        #[qinvokable]
        #[cxx_name = "shareOnlyOurs"]
        fn share_only_ours(self: &Voice) -> bool;

        /// Go live with a picked source (`window`: a window, else a screen).
        #[qinvokable]
        #[cxx_name = "startShare"]
        fn start_share(
            self: &Voice,
            source_id: &QString,
            source_name: &QString,
            window: bool,
            quality: &QString,
            audio: bool,
        );

        #[qinvokable]
        #[cxx_name = "stopShare"]
        fn stop_share(self: &Voice);

        #[qinvokable]
        #[cxx_name = "changeShareAudio"]
        fn change_share_audio(self: &Voice, on: bool);

        #[qinvokable]
        #[cxx_name = "changeShareQuality"]
        fn change_share_quality(self: &Voice, quality: &QString);

        /// The spatial panel's edit (SpatialAudioPanel.tsx `commit`): the
        /// room's whole new state as JSON. Heard at once and sent to the
        /// room; `spatial` follows.
        #[qinvokable]
        #[cxx_name = "updateSpatial"]
        fn update_spatial(self: &Voice, state: &QString);

        /// An id for a video surface to report its size under.
        #[qinvokable]
        #[cxx_name = "newSurface"]
        fn new_surface(self: &Voice) -> i32;

        /// Surface `surface` shows `key` at this many device pixels; an
        /// empty key or a zero size withdraws it.
        #[qinvokable]
        #[cxx_name = "reportVideo"]
        fn report_video(self: &Voice, surface: i32, key: &QString, width: i32, height: i32);

        /// The participant whose camera or share is focused (empty: none).
        #[qinvokable]
        #[cxx_name = "setStage"]
        fn set_stage(self: &Voice, participant_id: &QString);
    }

    impl cxx_qt::Initialize for Voice {}
    impl cxx_qt::Threading for Voice {}

    extern "RustQt" {
        /// The user popout: who it shows, filled in as their profile loads.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, user_id, cxx_name = "userId")]
        #[qproperty(QString, name)]
        #[qproperty(QString, username)]
        #[qproperty(QString, avatar)]
        #[qproperty(QString, initials)]
        #[qproperty(QString, presence)]
        #[qproperty(QString, custom_status, cxx_name = "customStatus")]
        /// "Playing a game": JSON `{name, icon, details, state, startedAt}`,
        /// or empty.
        #[qproperty(QString, game)]
        #[qproperty(QString, pronouns)]
        #[qproperty(QString, bio)]
        #[qproperty(QString, banner)]
        /// `#rrggbb`, or empty for the accent tint.
        #[qproperty(QString, banner_color, cxx_name = "bannerColor")]
        /// Roles in the shown server, highest first, with their colours.
        #[qproperty(QStringList, role_names, cxx_name = "roleNames")]
        #[qproperty(QStringList, role_colors, cxx_name = "roleColors")]
        #[qproperty(QStringList, role_ids, cxx_name = "roleIds")]
        #[qproperty(bool, me)]
        /// The avatar decoration (APNG URL), or empty.
        #[qproperty(QString, deco)]
        /// The profile effect and frame as JSON, or empty.
        #[qproperty(QString, effect)]
        #[qproperty(QString, frame)]
        /// The display-name style JSON, or empty.
        #[qproperty(QString, name_style, cxx_name = "nameStyle")]
        /// Where they are signed in, comma-joined (`desktop,mobile,web`).
        #[qproperty(QString, platforms)]
        /// The servers you share: `{count, items: [{id, name, icon,
        /// initial}]}`, or empty.
        #[qproperty(QString, mutual_servers, cxx_name = "mutualServers")]
        #[qproperty(QString, mutual_friends, cxx_name = "mutualFriends")]
        #[qproperty(QString, member_since, cxx_name = "memberSince")]
        #[qproperty(QString, load_error, cxx_name = "loadError")]
        type Profile = super::ProfileRust;

        #[qinvokable]
        fn show(self: &Profile, user_id: &QString);

        /// Closes the popout if it still shows `user_id`.
        #[qinvokable]
        fn close(self: &Profile, user_id: &QString);

        /// The pointer rests on what opens `user_id`'s popout: their
        /// profile loads now, so the card opens with its banner.
        #[qinvokable]
        fn prefetch(self: &Profile, user_id: &QString);
    }

    impl cxx_qt::Initialize for Profile {}
}

use core::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QList, QString, QStringList};
use yap_core::store::favorites::FavoritesCommand;
use yap_core::store::time::format_rfc3339;
use yap_core::{Phase, ViewUpdate};

use crate::client::{self, Command};
use crate::gui;

impl Default for qobject::Phase {
    fn default() -> Self {
        Self::Boot
    }
}

impl From<Phase> for qobject::Phase {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Boot => Self::Boot,
            Phase::SignedOut => Self::SignedOut,
            Phase::DeviceCode => Self::DeviceCode,
            Phase::Loading => Self::Loading,
            Phase::Ready => Self::Ready,
        }
    }
}

#[derive(Default)]
pub struct SessionRust {
    phase: qobject::Phase,
    status: QString,
    user_code: QString,
    verification_url: QString,
    me_id: QString,
    me_name: QString,
    me_username: QString,
    me_avatar: QString,
    me_initials: QString,
    me_status: QString,
    me_custom_status: QString,
    me_game: QString,
    me_game_icon: QString,
    me_deco: QString,
    me_plate: QString,
    me_effect: QString,
    me_frame: QString,
    me_name_style: QString,
    me_display: QString,
}

impl Drop for SessionRust {
    fn drop(&mut self) {
        client::shutdown();
    }
}

#[derive(Default)]
pub struct NavRust {
    connected: bool,
    link_attempt: i32,
    link_offline: bool,
    active_server_id: QString,
    active_server_name: QString,
    active_channel_id: QString,
    active_channel_name: QString,
    voice_panel_id: QString,
    voice_panel_name: QString,
    voice_panel_count: i32,
    voice_panel_chat: bool,
    home: bool,
    dm_user_id: QString,
    dm_username: QString,
    dm_avatar: QString,
    dm_deco: QString,
    dm_unread: i32,
}

#[derive(Default)]
pub struct ChatRust {
    typing_text: QString,
    hovered_mention: QString,
    mention_names: QString,
    today: i32,
    divider_id: QString,
    restore_id: QString,
    unread_banner: bool,
    unread_count: i32,
    unread_since_time: QString,
    unread_since_date: QString,
    unread_since_day: i32,
    older_loading: bool,
    has_older: bool,
    has_newer: bool,
    newer_loading: bool,
    can_manage_messages: bool,
    read_only: bool,
    page_loading: bool,
    uploads_ready: i32,
    uploads_busy: i32,
    gif_loading: bool,
    gif_more: bool,
    gif_error: QString,
    gif_units0: i32,
    gif_units1: i32,
    gif_count0: i32,
    gif_count1: i32,
    gif_favorites: QString,
    gif_browse: QString,
}

#[derive(Default)]
pub struct VoiceRust {
    channel_id: QString,
    channel_name: QString,
    status: QString,
    live: bool,
    muted: bool,
    deafened: bool,
    speaking: bool,
    error: QString,
    camera_wanted: bool,
    camera_on: bool,
    camera_error: QString,
    share_pending: bool,
    sharing: bool,
    share_source_id: QString,
    share_source_name: QString,
    share_window: bool,
    share_quality: QString,
    share_audio: bool,
    share_error: QString,
    share_sources: QString,
    share_sources_revision: i32,
    spatial: QString,
    /// A source refresh is running.
    share_refreshing: bool,
}

/// View updates waiting for the Qt thread, and whether a closure that will
/// drain them is already queued.
#[derive(Default)]
struct Pending {
    updates: Vec<ViewUpdate>,
    queued: bool,
}

impl qobject::Session {
    pub fn start(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        let pending = Arc::new(Mutex::new(Pending::default()));
        let sink: client::Sink = Box::new(move |batch| {
            let mut waiting = pending.lock().unwrap_or_else(PoisonError::into_inner);
            waiting.updates.extend(batch);
            if std::mem::replace(&mut waiting.queued, true) {
                return;
            }
            drop(waiting);
            let drain = Arc::clone(&pending);
            let queued = qt.queue(move |session| {
                let updates = {
                    let mut waiting = drain.lock().unwrap_or_else(PoisonError::into_inner);
                    waiting.queued = false;
                    std::mem::take(&mut waiting.updates)
                };
                gui::apply(session, updates);
            });
            // Fails only once the Session is gone (shutdown): nothing will
            // drain, so drop what is waiting instead of growing forever.
            if queued.is_err() {
                let mut waiting = pending.lock().unwrap_or_else(PoisonError::into_inner);
                waiting.queued = false;
                waiting.updates.clear();
            }
        });
        client::start(sink);
    }

    pub fn start_sign_in(&self) {
        client::command(Command::StartSignIn);
    }

    pub fn cancel_sign_in(&self) {
        client::command(Command::CancelSignIn);
    }

    pub fn use_token(&self, token: &QString) {
        client::command(Command::UseToken(token.to_string()));
    }

    pub fn sign_out(&self) {
        client::command(Command::SignOut);
    }

    pub fn choose_status(&self, status: &QString, custom_status: &QString) {
        let custom = custom_status.to_string();
        client::command(Command::SetStatus {
            status: status.to_string(),
            custom_status: (!custom.trim().is_empty()).then_some(custom),
        });
    }
}

impl qobject::Nav {
    pub fn select_server(&self, id: &QString) {
        client::command(Command::SelectServer(id.to_string()));
    }

    pub fn select_channel(&self, id: &QString) {
        client::command(Command::SelectChannel(id.to_string()));
    }

    pub fn select_home(&self) {
        client::command(Command::SelectHome);
    }

    pub fn select_home_last(&self) {
        client::command(Command::SelectHomeLast);
    }

    pub fn navigate_channel(&self, forward: bool, unread: bool) {
        client::command(Command::NavigateChannel { forward, unread });
    }

    pub fn navigate_server(&self, forward: bool) {
        client::command(Command::NavigateServer { forward });
    }

    pub fn prefetch_channel(&self, id: &QString) {
        client::command(Command::PrefetchChannel(id.to_string()));
    }

    pub fn open_dm(&self, user_id: &QString) {
        client::command(Command::OpenDm(user_id.to_string()));
    }

    pub fn show_voice_chat(&self, show: bool) {
        client::command(Command::ShowVoiceChat(show));
    }

    pub fn retry_link(&self) {
        client::command(Command::RetryGateway);
    }
}

#[derive(Default)]
pub struct ProfileRust {
    user_id: QString,
    name: QString,
    username: QString,
    avatar: QString,
    initials: QString,
    presence: QString,
    custom_status: QString,
    game: QString,
    pronouns: QString,
    bio: QString,
    banner: QString,
    banner_color: QString,
    role_names: QStringList,
    role_colors: QStringList,
    role_ids: QStringList,
    me: bool,
    deco: QString,
    effect: QString,
    frame: QString,
    name_style: QString,
    platforms: QString,
    mutual_servers: QString,
    mutual_friends: QString,
    member_since: QString,
    load_error: QString,
}

impl qobject::Profile {
    pub fn show(&self, user_id: &QString) {
        client::command(Command::ShowProfile(Some(user_id.to_string())));
    }

    pub fn close(&self, user_id: &QString) {
        client::command(Command::HideProfile(user_id.to_string()));
    }

    pub fn prefetch(&self, user_id: &QString) {
        client::command(Command::PrefetchProfile(user_id.to_string()));
    }
}

/// A nonce no other send from this process shares: the wall clock in
/// nanoseconds and a counter.
fn next_nonce(now: Duration) -> String {
    static SENT: AtomicU64 = AtomicU64::new(0);
    let n = SENT.fetch_add(1, Ordering::Relaxed);
    format!("native-{:x}-{n:x}", now.as_nanos())
}

/// A fresh nonce and the local send time, as the composer stamps a send.
pub(crate) fn stamp() -> (String, String) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let ms = i64::try_from(now.as_millis()).unwrap_or(i64::MAX);
    (next_nonce(now), format_rfc3339(ms))
}

fn optional(text: &QString) -> Option<String> {
    let text = text.to_string();
    (!text.is_empty()).then_some(text)
}

impl qobject::Chat {
    pub fn send_message(&self, text: &QString, reply_to_id: &QString) {
        let (nonce, sent_at) = stamp();
        client::command(Command::Send {
            content: text.to_string(),
            reply_to_id: optional(reply_to_id),
            nonce,
            sent_at,
        });
    }

    pub fn toggle_reaction(&self, message_id: &QString, emoji: &QString) {
        client::command(Command::ToggleReaction {
            message_id: message_id.to_string(),
            emoji: emoji.to_string(),
        });
    }

    pub fn add_files(&self, paths: &QStringList) {
        let paths: Vec<String> = QList::<QString>::from(paths)
            .iter()
            .map(|p| p.to_string())
            .filter(|p| !p.is_empty())
            .collect();
        let files = crate::media::upload_files(&paths);
        if !files.is_empty() {
            client::command(Command::AddUploads(files));
        }
    }

    pub fn remove_upload(&self, id: &QString) {
        client::command(Command::RemoveUpload(id.to_string()));
    }

    pub fn retry_upload(&self, id: &QString) {
        client::command(Command::RetryUpload(id.to_string()));
    }

    pub fn set_upload_sensitive(&self, id: &QString, nsfw: bool) {
        client::command(Command::SetUploadSensitive {
            local_id: id.to_string(),
            nsfw,
        });
    }

    pub fn search_gifs(&self, query: &QString, media_type: &QString, provider: &QString) {
        client::command(Command::GifSearch {
            query: query.to_string(),
            media_type: media_type.to_string(),
            provider: provider.to_string(),
        });
    }

    pub fn reset_gifs(&self) {
        client::command(Command::GifReset);
    }

    pub fn ui_memory(&self, name: &QString) -> QString {
        let Some(file) = ui_memory_file(&name.to_string()) else {
            return QString::default();
        };
        yap_cache::settings::app_dir()
            .and_then(|dir| yap_cache::settings::load_app_file(&dir, &file))
            .map(|value| QString::from(value.to_string().as_str()))
            .unwrap_or_default()
    }

    pub fn set_ui_memory(&self, name: &QString, json: &QString) {
        let (Some(file), Some(dir)) = (
            ui_memory_file(&name.to_string()),
            yap_cache::settings::app_dir(),
        ) else {
            return;
        };
        if let Err(error) = yap_cache::settings::save_app_file(&dir, &file, &json.to_string()) {
            eprintln!("[ui-memory] {file}: {error}");
        }
    }

    pub fn start_gifs(&self, media_type: &QString, provider: &QString) {
        client::command(Command::GifStart {
            media_type: media_type.to_string(),
            provider: provider.to_string(),
        });
    }

    pub fn show_gif_favorites(&self, media_type: &QString) {
        client::command(Command::GifFavorites {
            media_type: media_type.to_string(),
        });
    }

    pub fn suggest_gifs(&self, query: &QString, provider: &QString) {
        client::command(Command::GifTerms {
            query: query.to_string(),
            provider: provider.to_string(),
        });
    }

    pub fn toggle_gif_tile_favorite(&self, gif_id: &QString) {
        client::command(Command::GifTileFavorite(gif_id.to_string()));
    }

    pub fn more_gifs(&self) {
        client::command(Command::GifMore);
    }

    pub fn load_gif_favorites(&self) {
        client::command(Command::Favorites(FavoritesCommand::Load));
    }

    pub fn toggle_gif_favorite(&self, item: &QString) {
        match serde_json::from_str::<serde_json::Value>(&item.to_string()) {
            Ok(item) if item.is_object() => {
                client::command(Command::Favorites(FavoritesCommand::Toggle(item)));
            }
            _ => eprintln!("[gif] favourite: not an item"),
        }
    }

    pub fn remove_embeds(&self, message_id: &QString) {
        client::command(Command::RemoveEmbeds(message_id.to_string()));
    }

    pub fn send_gif(&self, gif_id: &QString, reply_to_id: &QString) {
        let (nonce, sent_at) = stamp();
        client::command(Command::SendGif {
            gif_id: gif_id.to_string(),
            reply_to_id: optional(reply_to_id),
            nonce,
            sent_at,
        });
    }

    pub fn download(&self, url: &QString, dest: &QString, open: bool) {
        client::command(Command::Download {
            url: url.to_string(),
            dest: dest.to_string(),
            open,
        });
    }

    pub fn retry_send(&self, nonce: &QString) {
        client::command(Command::RetrySend(nonce.to_string()));
    }

    pub fn discard_send(&self, nonce: &QString) {
        client::command(Command::DiscardSend(nonce.to_string()));
    }

    pub fn edit_message(&self, id: &QString, text: &QString) {
        client::command(Command::EditMessage {
            message_id: id.to_string(),
            content: text.to_string(),
        });
    }

    pub fn delete_message(&self, id: &QString) {
        client::command(Command::DeleteMessage(id.to_string()));
    }

    pub fn send_typing(&self) {
        client::command(Command::Typing);
    }

    pub fn load_older(&self) {
        client::command(Command::LoadOlder);
    }

    pub fn set_at_bottom(&self, at_bottom: bool) {
        client::command(Command::SetAtBottom(at_bottom));
    }

    pub fn set_seen_from(&self, message_id: &QString) {
        client::command(Command::SeenFrom(message_id.to_string()));
    }

    pub fn composer_emoji(&self, text: &QString) -> QString {
        let text = text.to_string();
        let spans: Vec<(usize, usize, String)> =
            yap_core::emoji::composer_emoji(&text, &yap_core::config::get().api_base)
                .into_iter()
                .map(|s| (s.start, s.end, s.url))
                .collect();
        QString::from(serde_json::to_string(&spans).unwrap_or_default().as_str())
    }

    pub fn scroll_at(&self, message_id: &QString) {
        client::command(Command::ScrollAt(message_id.to_string()));
    }

    pub fn set_focused(&self, focused: bool) {
        client::command(Command::SetFocused(focused));
    }

    pub fn mark_read(&self) {
        client::command(Command::MarkRead);
    }

    pub fn mark_server_read(&self, server_id: &QString) {
        client::command(Command::MarkServerRead(server_id.to_string()));
    }

    pub fn mark_channel_read(&self, channel_id: &QString) {
        client::command(Command::MarkChannelRead(channel_id.to_string()));
    }

    pub fn toggle_category(&self, category_id: &QString) {
        client::command(Command::ToggleCategory(category_id.to_string()));
    }

    pub fn query_mentions(&self, query: &QString, open: bool) {
        client::command(Command::MentionQuery(open.then(|| query.to_string())));
    }

    pub fn hover_mention(&self, username: &QString) {
        let name = username.to_string();
        client::command(Command::MentionHover((!name.is_empty()).then_some(name)));
    }

    pub fn last_editable_id(&self) -> QString {
        gui::with_rows(|rows| {
            rows.messages
                .iter()
                .rev()
                .find(|r| r.mine && !r.pending && !r.failed)
                .map(|r| QString::from(r.id.as_str()))
                .unwrap_or_default()
        })
    }

    pub fn jump_to_message(&self, channel_id: &QString, message_id: &QString) {
        client::command(Command::JumpToMessage {
            channel_id: channel_id.to_string(),
            message_id: message_id.to_string(),
        });
    }

    pub fn load_newer(&self) {
        client::command(Command::LoadNewer);
    }

    pub fn jump_to_present(&self) {
        client::command(Command::JumpToPresent);
    }

    pub fn message_link(&self, message_id: &QString) -> QString {
        let (server, channel) = gui::link_route();
        let origin = &yap_core::config::get().api_base;
        let link = yap_core::store::message_link(
            origin,
            server.as_deref(),
            &channel,
            &message_id.to_string(),
        );
        QString::from(link.as_str())
    }

    pub fn index_of_message(&self, id: &QString) -> i32 {
        let id = id.to_string();
        gui::with_rows(|rows| {
            rows.messages
                .iter()
                .position(|r| r.id == id)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1)
        })
    }
}

impl qobject::Voice {
    pub fn join(&self, channel_id: &QString) {
        client::command(Command::JoinVoice(channel_id.to_string()));
    }

    pub fn leave(&self) {
        client::command(Command::LeaveVoice);
    }

    pub fn set_muted_from_ui(&self, muted: bool) {
        client::command(Command::SetVoiceMuted(muted));
    }

    pub fn set_deafened_from_ui(&self, deafened: bool) {
        client::command(Command::SetVoiceDeafened(deafened));
    }

    pub fn watch(&self, participant_id: &QString, watch: bool) {
        client::command(Command::WatchStream {
            participant_id: participant_id.to_string(),
            watch,
        });
    }

    pub fn set_camera_from_ui(&self, on: bool) {
        client::command(Command::SetVoiceCamera(on));
    }

    pub fn camera_failed(&self, reason: &QString) {
        client::command(Command::VoiceCameraFailed(reason.to_string()));
    }

    pub fn refresh_share_sources(mut self: Pin<&mut Self>) {
        if std::mem::replace(&mut self.as_mut().rust_mut().share_refreshing, true) {
            return;
        }
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let json = crate::share::sources_json();
            let _ = qt.queue(move |mut voice| {
                voice.as_mut().rust_mut().share_refreshing = false;
                let json = QString::from(json.as_str());
                if *voice.share_sources() != json {
                    voice.as_mut().set_share_sources(json);
                }
                let revision = voice.share_sources_revision().wrapping_add(1);
                voice.set_share_sources_revision(revision);
            });
        });
    }

    pub fn share_only_ours(&self) -> bool {
        yap_capture::only_ours()
    }

    pub fn start_share(
        &self,
        source_id: &QString,
        source_name: &QString,
        window: bool,
        quality: &QString,
        audio: bool,
    ) {
        client::command(Command::StartVoiceShare(yap_core::VoiceShare {
            source_id: source_id.to_string(),
            source_name: source_name.to_string(),
            window,
            quality: quality.to_string(),
            audio,
        }));
    }

    pub fn stop_share(&self) {
        client::command(Command::StopVoiceShare);
    }

    pub fn change_share_audio(&self, on: bool) {
        client::command(Command::SetVoiceShareAudio(on));
    }

    pub fn change_share_quality(&self, quality: &QString) {
        client::command(Command::SetVoiceShareQuality(quality.to_string()));
    }

    pub fn update_spatial(&self, state: &QString) {
        client::command(Command::SetVoiceSpatial(state.to_string()));
    }

    pub fn new_surface(&self) -> i32 {
        crate::video::new_surface()
    }

    pub fn report_video(&self, surface: i32, key: &QString, width: i32, height: i32) {
        let px = |n: i32| u32::try_from(n).unwrap_or(0);
        crate::video::report(surface, &key.to_string(), px(width), px(height));
    }

    pub fn set_stage(&self, participant_id: &QString) {
        crate::video::set_stage(&participant_id.to_string());
    }
}

macro_rules! registered {
    ($($object:ident => $rust:ident),* $(,)?) => {$(
        impl cxx_qt::Initialize for qobject::$object {
            fn initialize(self: Pin<&mut Self>) {
                gui::register(self);
            }
        }

        impl Drop for $rust {
            fn drop(&mut self) {
                gui::unregister::<qobject::$object>();
            }
        }
    )*};
}

registered!(
    Nav => NavRust,
    Chat => ChatRust,
    Voice => VoiceRust,
    Profile => ProfileRust,
);

/// `ui-<name>.json` for a `[a-z0-9-]` name, so QML can only reach its own
/// files in the app folder.
fn ui_memory_file(name: &str) -> Option<String> {
    let ok = !name.is_empty()
        && name.len() <= 48
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    ok.then(|| format!("ui-{name}.json"))
}
