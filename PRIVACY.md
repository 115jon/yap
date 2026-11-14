# Yap Privacy Policy

Effective date: November 14, 2026

Yap is operated by 115jon. Questions about this policy, privacy requests,
or data deletion requests can be sent to <115jon@proton.me>.

## Information Yap processes

Yap processes account identifiers, optional account profile information,
room/server/channel information, chat messages, attachments, relationship and
read-state data, notification preferences, and audit records needed to operate
the service. When a user joins a voice or video session, the service processes
the room/channel identifiers, connection credentials, and the media the user
chooses to send.

Yap takes the username, display name, avatar and bio from the Kova sign-in
account (`auth.115jon.com`), which may carry the name and picture of the
account used to sign in to Kova (such as Google), when the Yap account is
created, and again whenever the Kova account changes. The username comes
from the Kova account's username or, without one, the part of its email
address before the `@`, and always follows the Kova account. The display
name and bio are taken only while the user has not set their own, and the
avatar only while the user has not uploaded one to Yap. Yap does not store
the email address.

Yap keeps per-account usage counters: time spent in voice and screen sharing,
the number of messages sent, and when the account was last active. They are
used to award profile badges and are deleted with the account.

Each signed-in device appears in the user's session list (Settings,
Devices) with what it runs on, read from its User-Agent (browser or Yap app,
and the operating system), its device name from a device sign-in (on
Android the phone's model name, on Windows the computer's name), and the
city and country Cloudflare derives from its IP address. Kova keeps these
with the session, along with the IP address. The user can sign a device out
from any other.

The desktop and Android applications store the session on the device (on
Windows in the Windows credential store; on Android in a file sealed with a
key from the Android Keystore, kept out of cloud backup and device transfer),
along with settings and a cache of recent conversations that is wiped on
sign-out. The web application keeps its session and settings in the
browser's storage. The desktop application's web players (see Link previews)
keep their own WebView2 data in the application's data folder.

## Microphone, camera and screen sharing

Yap uses the microphone only while the user is in a voice channel or a call
(or testing it in settings), the camera only while the user has turned it on,
and screen capture only while the user is sharing a screen or window.
Browsers and Android ask for each permission the first time it is needed; on
Windows, the system's privacy settings control the microphone and camera.
The Android app shows an ongoing notification, with Mute and Leave, for as
long as a call keeps the microphone open.

Voice, video and screen shares travel to Yap's selective forwarding unit
(Cloudflare Realtime) and from there to the other people in the call. They
are not recorded (see Retention and deletion). Noise suppression and camera
background effects run on the user's device.

A shared window shows that window; a shared screen shows everything on it,
notifications included. In the desktop application, sharing a whole screen with audio
sends everything the computer plays, other people's voices in the call
included; sharing a window sends only that application's sound.

The Android app shares the phone's screen only after Android's own consent
dialog, every time, where the user picks the whole screen or one app; while
it is shared, the call's notification and Android's own indicator say so,
and either can stop it. With "Share sound from other apps" turned on it also
sends what other apps play (videos, music, games), never Yap's own sounds
or the call, so the other people in the call do not hear themselves back.

The Android app's composer can take a photo with the phone's camera app; the
photo leaves the phone only if the user sends it.

## Game activity

Yap can show others the game a user is playing ("Playing X"): the game's
name, an icon from Yap's game catalog, and when it started. Yap shares it only
with people who share a server, a direct message or a friendship with the
user, keeps the live activity only while the user's connection is open, and
never shows it while the user's status is invisible. "Share my activity" is on
by default and can be turned off at any time in Privacy & safety settings;
while it is off, Yap shows no game activity for the user. Game icons are
served from Yap's own storage, so viewing someone's activity does not contact
a third party.

When a game session of five minutes or more ends, Yap adds it to the user's
Activity timeline and to per-day play time (the game, the day and the time
played), which the profile's Top games widget reads. Nothing is added while
"Share my activity" is off or the user is invisible (see Activity timeline).

