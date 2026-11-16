# Play Console: Data safety and declarations

The answers for the Yap Android app (`dev.yap.app`, this folder) in Play
Console's App content forms, each checked against the code it describes.
The privacy policy they must agree with is [PRIVACY.md](../../../PRIVACY.md),
published at `https://yap.115jon.com/privacy` (the web's `/privacy` route,
`src/routes/privacy.tsx`, renders that file; the app's Settings, About,
Privacy opens it on the core's web origin).

Update this file, and the forms, when the app starts collecting something
new. Pending pull requests that change an answer are marked **(pending)**.

## Privacy policy URL

`https://yap.115jon.com/privacy`

## Data collection and security

| Question                                                              | Answer  | Where it comes from                                                                                                                                                                                                                                                                                           |
| --------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Does your app collect or share any of the required user data types?   | **Yes** | Below.                                                                                                                                                                                                                                                                                                        |
| Is all of the user data collected by your app encrypted in transit?   | **Yes** | The API and gateway are HTTPS and WSS only (`yap-core` `config.rs` `api_base`, rustls in `native/mobile/rust/src/http.rs`); the app has no cleartext opt-in (target SDK 36, no `usesCleartextTraffic` or network security config). Voice and video use WebRTC's DTLS-SRTP to Cloudflare Realtime. FCM is TLS. |
| Do you provide a way for users to request that their data is deleted? | **Yes** | In the app: Settings, Account, Delete account (`rust/qml/SettingsAccountPage.qml`, POST `/api/account/deletion`, `src/server/routes/account-deletion.ts`). On the web: the same in Settings, Account. By email: the address in PRIVACY.md.                                                                    |

### Account deletion

| Question                                                                                                                             | Answer                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Does your app allow users to create an account?                                                                                      | **Yes**, through Kova sign-in (the phone shows a code; the user approves it at `/activate` in a browser).                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Delete account URL                                                                                                                   | `https://yap.115jon.com/privacy#deleting-your-account` (names the app and the developer, gives the in-app and web steps and the email address, and says what is deleted and what is kept).                                                                                                                                                                                                                                                                                                                                                                          |
| Do you provide a way for users to request that some or all of their data is deleted, without requiring them to delete their account? | **Yes**: messages, attachments, profile fields and connections can each be deleted, and Activity events hidden, in the app or on the web; anything else by email.                                                                                                                                                                                                                                                                                                                                                                                                   |
| Data kept after deletion                                                                                                             | Messages the account sent in servers and DMs, their attachments, soundboard sounds, invites, bans and audit-log entries stay, shown as from "Deleted user" and no longer linked to the sign-in; bans and blocks against the account stay under its Kova id; security records follow their own retention. The Kova sign-in account (email, name, picture) is separate and is deleted on request by email (PRIVACY.md says so). The account is restorable for 30 days, then purged (`src/services/account-deletion.service.ts`; PRIVACY.md, "Deleting your account"). |

## Data types

"Collected" means it leaves the phone for Yap's servers (Cloudflare Workers,
D1, R2 and Realtime, which process it for Yap as service providers, so they
are not "sharing"). Every type below is collected for **App functionality**;
**Account management** is added where noted. None is used for advertising,
marketing, analytics or fraud prevention beyond the service's own abuse
limits, and none is sold. "Optional" means the user chooses whether to
provide it; "required" means the app cannot work without it.

### Personal info

