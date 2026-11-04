# Yap voice wire protocol: minimal audio-only native client

Read from code on `spike/qt-desktop` (2026-11-04). Every claim cites `file:line`.
Paths are relative to the repo root. "UNVERIFIED" means the repo does not show it
(usually Cloudflare Realtime API behaviour behind the proxy).

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

## 1. Socket tickets and media bootstrap

`POST /api/voice/socket-ticket`. Auth headers are already established (Bearer Kova
token + `X-Publishable-Key`; `src/lib/voice/socket-ticket-client.ts:51-55`).
Send `Content-Type: application/json`.

### 1.1 Body (server voice channel)

The body is the same for both audiences. Only `audience` changes
(`apps/mobile/src/features/voice/tickets.ts:53-60`, `src/lib/voice/socket-ticket-client.ts:146-154`):

```json
{ "audience": "room",  "channelId": "<channelId>", "serverId": "<serverId>", "roomSlug": "voice-<serverId>-<channelId>" }
{ "audience": "voice", "channelId": "<channelId>", "serverId": "<serverId>", "roomSlug": "voice-<serverId>-<channelId>" }
```

How the server reads the fields (`src/server/routes/socket-ticket.ts`):

- `audience`: must be `"global" | "room" | "voice"`, else 400 `INVALID_AUDIENCE` (`:89-97`, `:506-510`).
- `channelId`: **required** for an authenticated room or voice ticket. Without it the
  request takes the public-demo path (`:99-113`) and fails off-origin (`:396-403`).
  The channel must be visible to the user (else 403, `:280-288`), of type `voice` (`:324-331`),
  and the user needs CONNECT (else 403 `CONNECT_DENIED`, `:346-378`).
- `serverId`: optional. If sent, it must match the channel's server (else 403 `SERVER_MISMATCH`, `:333-344`).
- `roomSlug`: **ignored for server channels**. The server derives it with
  `buildVoiceChannelRoomSlug(serverId, channelId)` (`:380-385`). It only matters for `dm-call-*` slugs (`:290-310`).
- `moveGrant`: optional, for moderator moves only (`:349-370`). Skip it.

### 1.2 Response

```json
{
  "access_mode": "authenticated",
  "audience": "room",
  "expires_at": 1730000000000,
  "room_slug": "voice-<s>-<c>",
  "ticket": "rt1.<payload>.<sig>",
  "media_bootstrap": {
    "participant_id": "<uuid>",
    "voice_token": "<token>",
    "ice_servers": [
      {
        "urls": ["turn:...:3478?transport=udp", "turn:...:443?transport=tcp"],
        "username": "...",
        "credential": "..."
      }
    ]
  }
}
```

(`src/server/routes/socket-ticket.ts:157-164`, `:241-245`)

- `media_bootstrap` is present **only for `audience: "room"`** (`:142-155`). The voice ticket has none.
- `participant_id` is the room ticket's nonce, `crypto.randomUUID()` (`:118`, `:242`). The MeetingRoom
  uses that nonce as your participant id (`realtime/realtime-admission.ts:54`, `realtime/meeting-room.ts:2876`).
- `voice_token` format: `${participantId}:${roomSlug}:${issuedAtMs}:${subject}.${base64(HMAC-SHA256)}`
  (`realtime/voice-token.ts:34-50`). It is valid for **60 min** (`realtime/voice-token.ts:1`, `:76-79`).
  Treat it as opaque.
- `ice_servers`: `[{urls: string[], username?, credential?}]` (`realtime/meeting-room/turn-credentials.ts:1-5`).
  This is Cloudflare TURN, filtered to `:3478?transport=udp`, `:443?transport=tcp` and `stun:` URLs, at most 2 entries
  (`:64-83`). The fallback is `[{urls:["stun:stun.cloudflare.com:3478"]}]` (`:19-21`). Clients also fall back to that STUN
  server if the list is empty (`src/lib/voice/partytracks-transport.ts:19-21,122-126`).
- A ticket is **single use, with a 60 s TTL** (`src/server/routes/socket-ticket.ts:39`). Its nonce is spent
  on the WebSocket upgrade (`realtime/voice-room.ts:483-492`). Every reconnect needs a fresh ticket.

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
3. Receive **MediaReady** (op 41) `{participant_id, ice_servers, voice_token}` (`realtime/meeting-room.ts:2911-2918`).
4. Receive **Ready** (op 2) `{participant_id, ice_servers, participants: VoiceState[], heartbeat_interval, voice_token, spatial_audio_state}`
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
   (`realtime/meeting-room.ts:3397-3417`). The web client refreshes every 45 min (`src/lib/sfu-client.ts:44,912-925`).
   Use the new token on the next PartyTracks request and VoiceIdentify (`apps/mobile/src/features/voice/transport.ts:103-105`).
