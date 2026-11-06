# Yap voice wire protocol: the native client

Read from code on `spike/qt-desktop` (2026-11-04). Every claim cites `file:line`.
Paths are relative to the repo root. "UNVERIFIED" means the repo does not show it
(usually Cloudflare Realtime API behaviour behind the proxy).

## The specs come first (2026-11-05)

The web team's contracts now live next to the code, and they win over this file
where the two differ:

- `packages/voice/STATE_MACHINE.md`: phases, teardown order and plan, server-initiated
  ends, the remote reconnect grace, voice-state deltas, the publication reconciler.
- `packages/voice/SCREEN_SHARE.md`: track names, watching a share, keepalive park and reuse.
- `packages/voice/DEVICES.md`: device choice, fallback and output routing.
- `realtime/ADMISSION.md`: the call sockets' tickets, the voice token and TURN, from one
  `POST /api/voice/admission` per join (section 1).
- `realtime/SOCKET_CORE.md`: which opcodes each socket accepts. A frame on the wrong
  socket gets `4001 "Unknown opcode"`.
- `realtime/VOICE_ROSTER.md`: the room object owns sidebar voice occupancy; op 33/34/15
  on the global socket no longer add, remove or update anyone.
- Simulcast: `src/lib/voice/video-layers.ts`, `video-render-demand.ts`, `video-keepalive.ts`,
  `realtime/partytracks-proxy.ts`.

What `yap-voice`/`yap-core` do now, and where they still differ:

