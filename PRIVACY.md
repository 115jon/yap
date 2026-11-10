# Yap Privacy Policy

Effective date: November 9, 2026

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
user, keeps it only while the user's connection is open, does not store it with
the account, and never shows it while the user's status is invisible. "Share my
activity" is on by default and can be turned off at any time in Privacy &
safety settings; while it is off, Yap shows no game activity for the user.
Game icons are served from Yap's own storage, so viewing someone's activity
does not contact a third party.

Automatic game detection is planned for the Yap desktop application and is not
part of the current release. When the desktop application supports it, it will
compare the names of running programs on the user's device against Yap's
published game catalog; the list of running programs will stay on the device,
and only a matched game's name, catalog icon and start time will be sent.
Hiding a single game will be a setting of the desktop application on that
device. This section will be updated when detection ships.

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

## Security

Yap uses HTTPS and transport encryption for supported network
connections. Voice/video transport uses WebRTC security mechanisms, including
DTLS-SRTP. Yap does not claim end-to-end encryption for service features
unless that design is independently documented and verified.

## Changes

This policy may change as the service changes. Material changes will be published
in this document with a revised effective date.