| Data type                                                                                                 | Collected | Shared  | Ephemeral | Required | Purposes                              | Notes                                                                                                                                                                                                      |
| --------------------------------------------------------------------------------------------------------- | --------- | ------- | --------- | -------- | ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Name                                                                                                      | Yes       | No      | No        | Required | App functionality, Account management | The display name, taken from the Kova account (which may carry the user's real name from Google) until the user sets their own. `src/routes/api/auth/sync.ts`.                                             |
| Email address                                                                                             | Yes       | No      | No        | Required | Account management                    | Held by Kova (`auth.115jon.com`, the same developer) for sign-in. Yap's server reads it to derive a username when the Kova account has none and does not store it. The app itself never sends it.          |
| User IDs                                                                                                  | Yes       | **Yes** | No        | Required | App functionality, Account management | The Yap account id, the username, the Kova session. Shared: KLIPY gets a pseudonymous id derived from the account id (HMAC, `src/lib/klipy-customer.ts`) with GIF searches; declared as shared to be safe. |
| Address, phone number, race and ethnicity, political or religious beliefs, sexual orientation, other info | No        |         |           |          |                                       |                                                                                                                                                                                                            |

### Location

| Data type            | Collected | Shared | Ephemeral | Required | Purposes                              | Notes                                                                                                                                                                                                                                                             |
| -------------------- | --------- | ------ | --------- | -------- | ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Approximate location | Yes       | No     | No        | Required | App functionality, Account management | The city and country Cloudflare derives from the IP address, shown per session in Settings, Devices (`src/lib/session-activity.ts` `requestGeo`, `src/server/routes/sessions.ts`); Kova keeps them, and the IP address, with the session. No location permission. |
| Precise location     | No        |        |           |          |                                       | No location permission. Photos may carry GPS metadata as picked; the server removes it before storing JPEG, PNG, WebP, GIF, MP4 and MOV (see Photos) and never reads or keeps it.                                                                                 |

### Financial info, Health and fitness, Web browsing, Calendar

**No.** The app has no payments, health data or browsing history.

### Messages

| Data type             | Collected | Shared | Ephemeral | Required | Purposes          | Notes                                                                                                                                                                                                                                                                |
| --------------------- | --------- | ------ | --------- | -------- | ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Emails, SMS or MMS    | No        |        |           |          |                   | The app reads neither.                                                                                                                                                                                                                                               |
| Other in-app messages | Yes       | No     | No        | Optional | App functionality | Server and DM messages, replies, reactions, threads. Links in a message are fetched by Yap's server for a preview (some through FxTwitter, vxTwitter, tikwm and the providers' oEmbed); only the link goes, never who posted it, so this is not declared as sharing. |

### Photos and videos

