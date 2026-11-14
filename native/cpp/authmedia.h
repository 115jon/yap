#pragma once

// Private video attachments in Qt Multimedia. `/api/attachments/*` answers
// 404 without the session and Media Foundation's HTTP source cannot send a
// header, so the player loads a loopback URL instead:
//
//   MediaPlayer { source: AuthMedia.needsAuth(url) ? AuthMedia.localUrl(url) : url }
//
// The Rust core (src/loopback.rs) relays each Range request to the API with
// the session, for the API's own private media only and never following a
// redirect with it. QMediaPlayer::setSourceDevice was tried first: Qt's
// MFStream serves one read at a time and drops seeks while one is pending,
// so a network-backed device stalled the MP4 source on its first frame.

#include <QtCore/QObject>
#include <QtCore/QString>

class AuthMedia : public QObject
{
  Q_OBJECT

public:
  using QObject::QObject;

  // Whether `url` is the API's private media (it needs the relay).
  Q_INVOKABLE bool needsAuth(const QString& url) const;
  // The loopback URL that plays `url` with the session; `url` itself when
  // it needs none.
  // Each localUrl() that returned a loopback URL holds its token for the
  // caller; release(url) gives that hold back when the player is done, and
  // the token stops working with its last holder.
  Q_INVOKABLE QString localUrl(const QString& url) const;
  Q_INVOKABLE void release(const QString& url) const;
};