| Topic                             | Native client                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | Spec                                                              |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| Sends per socket                  | global 0, 3, 7, 26, 27, 28, 33, 34, 35, 36, 37, 38, 39; room 0, 3, 7, 11, 15, 40; voice 100, 3, 5, 11, 12, 13, 106, 109. All are in the role tables. Op 13 goes out for our own withdrawn camera and share names only (leave relies on op 11).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | SOCKET_CORE.md tables                                             |
| Global op 15                      | Not sent any more. It carried mute/deafen, which no longer reaches the sidebar.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | VOICE_ROSTER.md "Old clients"                                     |
| Op 33 / 34                        | One op 33 when the room says Ready, again after a fresh room Ready mid-call. No re-send when the chat gateway re-identifies. Op 34 only for a joined call.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | STATE_MACHINE.md note 1, Teardown                                 |
| Phases                            | `joined` on the room's Ready. `reconnecting` on a room drop or the peer connection going `disconnected`. A voice socket drop only changes the status line.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | STATE_MACHINE.md Transitions                                      |
| Peer connection `failed`          | Rebuilt as PartyTracks does (`machine.rs` `rebuild`): the session and its worker, pulls and video sessions are dropped, the ADM is given back and media opens again on a new SFU session; the mic (and a camera that was on, on its next push) is pushed and announced (op 12) under the new capability, pulls run again once it connects, a live share ends ("The call connection was reset."). Three rebuilds without a connect in between end the call. `YAP_TEST_PC_FAIL_AFTER_SECS=<n>` fails the connection once, `n` s after it first connects (test hook).                                                                                                                                                                                                                                                                                                                     | PartyTracks rebuilds and republishes                              |
| Teardown                          | Stop the mic, then wait 100 ms (joined) or no time at all. Then op 11 on room, op 11 on voice, close room, close voice, close the peer connection. No leave sound or op 34 for a call that never joined.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | STATE_MACHINE.md Teardown                                         |
| Leave trigger (op 106 soundboard) | The leave sound (auto-soundboard.ts) goes out on the voice socket before the op 11s, only when the voice socket is ready and a leave sound is set; the join sound once per call after the first VoiceReady.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | Teardown step 2                                                   |
| Remote op 11                      | 3 s grace (was 15 s). Speaking clears at once. The roster row always stays: only the room removes people. On expiry, only the tracks they did not offer again are dropped.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | REMOTE_RECONNECT_GRACE_MS                                         |
| Deltas                            | Merged over the existing row. A delta for an unknown participant is dropped (it used to show "Someone").                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | Voice-state deltas                                                |
| Republish                         | The mic and the camera are published. Mute is `set_enabled`. A voice reconnect re-announces op 12 (the mic, and the camera while it is on) because the peer connection keeps its publications.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | Reconnect republish                                               |
| Our camera                        | One send-only VP8 transceiver (VP8 first, as Chrome offers) with rids `f`/`h`/`q` at scale 1/2/4, `h` capped at 500 kbps, `q` at 150 kbps and 15 fps, pushed with a UUID track name and announced as `cam-video-<pid>` with the negotiated `simulcast_rids`; op 15 `self_video` follows it. Off: parked on 64x36 black frames each second, op 13 for the alias, `self_video: false`. On again: the same SFU track, announced again; a keepalive that stalled 25 s pushes afresh (the old transceiver is detached, not closed). Capture is Qt Multimedia (`cpp/camera.cpp`); `YAP_FAKE_CAMERA=1` is a test pattern. No backgrounds.                                                                                                                                                                                                                                                     | video-layers.ts, video-keepalive.ts, partytracks-media-manager.ts |
| Our share                         | SCREEN_SHARE.md browser path, one step at a time, latest request wins. Windows Graphics Capture (`crates/yap-capture`, cursor on, no border, paced to the preset) into one send-only VP8 transceiver, one layer at the preset's `maxBitrate`/`maxFramerate`, screencast source, degradation from the content hint (window: motion, monitor: detail), last frame repeated after 1 s still. Pushed with a UUID name, announced as `screen-video-<pid>`; audio (WASAPI loopback: system mix for monitors, process tree for windows) as `screen-audio-<pid>`. Op 15 `self_stream` / `self_stream_audio`. A new source releases the old capture first and keeps the tracks. Stop parks the video on the keepalive (resumed by the next share), retires the audio, op 13 for both names. Senders are retired unfed, never `set_track(None)` (webrtc-sys 0.3.52 dereferences the null track). | screen-share-sources.ts, publication-media.ts                     |
| Cameras                           | Pulled once a visible tile renders them, with `simulcast.preferredRid` for the tile size (f/h/q). A size change goes through `tracks/update`, using `priorityOrdering: "none"` and then a settle update 2 s later. Upgrades wait 150 ms, downgrades 1 s. A hidden camera switches to `q` and its frames are dropped; after 15 s unseen its pull is released (see Video pulls). Camera off (op 13) parks the pull; the same SFU track coming back unparks it.                                                                                                                                                                                                                                                                                                                                                                                                                           | video-layers.ts, partytracks-media-manager.ts                     |
| Shares                            | Video is pulled only after Watch (or when on stage). Unwatching hides it, and 15 s later releases the pull (see Video pulls). Watching sends op 106 `stream.watch.start` / `stop`. Share audio is always pulled; its gain is 0 unless the owner's share or camera is on stage.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | SCREEN_SHARE.md Watching                                          |
| DM calls (1:1)                    | Ringing is the chat gateway's: op 36 `{target_user_id, channel_id}`, 37/38/39 `{call_id}`; `CALL_RING`, `CALL_RINGING`, `CALL_RING_STOP` drive the ring (store `calls.rs`). The call's room is `dm-call-<a>-<b>` (sorted user ids); it is joined like a voice channel with one admission `{channelId: <dm>, roomSlug, sockets, media}` (no `serverId`), op 33/34 name the DM channel, no connect sound (the ring stop plays `callConnect`). A caller whose ring times out or is declined stays in the room, as the web does.                                                                                                                                                                                                                                                                                                                                                           | chat-gateway.ts, useCallStore.ts                                  |
| Voice move                        | Room op 42 (`VoiceMove`) is reported (`Notice::Moved`); the 4010 "Moved to another voice channel" that follows ends the call quietly, and the store leaves and joins the target with the move's grant: the admission's `moveGrant`, then op 33's `move_grant` once. The mover: `POST /api/voice/move {server_id, user_id, from_channel_id, to_channel_id}` from "Move to…" on a call tile or Talk Tree leaf, offered with MOVE_MEMBERS (+ View, + Connect on the target).                                                                                                                                                                                                                                                                                                                                                                                                              | voice-move-client.ts, useFollowVoiceMove.ts                       |
| Track self-heal                   | A pull that keeps failing on `not_found_track_error` sends op 109 `{participant_id, track_name, sfu_track_name, session_id}`, at most 3 times until a pull of it works. Op 110 for our mic or live camera, naming our current SFU track and session, pushes it again under a new UUID (at most once per 5 s per track) and announces it (op 12); anything else is ignored (`stale`, `not-published`, `rate-limited`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | sfu-client.ts `handleRepublishRequest`                            |
| Standby                           | Opt-in (`YAP_VOICE_STANDBY=on`): the device mic swapped onto the standby's sender on promotion does not capture yet (InitRecording works, StartRecording is refused after the standby ran with recording off), so the promoted call sends nothing until the outbound sentinel republishes. With it on, the open voice landing of a channel someone else is in, while we are in no call, holds a hidden standby: room Identify with `standby: true`, the voice socket connected but not identified, an SFU session with a silent stand-in for the mic (recording off). Join promotes it: op 15 `{promote_standby, self_mute, self_deaf}`, op 100 on the open voice socket, the device mic swapped onto the same sender; a standby idle 25 s+ republishes its mic first. Leaving the landing (or joining elsewhere) leaves it.                                                           | voice-standby.ts                                                  |
| Stats and sentinels               | Every 5 s the stats snapshot feeds the voice details (bitrate, loss, jitter, round trip, the SFU edge, remote tracks, our outbound video) and two sentinels: no inbound audio for 30 s while someone speaks, and no outbound RTP for 30 s from an unmuted mic (that one also republishes the mic). Either shows the web's "We can't hear you" banner.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | audio-sentinel.ts, VoiceDetailsPanel.tsx                          |
| Disconnect beacon                 | On quit, a call in a server voice channel posts `/api/channels/{id}/voice-disconnect {server_id, voice_session_id}` (800 ms at most), then the call is left.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | voice-disconnect-beacon.ts                                        |
| Video pulls                       | A video pull nothing shows for `VIDEO_RELEASE_DELAY` (15 s; a camera with no visible tile, a share neither watched nor on stage) is closed at the SFU. Pulling that alias again later runs on a receive-only peer connection of its own (a video session: `open_receiver`, its own `sessions/new`, mids `v<n>:<mid>`), which drains 3 s after its pull closes and goes, so no recycled m-line ever reaches libwebrtc. Withdrawn pulls are still parked, not closed.                                                                                                                                                                                                                                                                                                                                                                                                                    | split-video-transport.ts ("fresh"), useVoiceChannel.ts:208-218    |

The closest existing reference is the mobile app (`apps/mobile/src/features/voice/`),
a port of the web client without the browser-only parts. Its join order is
summarised at `apps/mobile/src/features/voice/session.ts:44-71`.

## 0. Overview: two sockets, one peer connection

A call uses **two WebSockets and one WebRTC peer connection**:

| Piece        | URL                                                     | Server                                 | Purpose                                                     |
| ------------ | ------------------------------------------------------- | -------------------------------------- | ----------------------------------------------------------- |
| Room socket  | `wss://<api>/api/channels/<roomSlug>/ws?v=1`            | MeetingRoom DO                         | Roster, mute state, **the join grant**, voice-token refresh |
| Voice socket | `wss://<api>/api/channels/<roomSlug>/voice?v=1`         | VoiceRoom DO                           | Track announcements, speaking flags                         |
| SFU          | `https://<api>/api/channels/<roomSlug>/partytracks/...` | Worker proxy, then Cloudflare Realtime | SDP and tracks                                              |

Routing: `custom-server-entry.ts:396-399` (partytracks), `:423-438` (ws), `:441-456` (voice).
The path segment is the **roomSlug**, not the channel id. The router names it
`channelId`, but checks it against the ticket's `roomSlug` (`custom-server-entry.ts:270-278`,
`src/lib/voice/socket-ticket.ts:103-105`). For a server voice channel:
`roomSlug = "voice-" + serverId + "-" + channelId` (`src/lib/api-helpers.ts:57-62`,
`apps/mobile/src/features/voice/protocol.ts:234-236`).

