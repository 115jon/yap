#pragma once

// A player inside the app's window (QML type `InAppPlayer`, Yap.Mobile; the
// chat's player sheet, qml/MediaSheet.qml). The Qt build has no Multimedia
// and no WebView, so Android draws it (android/src/dev/yap/qtproto/media/
// InAppPlayers.java): a video file on Media3's PlayerView, or a web embed
// player (YouTube, Spotify, TikTok, X, Instagram) in an android.webkit.WebView
// framed as the desktop frames it. The view comes back here as a foreign
// window (QWindow::fromWinId), which QML's WindowContainer places over the
// sheet's body, so it moves with the sheet.
//
// One player per object at a time: playVideo/playWeb replace what played,
// stop() (and the object going) releases it. A video's URL goes to Java
// signed with a media cap (VideoPlayer::resolve), never with the session.
// When the app leaves the front a video lets its decoder go and comes back
// where it was, paused (or as full screen left it); a web page pauses.

#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtGui/QWindow>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniObject>
#endif

class InAppPlayer : public QObject
{
  Q_OBJECT
  // Whether this build plays in the app at all (Android).
  Q_PROPERTY(bool available READ available CONSTANT)
  // The player's view, for a WindowContainer (null: none yet, or gone).
  Q_PROPERTY(QWindow* window READ window NOTIFY windowChanged)
  Q_PROPERTY(State state READ state NOTIFY stateChanged)
  // The web player's own full screen is up (back leaves it first).
  Q_PROPERTY(bool fullScreen READ fullScreen NOTIFY fullScreenChanged)

public:
  enum State { Idle, Loading, Ready, Failed };
  Q_ENUM(State)

  explicit InAppPlayer(QObject* parent = nullptr);
  ~InAppPlayer() override;

  bool available() const;
  QWindow* window() const { return m_window; }
  State state() const { return m_state; }
  bool fullScreen() const { return m_fullScreen; }

  // A video file from `positionMs`; `loop` for a GIF. `title` names it for
  // TalkBack.
  Q_INVOKABLE void playVideo(const QString& url, qint64 positionMs = 0, bool loop = false,
                             const QString& title = QString());
  // A web embed's player page (the desktop WebPlayer's `url`); only the
  // hosts webAllowed() takes.
  Q_INVOKABLE void playWeb(const QString& url, const QString& title = QString());
  // Whether the web player frames `url` (the desktop's rule).
  Q_INVOKABLE bool webAllowed(const QString& url) const;
  Q_INVOKABLE void leaveFullScreen();
  Q_INVOKABLE void stop();

#ifdef Q_OS_ANDROID
  // From Java, on this object's thread.
  void onViewMade(qint64 id, const QJniObject& view);
  void onEvent(qint64 id, int event);
#endif

signals:
  void windowChanged();
  void stateChanged();
  void fullScreenChanged();

private:
  void setState(State state);
  void setFullScreen(bool on);
  void dropWindow();
  qint64 begin();

  // The player playing now (0: none); each play takes a new one, so an
  // answer for an older player is dropped.
  qint64 m_id = 0;
  QWindow* m_window = nullptr;
#ifdef Q_OS_ANDROID
  QJniObject m_view;
#endif
  State m_state = Idle;
  bool m_fullScreen = false;
  bool m_away = false;
};