| Data type | Collected | Shared | Ephemeral | Required | Purposes          | Notes                                                                                                                                                                                                                                                                                                                                                                         |
| --------- | --------- | ------ | --------- | -------- | ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Photos    | Yes       | No     | No        | Optional | App functionality | Attachments, avatar, banner, nameplate. The system photo picker (no storage permission) or the camera app (`NativeShim`, `CaptureProvider`). The server strips EXIF/XMP/IPTC from JPEG, PNG, WebP and GIF before storing (`src/lib/upload-metadata`); HEIC/AVIF attachments are stored as picked. An avatar or banner is converted and its location dropped on the phone too. |
| Videos    | Yes       | No     | No        | Optional | App functionality | Video attachments (location stripped from MP4/MOV). **(pending, PR #325)** the camera in calls, and **(pending, phone screen share PR)** the screen shared in a call (`MediaProjection`, after Android's consent each time): sent live through the SFU, not recorded.                                                                                                         |

### Audio files

| Data type                 | Collected | Shared | Ephemeral | Required | Purposes          | Notes                                                                                                                                                                                                                                                                                                                                                                             |
| ------------------------- | --------- | ------ | --------- | -------- | ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Voice or sound recordings | Yes       | No     | Yes       | Optional | App functionality | The microphone in voice channels and DM calls (`RECORD_AUDIO`, asked at the first join), forwarded live by Cloudflare Realtime and not recorded. **(pending, phone screen share PR)** with "Share sound from other apps", a screen share also sends what other apps play (`AudioPlaybackCapture`: media, game and unknown usages, Yap's own uid excluded), live and not recorded. |
| Music files               | No        |        |           |          |                   |                                                                                                                                                                                                                                                                                                                                                                                   |
| Other audio files         | Yes       | No     | No        | Optional | App functionality | Audio files the user attaches (also covered by Files and docs).                                                                                                                                                                                                                                                                                                                   |

### Files and docs

| Data type      | Collected | Shared | Ephemeral | Required | Purposes          | Notes                                                                                |
| -------------- | --------- | ------ | --------- | -------- | ----------------- | ------------------------------------------------------------------------------------ |
| Files and docs | Yes       | No     | No        | Optional | App functionality | Attachments picked with the system document picker. The file name is kept and shown. |

### Contacts

| Data type | Collected | Shared | Ephemeral | Required | Purposes          | Notes                                                                                                                                               |
| --------- | --------- | ------ | --------- | -------- | ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Contacts  | Yes       | No     | No        | Optional | App functionality | Not the phone's address book (no `READ_CONTACTS`): the in-app social graph Play counts here, friends, friend requests, blocks and DM conversations. |

### App activity

| Data type                    | Collected | Shared  | Ephemeral | Required | Purposes          | Notes                                                                                                                                                                                                                                                                                                                                             |
| ---------------------------- | --------- | ------- | --------- | -------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| App interactions             | Yes       | No      | No        | Required | App functionality | Read states, presence and status, voice time and message counters for badges, last active, and the Activity timeline (voice calls of 15 min or more, Listen Together, badges, servers joined; kept 7, 30 or 90 days, default 30, `src/lib/profile-board.ts`). The Android app detects no games.                                                   |
| In-app search history        | Yes       | **Yes** | No        | Optional | App functionality | GIF, sticker and clip searches go through Yap's server to KLIPY (with a pseudonymous id, `src/lib/klipy-customer.ts`) and Tenor, so this is declared as shared. Tenor results, and KLIPY results for a search without a pseudonymous id, are cached by search term, without the user, in Cloudflare KV. Message search goes to Yap's server only. |
| Installed apps               | No        |         |           |          |                   |                                                                                                                                                                                                                                                                                                                                                   |
| Other user-generated content | Yes       | No      | No        | Optional | App functionality | Bio, pronouns, custom status, profile board, server and channel names.                                                                                                                                                                                                                                                                            |
| Other actions                | No        |         |           |          |                   |                                                                                                                                                                                                                                                                                                                                                   |

### App info and performance

| Data type                  | Collected | Shared | Ephemeral | Required | Purposes | Notes                                                                                                                                                                                                                                                                                                              |
| -------------------------- | --------- | ------ | --------- | -------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Crash logs                 | No        |        |           |          |          | No crash SDK and no upload. Reports stay on the phone (`android/src/dev/yap/qtproto/crash`); the user may share one through Android's share sheet to a recipient they pick. That is the user's own transfer through another app, so it is not declared as collection. Revisit if an upload endpoint is ever added. |
| Diagnostics                | No        |        |           |          |          | No analytics or performance SDK; call quality is shown in the app only.                                                                                                                                                                                                                                            |
| Other app performance data | No        |        |           |          |          |                                                                                                                                                                                                                                                                                                                    |

### Device or other IDs

| Data type           | Collected | Shared | Ephemeral | Required | Purposes          | Notes                                                                                                                                                                                                                                                                                                                                                                            |
| ------------------- | --------- | ------ | --------- | -------- | ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Device or other IDs | Yes       | No     | No        | Required | App functionality | The FCM registration token, registered at every start (`rust/src/push.rs`, `/api/push-tokens`) and deleted when the phone's session is signed out or the account is deleted; with it the platform and app variant. The phone's model name (`Build.MODEL`) labels the session in the device list. Firebase Messaging (Google, a service provider) also keeps its installation id. |

## Push and call payloads (for the reviewer's questions)

- Message pushes are data-only FCM messages the app draws itself
  (`push/MessageNotifier.java`), `VISIBILITY_PRIVATE` with a public version
  that says only "New message". A push carries ids only (notification,
  message, channel, server and sender ids), the kind, the send time and a
  fixed label for older app versions ("Yap", "New mention"): no message text
  or names (`src/services/push.service.ts`). The phone fetches the text it
  shows from `GET /api/notifications/:id/push` with its own session.
- A DM ring (`call_ring`, `src/services/call.service.ts`) carries the call,
  channel and caller ids, the caller's name and avatar address and the ring
  time, so the phone can ring at once. It is shown with
  `VISIBILITY_PUBLIC` over the lock screen (`push/CallNotifier.java`).

## Declarations

### Full-screen intent (`USE_FULL_SCREEN_INTENT`)

- Declared in `android/AndroidManifest.xml` for incoming DM calls.
- Play Console, App content, Full-screen intent: core functionality
  **Calling (receiving voice calls)**. The ring is a `CallStyle.forIncomingCall`
  notification with category `CALL` and `setFullScreenIntent`
  (`push/CallNotifier.java`, `push/IncomingCalls.java`); it wakes the phone
  into the incoming-call screen over the lock screen while the call rings
  and nothing else uses it.
- On Android 14 and later the permission is granted only to apps whose
  declaration Play accepts, and the user can turn it off; without it the
  ring is a heads-up notification (handled in `IncomingCalls`).

### Foreground service types

| Type                            | Permission                            | Used                                                                                                                                                                                                                                                                                           | Declaration                                                                                                                                                                                                                                                                        |
| ------------------------------- | ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `microphone`                    | `FOREGROUND_SERVICE_MICROPHONE`       | Yes: `voice/VoiceService` while the user is in a voice channel or DM call, so the call keeps going with the screen off or the app in the background. Started from the user's join (`VoiceShim`), stopped when they leave; its ongoing notification has Mute and Leave.                         | Play Console, App content, Foreground service permissions: **Microphone**, task "an ongoing voice call the user started or joined", user-initiated, interrupting it drops the call. Play asks for a short video of joining a call, turning the screen off and the call continuing. |
| `camera`                        | `FOREGROUND_SERVICE_CAMERA`, `CAMERA` | **(pending, PR #325)** the same service carries the camera type while the user's camera is on in a call.                                                                                                                                                                                       | Add **Camera** to the same declaration when #325 ships: task "the user's camera in an ongoing video call", user-initiated, same video.                                                                                                                                             |
| `mediaProjection`               | `FOREGROUND_SERVICE_MEDIA_PROJECTION` | **(pending, phone screen share PR)** the same service carries the mediaProjection type while the user shares the phone's screen in a call, from Android's consent dialog (shown for every share) until the share stops (`voice/ScreenShim`); the call notification then also has Stop sharing. | Add **Media projection** to the same declaration when it ships: task "sharing the screen with the people in an ongoing call", user-initiated, interrupting it stops the share. Play asks for a short video of starting a share from the call, the consent dialog and stopping it.  |
| `phoneCall`, `dataSync`, others |                                       | No                                                                                                                                                                                                                                                                                             | Not declared.                                                                                                                                                                                                                                                                      |

### Notifications (`POST_NOTIFICATIONS`)

A runtime permission on Android 13 and later: the first channel the user
opens asks once, and Settings, Notifications asks again or opens Android's
notification settings (`push/PushShim.java`). No Play Console declaration.
Channels: `dms`, `mentions`, `other` (`push/MessageNotifier.java`),
`incoming_calls`, `missed_calls` (`push/CallNotifier.java`) and the call
in progress (`voice/VoiceService.java`).

### Other permissions in the merged manifest

`INTERNET`, `ACCESS_NETWORK_STATE`, `MODIFY_AUDIO_SETTINGS`, `WAKE_LOCK`,
`VIBRATE`, `FOREGROUND_SERVICE`, `WRITE_EXTERNAL_STORAGE` (API 28 only, for
Save image), and Firebase's `com.google.android.c2dm.permission.RECEIVE`
(checked with `aapt2 dump permissions` on a build of main). None needs a
Play declaration. The app asks for no location, contacts, phone, SMS,
storage (API 29+), accessibility or `QUERY_ALL_PACKAGES` permission.

### Ads

**No ads**, and no ad or analytics SDK (`android/build.gradle` has
`androidx.core` and `firebase-messaging` only).