You need the room socket even for audio only: the VoiceRoom will not identify you,
and the proxy will not let you pull, until the MeetingRoom has a non-standby Identify
from you (section 2.4).

## 1. Call admission (tickets, voice token, TURN)

`realtime/ADMISSION.md` is the contract: request and response, ticket claims, what each socket does
with an admitted ticket, the client flow and the errors. Read it there; this section only records how
the native client follows it (2026-11-05). The deprecated two-request flow on
`POST /api/voice/socket-ticket` (`audience: "room" | "voice"`, `media_bootstrap`) is not used any more,
with no fallback. The chat gateway keeps `socket-ticket` with `{"audience":"global"}`, as ADMISSION.md says
(`crates/yap-core/src/api.rs` `gateway_ticket`).

Native specifics:

- **Join** (`CallMachine::start`, `driver.rs` `admit`): one `POST /api/voice/admission`
  `{channelId, serverId, roomSlug, sockets:["room","voice"], media:true}`
  (`VoiceAdmissionRequest::join`). From the reply: `room_ticket`, `voice_ticket` and `media`
  (`participant_id`, `voice_token`, `ice_servers`; no ICE servers falls back to
  `stun:stun.cloudflare.com:3478`). A reply missing a ticket or `media` ends the call ("The call admission
  was incomplete."). Both sockets open at once, with the voice socket's op 100 built from `media`.
  The room's Identify gets no MediaReady and a Ready without credentials; the client does not read them.
- **Reconnects** (`driver.rs` `call_tickets`): each socket asks for its own ticket alone, minting nothing.
  Room: `{..., sockets:["room"], media:false}`. Voice: `{..., sockets:["voice"], media:false,
participantId:<ours>}`. The participant is fixed for the call, so the voice binding is fixed too. The
  Resume reply (op 9) carries no credentials and the held ones stay. Each request logs
  `[voice/room] admission: sockets=room` or `[voice/voice] admission: sockets=voice bound` (never the reply).
- **Credential refresh**: `CREDENTIAL_REFRESH` (45 min) after each voice token arrives (the admission, then
  each op 40 reply), the machine sends op 40 on the room socket and arms again. A fresh token goes to the
  SFU's `Authorization` header (`SetVoiceToken`) and to the next op 100. A voice 4004 still sends op 40 at once.
- **1013** (`VOICE_JOIN_UNVERIFIED_CLOSE_CODE`): a plain retry in `yap-realtime`'s close table. The voice
  socket reconnects on a fresh ticket bound to our participant and identifies again; the machine sees a voice
  drop and a VoiceReady, and re-announces. 4008 and 4010 still stop the call.
- **Standby promotion** (Client flow 5) and **`moveGrant`** are followed (see the table above).
- **Not followed**: prefetch. A room Resume refused with 4006 does Identify on the same socket, as
  ADMISSION.md says, but a Ready for a new participant still ends the call ("The call connection was reset.
  Join again."): only a failed peer connection rebuilds the SFU session. The 401 retry after an app-token
  refresh is whatever `yap-core`'s `Api` does for every request.

### 1.3 WebSocket handshake (both sockets)

- Subprotocols: `["ralph.realtime.v1", "ralph.ticket.<ticket>"]` (`src/lib/voice/socket-ticket.ts:152-157`).
  Any other protocol entry, or a missing or duplicate one, is rejected (`:159-195`). The server answers
  `Sec-WebSocket-Protocol: ralph.realtime.v1` (`custom-server-entry.ts:293-294`, `realtime/voice-room.ts:516-519`).
- `Origin` is **required**. It must be the API's own origin, `http(s)://tauri.localhost`, or an entry in
  `REALTIME_ALLOWED_ORIGINS` (`custom-server-entry.ts:214-228`). Otherwise the server returns HTTP 403. A native
  client should send `Origin: https://<api host>`. (The mobile app never sets it; I assume RN's socket adds
  one. UNVERIFIED.)
- Bad ticket: HTTP 401 before the upgrade (`custom-server-entry.ts:257-286`).

## 2. Gateway framing (both sockets)

Text frames carry JSON `{"op": <int>, "d": <object>}` (`packages/protocol/src/codec.ts:57-68`). Opcodes are at
`packages/protocol/src/opcodes.ts:15-135`, and close/error codes at `:162-189`.

### 2.1 Hello and heartbeat

- The server sends Hello first: `{"op":8,"d":{"heartbeat_interval":15000,"gateway_version":1}}`
  (`realtime/voice-room.ts:508-514`, `realtime/meeting-room.ts:728`). Both DOs use 15 s
  (`realtime/voice-room.ts:176`, `realtime/meeting-room.ts:293`). The client default is 45 s if the field is missing
  (`packages/protocol/src/server-payloads.ts:23`, `packages/realtime/src/realtime-socket.ts:185-195`).
- Heartbeat: send exactly `{"op":3}` every `heartbeat_interval`, starting at Hello
  (`packages/protocol/src/opcodes.ts:150`, `packages/realtime/src/realtime-socket.ts:244,584-588`).
  The server replies `{"op":6,"d":{"seq":0}}` (`realtime/voice-room.ts:2306-2319`). The web client
  reconnects after 3 missed ACKs (`packages/realtime/src/realtime-socket.ts:251`, `packages/realtime/src/heartbeat.ts:67-79`).
  The server drops a participant after 90 s without a heartbeat (`realtime/voice-room.ts:177`, `realtime/meeting-room.ts:299`).

### 2.2 Error frames and close codes

- Error frame: `{"op":18,"d":{"code":<n>,"message":"..."}}` (`packages/protocol/src/server-payloads.ts:30-37`).
- 4000 InvalidPayload, 4001 UnknownOpcode, 4003 NotAuthenticated, 4004 AuthenticationFailed (bad or expired
  voice token; the socket stays open), 4005 AlreadyAuthenticated, 4006 SessionInvalid (room Resume failed;
  Identify again), **4008 AdmissionRejected (stop, do not reconnect)**, **4010 SessionReplaced (stop, do not
  reconnect)**, 4290 RateLimited (`packages/protocol/src/opcodes.ts:162-189`). The client stops for good on close
  4010 or 4008, or on `1000` with reason "Replaced by new connection" (`packages/realtime/src/realtime-socket.ts:197-202`).
- **4010: one voice session per user per room.** Your Identify ends the user's other sessions in the same room,
  for example their web tab (`realtime/meeting-room.ts:2371-2418`, `:3001`). The VoiceRoom then closes their voice
  socket with 4010 (`realtime/voice-room.ts:1737-1761`).

### 2.3 Room socket (`/ws`, MeetingRoom)

1. Receive Hello.
2. Send Identify (op 0) (`src/lib/voice/gateways/room-gateway.ts:169-189,315-325`;
   schema `packages/protocol/src/client-payloads.ts:40-58`):
   ```json
   {
     "op": 0,
     "d": {
       "name": "<display name>",
       "username": "<username>",
       "display_name": null,
       "avatar_url": null,
       "platform": "desktop",
       "supports_voice_state_deltas": true
     }
   }
   ```
   Only `name` (string) is required. **Do not send `standby: true`**: a standby session gets no join grant
   (`realtime/meeting-room.ts:2877-2884`). The server overrides the profile fields with stored ones (`:2931-2937`).
   `platform`: unknown values fall back to `"web"` (`:2929`). Whether `"desktop"` is accepted is UNVERIFIED; mobile sends `"mobile"`
   (`apps/mobile/src/features/voice/session.ts:262`).
3. On an admission ticket with `media: "issued"` (the join) no MediaReady comes. On a `held` ticket (a reconnect)
   an Identify is a new participant and gets **MediaReady** (op 41) `{participant_id, ice_servers, voice_token}`
   first (ADMISSION.md).
4. Receive **Ready** (op 2) `{participant_id, participants: VoiceState[], heartbeat_interval, spatial_audio_state}`, plus
   `ice_servers` and `voice_token` only when the ticket was `held`
   (`:3007-3019`). The roster excludes you (`apps/mobile/src/features/voice/session.ts:666`). VoiceState carries
   `{id, user_id, name, username, display_name, avatar_url, self_mute, self_deaf, ...}` (`apps/mobile/src/features/voice/protocol.ts:50-61`).
   Map a track's `participant_id` to the roster entry whose `id` matches.
5. The server starts you **muted** (`self_mute: true`, `realtime/meeting-room.ts:2979`). Send your real state
   (`apps/mobile/src/features/voice/session.ts:679-680,991-999`; schema `packages/protocol/src/client-payloads.ts:65-77`):
   ```json
   {
     "op": 15,
     "d": {
       "self_mute": false,
       "self_deaf": false,
       "self_video": false,
       "self_stream": false,
       "self_stream_audio": false
     }
   }
   ```
   This is roster and UI state. I found no code that gates audio playback on it (`src/lib` only has the type and
   the sender: `src/lib/voice/gateways/room-gateway.ts:327-332`). Whether others hear you without it is UNVERIFIED.
6. Server events after that: VoiceStateUpdate (op 15) `{participant, action:"join"|"leave"|"update"}`, and
   ProfileUpdate (op 16) (`apps/mobile/src/features/voice/protocol.ts:89-100`).
7. Voice-token refresh: send `{"op":40,"d":{}}`. The reply is **Resumed** (op 9) `{voice_token, ice_servers, participants}`
   (`realtime/meeting-room.ts:3397-3417`). Web and mobile send it 45 min after the token arrived (ADMISSION.md).
   Use the new token on the next PartyTracks request and VoiceIdentify (`apps/mobile/src/features/voice/transport.ts:103-105`).
8. Leave: send `{"op":11,"d":{}}`, then close (`apps/mobile/src/features/voice/gateways.ts:142-150`,
   `realtime/meeting-room.ts:1101-1103`). This revokes your grant, and the VoiceRoom drops you (`realtime/voice-room.ts:533-542`).

### 2.4 Voice socket (`/voice`, VoiceRoom)

1. Receive Hello.
2. Send VoiceIdentify (op 100) with the admission's `media` credentials (`src/lib/voice/gateways/voice-gateway.ts:314-346`;
   schema `packages/protocol/src/client-payloads.ts:228-233`):
   ```json
   {
     "op": 100,
     "d": {
       "participant_id": "<media.participant_id>",
       "voice_token": "<voice_token>"
     }
   }
   ```
   Both fields are required. `supports_listen_together_snapshot_events` is optional; skip it.
   - The token must match `participant_id` and roomSlug, and its subject must match the voice ticket's user
     (`realtime/voice-room.ts:2090-2097`). A subject mismatch gets error 4008 and a close (`:2113-2126`). An expired or invalid
     token gets error **4004 and the socket stays open**: refresh (2.3 step 7) and identify again
     (`:2128-2143`, `src/lib/voice/gateways/voice-gateway.ts:209-222`).
   - **Join-required rule:** the VoiceRoom waits **up to 10 s** for the MeetingRoom's join grant
     (`realtime/voice-room.ts:2146-2153`, `realtime/voice-join-grant.ts:22`). That grant is pushed when your room Identify
     (non-standby) arrives (`realtime/voice-join-grant.ts:7-11`, `realtime/meeting-room.ts:2880-2884`). If no grant comes, you get
     error 4008 "Voice channel join required" and a **close with 4008** (`realtime/voice-room.ts:2156-2171`). You may send
     VoiceIdentify before room Ready, since the web client does so right after the ticket (`src/lib/sfu-client.ts:353-378`). The
     room Identify must still go out first or alongside.
   - A second identify for the same participant evicts the older socket with close `1000 "Replaced by new connection"`
     (`realtime/voice-room.ts:2201-2214`).
3. Receive **VoiceReady** (op 101) (`realtime/voice-room.ts:2289-2297`):
   ```json
   {"op":101,"d":{"participant_id":"<you>","tracks":[TrackInfo...],"speaking":{"<pid>":1},"join_wait_ms":12}}
   ```
   `tracks` holds every live track except yours (`:2241-2261`). Until VoiceReady, every op except heartbeat gets
   error 4003 "Not identified on voice" (`:1834-1847`). **Queue outgoing ops until VoiceReady**, as both clients do
   (`src/lib/voice/gateways/base-gateway.ts:99-110`, `apps/mobile/src/features/voice/gateways.ts:319-322`).
   VoiceReady also sends some VoiceAppEvents (op 106: stream-watch snapshot, demo chat). Ignore them
   (`realtime/voice-room.ts:2298-2299`).
4. **Announce your published track** with Video (op 12) (`src/lib/sfu-client.ts:796-810`,
   `apps/mobile/src/features/voice/session.ts:507-521`; schema `packages/protocol/src/client-payloads.ts:236-263`):
   ```json
   {
     "op": 12,
     "d": {
       "tracks": [
         {
           "participant_id": "<you>",
           "track_name": "cam-audio-<you>",
           "sfu_track_name": "<trackName from the push response>",
           "session_id": "<session capability from sessions/new>",
           "kind": "audio"
         }
       ]
     }
   }
   ```
   - Required: `participant_id`, `track_name` (1-200 chars), `session_id`, `kind` (`"audio"|"video"`). `sfu_track_name` is optional
     in the schema but **required by the handler**: without it you get "Browser SFU negotiation is unsupported"
     (`realtime/voice-room.ts:2703-2711`). `mid` is optional and gets dropped.
   - The server checks that `session_id` is a valid capability for this roomSlug **and your participant id**
     (`:2732-2749`), and that no other participant owns `track_name` (`:2722-2731`). It overwrites `participant_id` with yours (`:2750-2756`),
     upserts the track, and sends Video to everyone else (`:2759-2783`). The rate limit is 20 Video ops per minute (`:194-203`).
   - Send it again whenever the push metadata changes, for example after a new PartyTracks session (`src/lib/voice/partytracks-media-manager.ts:290-298`).
     The web client also re-announces on every VoiceReady (`src/lib/sfu-client.ts:544-547`).
5. Learn about others' tracks:
   - Initial list: `VoiceReady.d.tracks`.
   - New or changed: Video (op 12) `{"participant_id":"<pid>","tracks":[TrackInfo]}` (`realtime/voice-room.ts:2777-2783`).
     If a track has no `participant_id`, use the outer one (`src/lib/voice/gateways/voice-gateway.ts:196-206`).
   - Removed: StopTracks (op 13) `{"participant_id","track_names":[...],"tracks":[TrackInfo]}` (`realtime/voice-room.ts:3015-3031`).
   - Participant gone: ClientDisconnect (op 11) `{"participant_id"}`, which also implies a speaking reset (`realtime/voice-room.ts:4066-4083`).
     Drop all of that participant's pulls (`src/lib/sfu-client.ts:627-639`).
   - Wire shape of TrackInfo: `{participant_id, track_name, sfu_track_name?, session_id, mid?, kind}` (`apps/mobile/src/features/voice/protocol.ts:24-32`).
     **Skip tracks without `sfu_track_name`** (legacy or native screen) and your own (`src/lib/voice/partytracks-media-manager.ts:359-366`).
     For audio only, pull `kind == "audio"`. Mobile always pulls audio, and pulls video only when it is watched (`apps/mobile/src/features/voice/session.ts:58-60`).
6. Speaking (optional, for others' speaking indicators): send `{"op":5,"d":{"speaking":1}}` / `{"op":5,"d":{"speaking":0}}`.
   Bit 0 is MICROPHONE (`src/lib/types.ts:39-45`). The server stores it and relays `{participant_id, speaking}` to everyone else
   (`realtime/voice-room.ts:2403-2424`). Media does not need it.
7. Leave: send StopTracks `{"op":13,"d":{"track_names":["cam-audio-<you>"]}}` (optional), then `{"op":11,"d":{}}`, then close
   (`src/lib/sfu-client.ts:1139-1142`, `apps/mobile/src/features/voice/gateways.ts:329-336`). The server purges your tracks and
   sends StopTracks and ClientDisconnect to the others (`realtime/voice-room.ts:726-728,3508-3517`). A bare socket close does the same
   (`:746-757`).

## 3. SFU over the PartyTracks proxy

The prefix is `https://<api>/api/channels/<roomSlug>/partytracks` (`src/lib/voice/partytracks-transport.ts:128-130`).
The proxy forwards to `https://rtc.live.cloudflare.com/v1/apps/<CALLS_APP_ID>/<rest of path>`, adds its own Bearer token,
and keeps the query string (`node_modules/partytracks/dist/server/index.js:6-11,28-31`). It runs with
`lockSessionToInitiator: false`, so no cookies are involved (`realtime/partytracks-proxy.ts:455`). Request and response bodies are
**Cloudflare Realtime API JSON**. Their exact fields beyond what PartyTracks reads are UNVERIFIED in this repo.

### 3.1 Headers on every request

```
Authorization: Bearer <voice_token>
X-Voice-Session-Id: <participant_id>
Content-Type: application/json
```

(`src/lib/voice/partytracks-transport.ts:118-121`, `realtime/partytracks-proxy.ts:8,331-347`.) Without either header, or with a token
that does not verify against `participant_id` and roomSlug, the proxy returns 401 `{"error":"Unauthorized"}`. The proxy strips both
headers and `Cookie` before forwarding (`:387-391`). PartyTracks itself sets no Content-Type (`node_modules/partytracks/dist/client/index.js:235-244`). The
proxy re-sets it to JSON after it rewrites the body (`realtime/partytracks-proxy.ts:408`), so sending it yourself is safe.

### 3.2 Session capabilities (how IDs are rewritten)

- You **never see a raw Cloudflare session id**. `sessions/new` returns `sessionId` replaced by a capability:
  `base64url(JSON{version:1, roomSlug, participantId, sessionId}).base64url(HMAC-SHA256)` (`realtime/partytracks-proxy.ts:141-154,460-477`).
- **URL path**: `/sessions/<capability>/...`. The proxy verifies that the capability is for this roomSlug **and for your
  `X-Voice-Session-Id`**, else 403. It then swaps in the raw id (`:357-385`). URL-encode the capability. It is base64url plus `.`,
  so in practice it is URL-safe.
- **JSON body**: every key named `sessionId`, at any depth, must be a valid capability for this roomSlug, else 403. It is decoded
  to the raw id (`:206-249,393-419`). If any decoded capability belongs to **another** participant (a pull), the proxy asks the
  VoiceRoom whether you hold a join grant, waiting up to 10 s, else 403 (`:420-432,43-63`, `realtime/voice-room.ts:547-560`).
- **Responses**: raw `sessionId` values the request used are mapped back to capabilities (`realtime/partytracks-proxy.ts:251-271,479-481`).
- So, in practice: announce your own capability as `session_id` in Video. Put a remote track's `session_id` (the capability you
  received) in the pull body. Use your own capability in the URL.

### 3.3 Calls (as `partytracks@0.0.56`, patched, makes them; there is one peer connection per call)

**(a) Create a session**: `POST {prefix}/sessions/new?`, no body
(`node_modules/partytracks/dist/client/index.js:601-608`). Response: `{"sessionId":"<capability>", ...}`. Then create
`RTCPeerConnection({iceServers: media.ice_servers, bundlePolicy: "max-bundle"})` (`:611-614`). If you pass ICE servers,
PartyTracks skips `GET {prefix}/generate-ice-servers` (`:608`). The web client always passes them (`src/lib/voice/partytracks-transport.ts:132-136`).

**(b) Push the microphone (client offers)** (`node_modules/partytracks/dist/client/index.js:257-307,318-331`):

1. `addTransceiver(micTrack, {direction: "sendonly"})`, then `createOffer`, then `setLocalDescription`.
2. `POST {prefix}/sessions/<myCap>/tracks/new?`
   ```json
   {
     "sessionDescription": { "sdp": "<offer>", "type": "offer" },
     "tracks": [
       {
         "trackName": "<random UUID>",
         "mid": "<transceiver.mid>",
         "location": "local"
       }
     ]
   }
   ```
   PartyTracks uses `crypto.randomUUID()` as the SFU `trackName` (`:327,344`). This is **not** the `cam-audio-...` alias. It
   batches up to 32 tracks per request (`:225-227`).
3. Response: `{"sessionDescription":{"type":"answer","sdp":"..."},"tracks":[{"trackName","mid",...}], "errorCode"?}`. If there is
   no `errorCode`, `setRemoteDescription(answer)` and wait for `signalingState == "stable"` (`:281-285`).
4. The track metadata becomes `{trackName, sessionId: <myCap>, location: "remote"}` (`:288-297`). Announce it at once with
   Video (op 12): `sfu_track_name = trackName`, `session_id = sessionId` (`src/lib/voice/partytracks-transport.ts:197-211`). The patch
   announces before RTP flows (`node_modules/partytracks/dist/client/index.js:290-292`). Listeners retry briefly (`:434-440`).

**(c) Pull a remote track (SFU offers, client answers)** (`:369-441`):

1. **Wait until the peer connection has connected** if it has already negotiated (for example your push). A pull that renegotiates
   earlier makes the SFU return `not_found_track_error` (`:374-380,552-584`).
2. `POST {prefix}/sessions/<myCap>/tracks/new?`
   ```json
   {
     "tracks": [
       {
         "location": "remote",
         "trackName": "<their sfu_track_name>",
         "sessionId": "<their session_id capability>"
       }
     ]
   }
   ```
   (`:381-384`, metadata mapping `src/lib/voice/partytracks-transport.ts:80-86`.)
3. Response: `{"tracks":[{"trackName","sessionId","mid",...}],"requiresImmediateRenegotiation":true,"sessionDescription":{"type":"offer","sdp":"..."}}`.
   An `errorCode` or `errorDescription` means the pull failed: retry with backoff, starting at 100 ms
   (`node_modules/partytracks/dist/client/index.js:385,439`).
4. If `requiresImmediateRenegotiation`: `setRemoteDescription(offer)`, then `createAnswer`, then `setLocalDescription`, then
   `PUT {prefix}/sessions/<myCap>/renegotiate?` with `{"sessionDescription":{"type":"answer","sdp":"<answer>"}}`. Check for `errorCode`,
   then wait for stable (`:398-411`).
5. The remote audio arrives on the transceiver whose `mid` equals the response's `mid`, through the `track` event (`:387-396,510-522`).

**(d) Close tracks (client offers)** (`:480-508`): only while `connectionState == "connected"`. Call `transceiver.stop()`, then
`createOffer`, then `setLocalDescription`, then `PUT {prefix}/sessions/<myCap>/tracks/close?`:

```json
{
  "tracks": [{ "mid": "<mid>" }],
  "sessionDescription": { "sdp": "<offer>", "type": "offer" },
  "force": false
}
```

Response: `{"sessionDescription":{"type":"answer",...}}`, then `setRemoteDescription`. On leave, the web and mobile clients just
close the peer connection (`src/lib/voice/partytracks-transport.ts:176-195`). The VoiceRoom StopTracks or ClientDisconnect tells
listeners to stop pulling.

**Recovery**: on `connectionState` `failed` or `closed`, on `iceConnectionState` `failed`, or on `disconnected` for more than 7 s,
PartyTracks discards the session and starts again from (a). That means a new capability, re-pushed tracks, and a new Video
announcement (`node_modules/partytracks/dist/client/index.js:615-640`). A listener whose pulls keep failing can send TrackUnavailable
(op 109). The publisher then receives RepublishRequest (op 110) `{track_name, sfu_track_name?, session_id?}`. The fix is to push again
with a new `trackName` and re-announce (`src/lib/sfu-client.ts:848-875`, `apps/mobile/src/features/voice/protocol.ts:188-193`). Optional for a minimal client.

## 4. Track naming

- Ralph alias: `${prefix}-${kind}-${participantId}` (`src/lib/voice/partytracks-media-manager.ts:218-222`). The microphone uses
  prefix `cam`, giving **`cam-audio-<participantId>`** (`src/hooks/useVoiceChannel.ts:2716`, `apps/mobile/src/features/voice/session.ts:86-90,583`).
  Others: `cam-video-<pid>`, and `screen-video-<pid>` / `screen-audio-<pid>`. Mobile and web treat `screen-` audio as stream audio
  (`apps/mobile/src/features/voice/session.ts:540-542`).
- SFU name (`sfu_track_name`): whatever `trackName` you pushed. PartyTracks uses a random UUID per push.
- The VoiceRoom stores no other metadata. It keeps `track_name, participant_id, sfu_track_name, session_id, kind`
  (`realtime/voice-room.ts:2759-2775`).

## 5. Codec, SDP and ICE

- The only SDP munging is **`a=ptime:10`** on every remote description's Opus audio m-section. It is inserted after the Opus
  `a=fmtp` line or replaces an existing ptime (`src/lib/voice/opus-ptime.ts:22,53-95,104-125`), and is applied to every
  `setRemoteDescription` on the peer connection (`src/lib/voice/partytracks-transport.ts:143-155`). Effect: libwebrtc sends 10 ms
  Opus frames instead of 20 ms. The SFU's fmtp is `minptime=10;useinbandfec=1` (FEC on, no DTX) and is left alone
  (`src/lib/voice/opus-ptime.ts:6-14`). This is optional; leaving it out gives 20 ms frames.
- No stereo, DTX, or bitrate munging was found (searched `src/` for `usedtx|stereo=|maxaveragebitrate`). The web client sets no
  `sendEncodings` for audio (`node_modules/partytracks/dist/client/index.js:323-331`).
- Receive side: the web client sets `receiver.jitterBufferTarget = 0` and `playoutDelayHint = 0` on remote audio receivers
  (`src/lib/sfu-client.ts:293-307,819`). This is optional.
- The capture side relies on the platform's echo cancellation, noise suppression, and AGC (`apps/mobile/src/features/voice/engine.ts:16`).
  A native client should enable libwebrtc's APM.
- ICE: use the admission's `media.ice_servers` (TURN UDP/TCP plus STUN). One session, BUNDLE (`max-bundle`). TURN credentials last
  48 h (`realtime/meeting-room/turn-credentials.ts:23,95-101`). Op 40's Resumed sends a fresh list.

## 6. Minimal native client sequence

1. Compute `roomSlug = "voice-<serverId>-<channelId>"`.
2. `POST /api/voice/admission` `{channelId, serverId, roomSlug, sockets:["room","voice"], media:true}`. Keep
   `room_ticket`, `voice_ticket` and `media` (`participant_id`, `voice_token`, `ice_servers`).
3. Use both tickets within 60 s (section 1).
4. Open the room socket `wss://<api>/api/channels/<roomSlug>/ws?v=1` with subprotocols `["ralph.realtime.v1","ralph.ticket.<roomTicket>"]`
   and a permitted `Origin`. On Hello, start `{"op":3}` every `heartbeat_interval` ms and send Identify (op 0, no `standby`).
5. Open the voice socket `.../voice?v=1` with the voice ticket. On Hello, start heartbeats and send VoiceIdentify (op 100)
   `{participant_id, voice_token}` from `media`.
6. In parallel: `POST {prefix}/sessions/new`, giving `myCap`. Build the peer connection from `ice_servers`
   (`max-bundle`). Capture the mic, `addTransceiver(sendonly)`, create the offer, `POST sessions/<myCap>/tracks/new`
   (`location:"local"`, UUID `trackName`), then set the answer.
7. Room socket: Ready (op 2) gives the roster. Send VoiceStateUpdate (op 15) `{"self_mute":false,"self_deaf":false}`.
8. Voice socket: VoiceReady (op 101). Send Video (op 12)
   `{tracks:[{participant_id, track_name:"cam-audio-<pid>", sfu_track_name:<uuid>, session_id:myCap, kind:"audio"}]}`.
9. For each audio TrackInfo from VoiceReady or Video (op 12) with an `sfu_track_name`, not yours: once the peer connection
   is `connected`, `POST sessions/<myCap>/tracks/new` `{tracks:[{location:"remote", trackName:sfu_track_name, sessionId:session_id}]}`.
   Then set the SFU's offer, answer, and `PUT sessions/<myCap>/renegotiate`. Play the `track` whose `mid` matches. Retry
   `not_found_track_error` with backoff (100 ms to 10 s).
10. On StopTracks (op 13) or ClientDisconnect (op 11), stop the named or participant's pulls (optionally
    `PUT tracks/close`).
