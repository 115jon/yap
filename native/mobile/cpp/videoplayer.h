#pragma once

// Video and sound previews on the phone (QML singleton `Player`, Yap.Mobile;
// the Qt build has no Multimedia). Android's own player does the playing,
// in Java (android/src/dev/yap/qtproto/media):
//
//   play()       a video full screen in PlayerActivity (Media3), from where
//                it was; a GIF loops. A tap on a video in the chat plays it
//                in the player sheet instead (InAppPlayer, inappplayer.h,
//                which signs its URL through resolve()).
//   playAudio()  one sound preview at a time (InlineAudio, a TikTok post's
//                sound), stopped when the app leaves the front.
//   posters      `image://videoposter/<url>`: a video's first frame for the
//                chat, from Android's frame grabber, kept in memory only.
//
// Every URL goes to Java as a player loads it: the API's private media
// signed with a media cap (rust/src/mediacap.rs), never with the session.

#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtQuick/QQuickAsyncImageProvider>

#include <functional>

class VideoPlayer : public QObject
{
  Q_OBJECT
  // Whether this build plays at all (Android).
  Q_PROPERTY(bool available READ available CONSTANT)
  // The sound preview asked for last (empty: none) and what it is doing.
  Q_PROPERTY(QString audioSource READ audioSource NOTIFY audioChanged)
  Q_PROPERTY(AudioState audioState READ audioState NOTIFY audioChanged)

public:
  enum AudioState { Idle, Loading, Playing, Failed };
  Q_ENUM(AudioState)

  explicit VideoPlayer(QObject* parent = nullptr);
  ~VideoPlayer() override;
  static VideoPlayer* instance();

  bool available() const;
  QString audioSource() const { return m_audioSource; }
  AudioState audioState() const { return m_audioState; }

  // Full screen from `positionMs`; `loop` for a GIF. `title` names it for
  // TalkBack. failed(url) when it cannot be had (signed out, offline).
  Q_INVOKABLE void play(const QString& url, qint64 positionMs = 0, bool loop = false,
                        const QString& title = QString());
  Q_INVOKABLE void playAudio(const QString& url);
  Q_INVOKABLE void stopAudio();

  // `url` as a player can load it, given to `done` on this object's thread
  // (empty: it cannot be).
  void resolve(const QString& url, std::function<void(const QString&)> done);

  // From Java (MediaShim.nativeAudioState).
  void onAudioState(int state);

signals:
  void audioChanged();
  void failed(const QString& url);

private:
  void setAudio(const QString& source, AudioState state);

  QString m_audioSource;
  AudioState m_audioState = Idle;
  // Bumped by each playAudio/stopAudio: an answer for an older one is dropped.
  quint64 m_audioRequest = 0;
};

// `image://videoposter/<percent-encoded url>`: the first frame, scaled to
// the requested size (the chat's box in device pixels), on a pool of two
// threads (each grab holds a hardware decoder while it runs).
class VideoPosterProvider : public QQuickAsyncImageProvider
{
public:
  VideoPosterProvider();
  ~VideoPosterProvider() override;
  QQuickImageResponse* requestImageResponse(const QString& id, const QSize& requestedSize) override;
};