8. Leave: send `{"op":11,"d":{}}`, then close (`apps/mobile/src/features/voice/gateways.ts:142-150`,
   `realtime/meeting-room.ts:1101-1103`). This revokes your grant, and the VoiceRoom drops you (`realtime/voice-room.ts:533-542`).

### 2.4 Voice socket (`/voice`, VoiceRoom)

1. Receive Hello.
2. Send VoiceIdentify (op 100) with the room ticket's bootstrap credentials (`src/lib/voice/gateways/voice-gateway.ts:314-346`;
   schema `packages/protocol/src/client-payloads.ts:228-233`):
   ```json
   {
     "op": 100,
     "d": {
       "participant_id": "<media_bootstrap.participant_id>",
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
`RTCPeerConnection({iceServers: media_bootstrap.ice_servers, bundlePolicy: "max-bundle"})` (`:611-614`). If you pass ICE servers,
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
- ICE: use `media_bootstrap.ice_servers` (TURN UDP/TCP plus STUN). One session, BUNDLE (`max-bundle`). TURN credentials last
  48 h (`realtime/meeting-room/turn-credentials.ts:23,95-101`). The ticket and Ready send the same list.

## 6. Minimal native client sequence

1. Compute `roomSlug = "voice-<serverId>-<channelId>"`.
2. `POST /api/voice/socket-ticket` `{audience:"room", channelId, serverId, roomSlug}`. Keep `ticket` and `media_bootstrap`
   (`participant_id`, `voice_token`, `ice_servers`).
3. `POST /api/voice/socket-ticket` `{audience:"voice", ...same}`. Keep `ticket`. Do this in parallel with step 2. Use the ticket within 60 s.
4. Open the room socket `wss://<api>/api/channels/<roomSlug>/ws?v=1` with subprotocols `["ralph.realtime.v1","ralph.ticket.<roomTicket>"]`
   and a permitted `Origin`. On Hello, start `{"op":3}` every `heartbeat_interval` ms and send Identify (op 0, no `standby`).
5. Open the voice socket `.../voice?v=1` with the voice ticket. On Hello, start heartbeats and send VoiceIdentify (op 100)
   `{participant_id, voice_token}` from the bootstrap.
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
12. Every 45 min, send room op 40. On Resumed (op 9), swap in the new `voice_token` for the PartyTracks `Authorization` header.
13. On peer-connection failure, repeat steps 6 and 8 with the new capability. On socket drop (not 4008/4010), get a new ticket for that socket,
    reconnect, and re-identify. The room socket may Resume (op 7, `{session_id: participant_id, seq_ack}`) (`src/lib/voice/gateways/room-gateway.ts:169-182`).
    The voice socket re-identifies and VoiceReady replays the tracks (`src/lib/voice/gateways/voice-gateway.ts:46-51`).
14. Leave: voice `{"op":11,"d":{}}`, room `{"op":11,"d":{}}`, close both sockets, close the peer connection.
15. On error 4010 or close 4010/4008: stop. Do not reconnect.

## 7. Web-only or optional (safe to skip for audio only)

- **Global chat gateway presence** (`/api/gateway`, VoiceChannelJoin op 33 / Leave op 34): this is what puts you in other members'
  sidebar channel tree (`src/hooks/useVoiceChannel.ts:1978-1983`, `src/stores/chat-gateway.ts:1165-1184`,
  `apps/mobile/src/features/voice/session.ts:1029-1047`). It is not needed for media, but without it the sidebar will not list you.
- Ticket prefetch on hover (`src/lib/voice/socket-ticket-client.ts:97-155`). Move grants (`moveGrant`, VoiceMove op 42).
- Standby / pre-warm (`standby: true`, `promote_standby`) (`src/lib/sfu-client.ts:933-950,1111-1121`).
- Speaking / VAD (op 5). It is cosmetic for others.
- TrackUnavailable (op 109) / RepublishRequest (op 110) self-healing. Recommended later.
- VoiceAppEvent (op 106): soundboard, listen together, stream-watch snapshots, demo chat (`realtime/voice-room.ts:3057`, `:1882-1904`).
- Native screen share (SelectProtocol op 1, SessionDescription 4, NegotiationDone 10, TracksReady 102).
- Camera and screen publications, simulcast `tracks/update` (`node_modules/partytracks/dist/client/index.js:456-478`), spatial
  audio fields, ProfileRefresh (op 17), stats/latency marks, audio sentinels, `a=ptime:10`, jitter-buffer pinning.
