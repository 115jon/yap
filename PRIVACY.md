# Yap Privacy Policy

Effective date: November 13, 2026

Yap is operated by 115jon. Questions about this policy, privacy requests,
or data deletion requests can be sent to <115jon@proton.me>.

## Information Yap processes

Yap processes account identifiers, optional account profile information,
room/server/channel information, chat messages, attachments, relationship and
read-state data, notification preferences, and audit records needed to operate
the service. When a user joins a voice or video session, the service processes
the room/channel identifiers, connection credentials, and the media the user
chooses to send.

Yap keeps per-account usage counters: time spent in voice and screen sharing,
the number of messages sent, and when the account was last active. They are
used to award profile badges and are deleted with the account.

The desktop application may store account-session information and application
settings on the user's device. Browser and CEF runtime data, such as cached web
resources and local storage, may also be stored locally.

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
the desktop application's Registered games settings on that device.

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
- WebRTC/SFU services used by Yap for voice, video, and screen sharing.
- `steamcommunity.com` when a user links a Steam account (see Connections).
- `deadlock-api.com`, from Yap's server only, for Deadlock stats (see
  Deadlock stats).
- Firebase Cloud Messaging (Google) for push notifications to the mobile
  apps. A message notification carries the sender's name, account ID and
  avatar address, the channel and server names and IDs, the message ID and
  send time, and the start of the message (up to 180 characters).
- KLIPY and Tenor for GIF, sticker and clip search. Yap's server sends them
  the search terms; KLIPY also gets a pseudonymous identifier derived from
  the account (never the account ID or name). The pictures and videos
  themselves currently load in the apps from the providers' servers.

### Link previews

When a message contains a link, Yap's server fetches the page to build a
preview. Some sites are read through other services:

- X/Twitter posts through the public mirror services FxTwitter
  (`api.fxtwitter.com`), vxTwitter (`api.vxtwitter.com`, `vxtwitter.com`)
  and their `fixupx.com` and `fixvx.com` hosts;
- TikTok videos through TikTok's oEmbed, page and player API and through
  `tikwm.com`;
- Instagram posts through Instagram's oEmbed and media endpoints.

Some of these requests, and YouTube requests for Listen Together, go through
a relay Yap's operator runs on a server in the United States
(`egress.115jon.com`); YouTube requests leave that relay through a VPN. The
relay forwards the link, never who posted it. Pasting a link in the composer
can start the preview before the message is sent, except for links that look
like sign-in, password-reset or magic links or carry a token in the address.
A link written as `<https://...>` gets no preview.

Some previews load the provider's own content in the apps, so the provider
sees the viewer's IP address and its own cookies: YouTube's player (when the
viewer presses play) and video thumbnails, Spotify's player (when the preview
scrolls into view), and TikTok's player when no video file is available.

The web and desktop applications can also connect to locations selected by the
user or supplied in user content, such as a shared link, embed, attachment, or
room invitation.

## Automatic desktop connections

The current desktop client may check GitHub Releases for application updates,
restore a previously saved session with Yap/Kova services, and connect to
the chat gateway after a signed-in chat view opens. These behaviors are being
made explicitly configurable during installation and in desktop settings before
the next SignPath Foundation candidate release. Automatic update downloads and
installation must require a user action.

The desktop application does not include a dedicated third-party analytics SDK.
This statement does not limit infrastructure logging needed to secure, operate,
or diagnose the service.

## How Yap uses information

Yap uses information to authenticate users, provide chat and real-time
communication, deliver notifications, store user-selected content, prevent
abuse, secure the service, and investigate service failures. Yap does not
sell personal information.

## Retention and deletion

Account and profile data is kept while an account is active. Following a confirmed
account deletion, Yap deletes or anonymizes live account data within 30
days, except for limited records needed for security, fraud prevention, legal
compliance, or software-distribution obligations.

Messages, DMs, and server content remain until deleted by their sender, a
moderator, or a server/channel deletion action. When an account is deleted,
shared conversation content is retained for conversation integrity and the
deleted account is anonymized within 30 days. Public message-share links default
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
desktop and mobile apps. Yap asks them to type their username to confirm. A
user who owns a server must transfer it or delete it first.

When a user asks for deletion, Yap at once:

- removes them from every server;
- shows them to everyone else as "Deleted user", with no avatar, banner,
  bio, profile board, Activity, guestbook, wishlist or connections;
- signs them out on every device and forgets their notification tokens.

Their data stays for 30 days so they can change their mind. Signing in
during that time offers to restore the account, which puts back the profile
and the servers that still exist and have not banned them. Nothing else on
Yap works for the account until it is restored.

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

The kept content is moved to a new "Deleted user" record that is not linked
to the account's sign-in. The Kova sign-in account (`auth.115jon.com`) is
not Yap's and is not deleted; it can be deleted from Kova separately, and if
it is used to sign in to Yap again, Yap creates a new, empty account.
Records listed under "Retention and deletion" as kept for security, legal or
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