The Yap desktop application detects games itself. On the user's device it
reads the list of running programs, Steam's currently running game and the
foreground window, and compares them against Yap's game catalog, which ships
with the application and is refreshed from Yap's server. The list of running
programs stays on the device; only a matched game's name, catalog icon and
start time are sent. Detection can be turned off, and single games hidden, in
the desktop application's Registered games settings on that device. The
Android app does not detect games.

## Activity timeline

A user's profile has an Activity timeline that Yap's server writes:

- games played for five minutes or more;
- voice calls of 15 minutes or more and screen shares of five minutes or
  more, with the channel and server they were in;
- Listen Together sessions of three tracks or more, with the most played
  track;
- badges earned and servers joined.

Games, calls, screen shares and Listen Together sessions are not recorded
on the timeline while "Share my activity" is off or the user is invisible (as
of when the session ends). The usage counters above (voice and screen-share
time per day) still count them.

The user sees their whole timeline. Other people see it only when they share
a server or a friendship with the user, or a direct message the user has
written in, and never when
either side has blocked the other. They do not see events or kinds of events
the user has hidden, or calls, screen shares and Listen Together sessions in
direct messages and group direct messages; and they see the channel and server
of a call only when they are in that server. Events are kept for the period
the user chooses (7, 30 or 90 days; 30 by default) and then deleted.

Profile board widgets the user adds, such as Top games and Voice hours, show
summaries of these records and counters to anyone who can open the profile;
they leave out calls and Listen Together sessions in direct messages.

## Connections

A user can link an account they own on another service to their Yap
profile. Steam is the first. Linking sends the user to Steam's own sign-in
page ("Sign in through Steam"); Yap never sees the Steam password. Steam tells
Yap the account's SteamID, and Yap confirms the answer with Steam. Yap then
reads the account's public persona name and avatar from its public Steam
profile.

Yap stores the SteamID (and the account number derived from it, for later
game-stat features), the persona name, the avatar address, the profile
address, whether the connection shows on the profile, and when it was
verified. One Steam account can be linked to only one Yap account.

While "Show on profile" is on, anyone who can open the user's profile sees
the persona name and a link to the Steam profile, which contains the
SteamID. People the user has blocked see no connections. The profile also
carries the avatar address; the Yap apps show the avatar only to the user, in
settings, through Yap's own media proxy.
Unlinking deletes the stored connection at once.

## Deadlock stats

A user who links Steam and puts the Deadlock widget on their profile board
has their Deadlock stats shown there. Yap's server, never a user's device,
sends that Steam account number to deadlock-api.com, a community API, and
reads the account's rank, per-hero totals, match history and the newest
match's details. Deadlock match data is public there whatever the Steam
privacy settings are.