11. Optionally send Speaking (op 5) `{speaking:1|0}` from a local VAD.
12. 45 min after each voice token arrives, send room op 40. On Resumed (op 9), swap in the new `voice_token` for the PartyTracks
    `Authorization` header.
13. On peer-connection failure, repeat steps 6 and 8 with the new capability (the native client does, up to
    three times without a connect). On socket drop (not 4008/4010; a voice 1013 is a drop),
    get a new ticket for that socket alone (section 1),
    reconnect, and re-identify. The room socket may Resume (op 7, `{session_id: participant_id, seq_ack}`) (`src/lib/voice/gateways/room-gateway.ts:169-182`).
    The voice socket re-identifies and VoiceReady replays the tracks (`src/lib/voice/gateways/voice-gateway.ts:46-51`).
14. Leave (STATE_MACHINE.md "Teardown"): stop the mic; for a joined call wait the 100 ms send window;
    then room `{"op":11,"d":{}}`, voice `{"op":11,"d":{}}`, close the room socket, then the voice socket,
    then the peer connection.
15. On error 4010 or close 4010/4008: stop. Do not reconnect.

## 7. Web-only or optional (safe to skip for audio only)

- **Global chat gateway presence** (`/api/gateway`, VoiceChannelJoin op 33 / Leave op 34): since `realtime/VOICE_ROSTER.md`
  the room object puts you in the sidebar on its own; op 33/34 only set a hint channel and may trigger a roster pull.
  The native client still sends one op 33 when the room says Ready and op 34 on a joined leave, as the web does.
- Admission prefetch on hover (`src/lib/voice/voice-admission-client.ts`, ADMISSION.md "Prefetch").
- Speaking / VAD (op 5). It is cosmetic for others.
- VoiceAppEvent (op 106): demo chat. The soundboard, Listen Together, activities and `wordle.progress` are handled (`yap-protocol` `voice_app.rs`, `yap-core` `store/reduce/apps.rs`); op 5 carries SOUNDSHARE (bit 1) while our own clip plays, and any non-zero op 5 from others counts as speaking, as on the web.
- Native screen share (SelectProtocol op 1, SessionDescription 4, NegotiationDone 10, TracksReady 102).
- Screen publications, simulcast `tracks/update` (`node_modules/partytracks/dist/client/index.js:456-478`), spatial
  audio fields, ProfileRefresh (op 17), latency marks, `a=ptime:10`, jitter-buffer pinning.