Yap keeps only a small summary (rank, the last ten matches, totals for 7
days, 30 days and the season, the newest match's items and damage) and when
it was fetched. It does not keep the full match history, other players'
account numbers or the Steam friends list (which Yap never requests). Hero,
rank and item pictures are copied to Yap's own storage, so the apps never
load them from a third party. The summary is deleted when Steam is unlinked
or the widget leaves the board. An account that asked deadlock-api.com to
delete its data (deadlock-api.com/data-privacy) shows nothing.

When the desktop app reports what the user is doing in Deadlock (the phase,
hero and mode, read from the game's own log), the account number in that
log is checked against the linked one on Yap's server and is not stored or
shown. Party members are shown only when they are Yap users who linked their
own Steam account; other party members' account numbers are dropped. This
follows "Share my activity", like other game activity.

## Where information is sent

Yap uses these services:

- `yap.115jon.com` for the Yap
  application API, chat gateway, and service functions.
- `auth.115jon.com` for the project-maintained Kova authentication service.
- Cloudflare services for application delivery and infrastructure.
- GitHub Releases for published desktop releases and desktop update metadata.
- Cloudflare Realtime, Yap's selective forwarding unit, for voice, video and
  screen sharing.
- `steamcommunity.com` when a user links a Steam account (see Connections).
- `deadlock-api.com`, from Yap's server only, for Deadlock stats (see
  Deadlock stats).
- Firebase Cloud Messaging (Google) for push notifications to the mobile
  apps. A message notification carries IDs only (the notification, message,
  channel, server and sender account IDs), its kind (mention, reply or
  direct message) and the send time: no message text, attachment, or
  sender, channel or server name. The Android app fetches the text it shows
  from Yap's server with its own session. A DM call's ring also carries the
  caller's name and avatar address, so the phone can ring at once.
- KLIPY and Tenor for GIF, sticker and clip search. Yap's server sends them
  the search terms; KLIPY also gets a pseudonymous identifier derived from
  the account (never the account ID or name), the user's language, and which
  result the user sent. The pictures and videos load
  through Yap's media proxy (see Media proxy), not from the providers. KLIPY
  also makes custom emoji: Yap's server sends it the text prompt the user
  types and stores the picture it returns.
- `myinstants.com`, from Yap's server only, for soundboard sound search.
- `radio-browser.info` for radio station search in Listen Together. The web
  and desktop applications ask it directly, so it sees the user's IP address
  and the search; no account information is sent. A radio station that is
  playing streams to each listener's app from the station's own address, so
  the station sees every listener's IP address.
- jsDelivr (`cdn.jsdelivr.net`), from which the apps load emoji pictures.
- Discord's CDN (`cdn.discordapp.com`, `media.discordapp.net`) and
  `cdn.yapper.shop`, from which the apps load the avatar decorations, profile
  effects and nameplates of Yap's shop, so those hosts see the viewer's IP
  address. Yap's server reads the shop's catalog from `api.yapper.dev`.

### Link previews

When a message contains a link, Yap's server fetches the page to build a
preview. Some sites are read through other services:

- X/Twitter posts through the public mirror services FxTwitter
  (`api.fxtwitter.com`), vxTwitter (`api.vxtwitter.com`, `vxtwitter.com`)
  and their `fixupx.com` and `fixvx.com` hosts;
- TikTok videos through TikTok's oEmbed, page and player API and through
  `tikwm.com`;
- Instagram posts through Instagram's oEmbed and media endpoints;
- YouTube videos through YouTube's oEmbed and player API, and Spotify links
  through Spotify's oEmbed.

Some of these requests, and YouTube requests for Listen Together, go through
a relay Yap's operator runs on a server in the United States
(`egress.115jon.com`); YouTube requests leave that relay through a VPN. The
relay forwards the link, never who posted it. Pasting a link in the composer,
or finishing typing one, can start the preview before the message is sent, except for links that look
like sign-in, password-reset or magic links or carry a token in the address.
A link written as `<https://...>` gets no preview.

Some previews can play in the provider's own player: YouTube's, Spotify's,
TikTok's when no video file is available, and other sites' video players.
These load only when the viewer presses play, and from then on the provider
sees the viewer's IP address and its own cookies.

Yap also answers oEmbed requests for its own public message-share links, so
other sites can preview a shared message.

### Media proxy

Pictures, videos and audio from other sites in link previews, GIFs, avatars
from sign-in providers and Steam, and Listen Together artwork reach the apps
through Yap's media proxy (`/api/proxy-media`), so those sites see Yap's
server, not the viewer. The proxy sends the viewer's cookies and identifying
headers to no one; it passes on only what a download needs, such as the byte
range. For sites other than a fixed list of GIF, social-media and avatar
hosts, it also refuses private network addresses, accepts only picture,
video and audio files up to a size limit, and serves only addresses Yap's
server signed or requests from a signed-in user. Files may stay in Cloudflare's cache for up
to seven days. Videos streamed as playlists of many segments (HLS) still
load from the provider.

The apps also open locations the user chooses or that appear in user
content, such as a link the user opens or a room invitation.

## Notifications and calls

Push notifications reach the Android app through Firebase Cloud Messaging;
what a message notification carries is listed under Where information is
sent. The app asks for the notification permission on Android 13 and later.
Signing the phone out, from the phone or from another device, or deleting
the account removes the phone's notification token.

When someone rings a user in a direct message, the ring goes to the user's
signed-in apps and, through Firebase Cloud Messaging, to their phones. The
ring carries the call and channel IDs, the caller's account ID, name and
avatar address, and how long it rings, so the phone can show the call at
once. The Android app shows an incoming call over the lock screen with the
caller's name and avatar (on Android 14 and later only if the user allows
Yap's full-screen notifications). A ring that is not answered becomes a
missed call.

Message notifications are marked private: on a phone set to hide sensitive
notification content, the lock screen shows only "New message" or "New
direct message".

## Crash reports

The desktop application writes a crash dump (the threads, their stacks and
the loaded modules, without the memory they point to, though a stack can
still hold fragments of what the application was handling) to its own data folder
on the computer and keeps the newest ten. Nothing is uploaded. The next
launch offers to open the folder, and the user decides whether to send a
dump to anyone.

The Android app does not upload crash reports and contains no crash
reporting or analytics SDK. Where it saves a report about a crash, the
report stays on the phone, and the user can read it, share it through
Android's share sheet with a recipient they choose, or delete it.

## Automatic desktop connections

After it starts, the desktop application restores the saved session with
Yap and Kova and connects to the chat gateway. It checks GitHub Releases for
updates about 20 seconds after it starts and every six hours after that, and
downloads a newer version in the background. The update is installed when
the user presses Restart now or, while "Install on quit" is on (the default,
in Settings, Desktop app), when the application quits or next starts. Development builds and
portable copies do not update themselves.

The desktop application does not include a dedicated third-party analytics SDK.
This statement does not limit infrastructure logging needed to secure, operate,
or diagnose the service.

## Uploaded files

Before an uploaded file is kept (a large file uploaded in parts, right after
its parts are joined), Yap's server removes location, camera and other
metadata from it: EXIF (with its GPS, camera and time tags), XMP,
IPTC and comments in JPEG, PNG, WebP and GIF pictures, and location and
similar metadata in MP4 and MOV videos. It does this for attachments,
avatars, banners, nameplates, server icons, profile board pictures, camera
backgrounds and voice status media. Yap cannot clean some formats
(HEIC/HEIF, AVIF, TIFF, JPEG XL), videos with a location or subtitle track,
or a file it cannot finish reading (a damaged or very large one). As an avatar, banner or other public picture such a file is
refused; as an attachment it is stored as uploaded, and the app is told its
metadata could not be checked. Other files, such as documents and audio, are
stored as uploaded.

The Android app also turns a picture chosen for an avatar or banner upright,
converts HEIC and AVIF to JPEG and drops its location before uploading it.

An attachment keeps its file name, which others in the conversation see.

## How Yap uses information

Yap uses information to authenticate users, provide chat and real-time
communication, deliver notifications, store user-selected content, prevent
abuse, secure the service, and investigate service failures. Yap does not
sell personal information.

## Blocking

A user can block another user. The blocked user is not notified, but sees
that they can no longer message or call the user or open their profile.
Across a block, in either direction, neither user can start a direct message with the
other or, in their direct message, send messages, reactions or typing, ring,
or join its call, and a block ends a ring or a direct-message call already
under way. A blocked user's friend requests are refused, and their mentions,
replies and direct messages do not notify the user who blocked them. In
servers, the blocker's apps collapse the blocked user's messages. The
blocked user cannot see the blocker's Activity timeline or connections.
Blocks last until the user unblocks (and survive account deletion, see
Deleting your account).

## Retention and deletion

Account and profile data is kept while an account is active. Following a confirmed
account deletion, Yap deletes or anonymizes live account data about 30
days after the request (see Deleting your account), except for limited records needed for security, fraud prevention, legal
compliance, or software-distribution obligations.

Messages, DMs, and server content remain until deleted by their sender, a
moderator, or a server/channel deletion action. When an account is deleted,
shared conversation content is retained for conversation integrity and the
deleted account is anonymized about 30 days after the request. Public message-share links default
to 30 days and are removed earlier when revoked or when their source message is
deleted.

Attachments and other stored files remain while their related content needs them.
Unattached uploads are removed within 24 hours. Files tied to deleted content are
removed from live object storage within 7 days. Server audit logs are retained
for 180 days and security/abuse-prevention records for 90 days, unless an active
investigation, legal hold, or security need requires longer retention.

Yap does not intentionally record or store live voice, video, or
screen-share media during normal operation. Realtime credentials and recovery
metadata are retained only for the short periods needed to run or recover a
session. Operational logs are retained for up to 7 days and must not intentionally
contain message bodies, attachment contents, authentication tokens, or full
SDP/ICE payloads. Disaster-recovery backups, if enabled, are retained for up to
30 days; live deletion happens first and backup copies age out on that cycle.

Before deletion, users may request an export of their account data. Users may
request access, correction, deletion, or export by contacting <115jon@proton.me>.
GPL source-offer request and fulfillment records are retained for at least three
years from the relevant binary distribution date.

## Deleting your account

A user can delete their Yap account in Settings, under Account, in the web,
desktop and Android apps. Without an app installed, the web app at
`yap.115jon.com` works from any browser, or the user can email
<115jon@proton.me>. Yap asks them to type their username to confirm. A
user who owns a server must delete it first.

When a user asks for deletion, Yap at once:

- removes them from every server;
- shows them to everyone else as "Deleted user", with no avatar, banner,
  bio, profile board, Activity, guestbook, wishlist or connections;
- signs them out on every device and forgets their notification tokens.

Their data stays for 30 days so they can change their mind. Signing in
during that time offers to restore the account, which puts back the profile
and the servers that still exist and have not banned them. Nothing else on
Yap works for the account until it is restored.

An account that is suspended after asking for deletion, or that owns a
server again, is held rather than deleted: its data stays until a site
admin resolves the suspension or the server is deleted or handed over.

After 30 days Yap deletes the account's profile, avatar, banner, nameplate,
profile board and widgets, Activity, guestbook entries, wishlist, settings,
friends and blocks, reactions, read states, notifications, notification
tokens, badges, plan, usage counters, connections and favourites, and the
files it uploaded that are not part of a kept message: avatars, profile and
board images, generated emojis, camera backgrounds and unfinished uploads.
Messages it sent in servers and direct messages stay, with their attachments,
and are shown as from "Deleted user", so the other people in those
conversations keep their history. The same goes for server soundboard sounds,
invites, bans and server audit-log entries it made. A user who wants
particular messages gone should delete them before deleting the account.
Mentions of the account in other people's messages are text and are not
rewritten.

Bans and blocks against the account also stay: a server's ban of it, another
user's block of it and a guestbook block are kept under the account's Kova
sign-in id, so they still apply if that Kova account signs in to Yap again.
A server's ban list shows such a ban as "Deleted user".

The kept content is moved to a new "Deleted user" record that is not linked
to the account's sign-in. The Kova sign-in account (`auth.115jon.com`) is
not Yap's and is not deleted; it can be deleted from Kova separately, or by
emailing <115jon@proton.me>, and if
it is used to sign in to Yap again, Yap creates a new, empty account, to which
the bans and blocks above still apply. Entries in Yap's site-admin audit log
about the account (such as a suspension) keep its Kova id as a security
record. Records listed under "Retention and deletion" as kept for security, legal or
software-distribution reasons, and backups, follow their own retention
periods.

## Security

Yap uses HTTPS and transport encryption for supported network
connections. Voice/video transport uses WebRTC security mechanisms, including
DTLS-SRTP. Yap does not claim end-to-end encryption for service features
unless that design is independently documented and verified.

## Changes

This policy may change as the service changes. Material changes will be published
in this document with a revised effective date.
