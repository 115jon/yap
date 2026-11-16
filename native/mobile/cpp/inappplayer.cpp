// InAppPlayer (inappplayer.h).

#include "inappplayer.h"

#include "videoplayer.h"

#include <QtCore/QHash>
#include <QtCore/QPointer>
#include <QtGui/QGuiApplication>

#include <iterator>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniEnvironment>
#include <QtCore/qcoreapplication_platform.h>
#endif

namespace {

#ifdef Q_OS_ANDROID
constexpr char kShim[] = "dev/yap/qtproto/media/InAppPlayers";

// InAppPlayers.java's events.
constexpr int kReady = 1;
constexpr int kFailed = 2;
constexpr int kFullScreen = 3;
constexpr int kWindowed = 4;
#endif

// GUI thread only: the players by the id each play took.
QHash<qint64, QPointer<InAppPlayer>>& players()
{
  static QHash<qint64, QPointer<InAppPlayer>> map;
  return map;
}

qint64 g_next = 0;

#ifdef Q_OS_ANDROID
QJniObject androidContext()
{
  return QNativeInterface::QAndroidApplication::context();
}

void jniViewMade(JNIEnv* env, jclass, jlong id, jobject view)
{
  // A global reference, for the GUI thread to take its own from.
  jobject global = env->NewGlobalRef(view);
  QMetaObject::invokeMethod(qApp, [id, global] {
    QJniObject held(global);
    QJniEnvironment().jniEnv()->DeleteGlobalRef(global);
    if (InAppPlayer* player = players().value(qint64(id)))
      player->onViewMade(qint64(id), held);
  }, Qt::QueuedConnection);
}

void jniEvent(JNIEnv*, jclass, jlong id, jint event)
{
  QMetaObject::invokeMethod(qApp, [id, event] {
    if (InAppPlayer* player = players().value(qint64(id)))
      player->onEvent(qint64(id), int(event));
  }, Qt::QueuedConnection);
}

void registerNatives()
{
  static bool done = false;
  if (done)
    return;
  done = true;
  const JNINativeMethod methods[] = {
    { "nativeViewMade", "(JLandroid/view/View;)V", reinterpret_cast<void*>(jniViewMade) },
    { "nativeEvent", "(JI)V", reinterpret_cast<void*>(jniEvent) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kShim, methods, std::size(methods)))
    qWarning("[media] could not register the in-app player's callbacks");
}
#endif

} // namespace

InAppPlayer::InAppPlayer(QObject* parent)
  : QObject(parent)
{
#ifdef Q_OS_ANDROID
  registerNatives();
#endif
  // Out of the front, a video lets its decoder go and a page pauses; back
  // in front they pick up.
  connect(qGuiApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState state) {
    const bool away = state != Qt::ApplicationActive;
    if (away == m_away)
      return;
    m_away = away;
#ifdef Q_OS_ANDROID
    if (m_id != 0)
      QJniObject::callStaticMethod<void>(kShim, away ? "background" : "foreground", "(J)V", jlong(m_id));
#endif
  });
}

InAppPlayer::~InAppPlayer()
{
  stop();
}

bool InAppPlayer::available() const
{
#ifdef Q_OS_ANDROID
  return true;
#else
  return false;
#endif
}

qint64 InAppPlayer::begin()
{
  stop();
  m_id = ++g_next;
  players().insert(m_id, this);
  m_away = QGuiApplication::applicationState() != Qt::ApplicationActive;
  setState(Loading);
  return m_id;
}

void InAppPlayer::playVideo(const QString& url, qint64 positionMs, bool loop, const QString& title)
{
  const qint64 id = begin();
#ifdef Q_OS_ANDROID
  VideoPlayer* resolver = VideoPlayer::instance();
  if (!resolver) {
    setState(Failed);
    return;
  }
  QPointer<InAppPlayer> self(this);
  resolver->resolve(url, [self, id, positionMs, loop, title](const QString& playable) {
    if (!self || self->m_id != id)
      return;
    if (playable.isEmpty()) {
      self->setState(Failed);
      return;
    }
    QJniObject::callStaticMethod<void>(
      kShim, "video", "(Landroid/content/Context;JLjava/lang/String;JZLjava/lang/String;)V",
      androidContext().object(), jlong(id), QJniObject::fromString(playable).object<jstring>(),
      jlong(qMax<qint64>(0, positionMs)), jboolean(loop), QJniObject::fromString(title).object<jstring>());
    // Left the front while the link was being signed: the player is made
    // after that word went by, so it is sent again (Android's main thread
    // takes both in order).
    if (self->m_away)
      QJniObject::callStaticMethod<void>(kShim, "background", "(J)V", jlong(id));
  });
#else
  Q_UNUSED(id);
  Q_UNUSED(url);
  Q_UNUSED(positionMs);
  Q_UNUSED(loop);
  Q_UNUSED(title);
  setState(Failed);
#endif
}

void InAppPlayer::playWeb(const QString& url, const QString& title)
{
  const qint64 id = begin();
#ifdef Q_OS_ANDROID
  if (!webAllowed(url)) {
    setState(Failed);
    return;
  }
  QJniObject::callStaticMethod<void>(kShim, "web", "(Landroid/content/Context;JLjava/lang/String;Ljava/lang/String;)V",
                                     androidContext().object(), jlong(id),
                                     QJniObject::fromString(url).object<jstring>(),
                                     QJniObject::fromString(title).object<jstring>());
#else
  Q_UNUSED(id);
  Q_UNUSED(url);
  Q_UNUSED(title);
  setState(Failed);
#endif
}

bool InAppPlayer::webAllowed(const QString& url) const
{
#ifdef Q_OS_ANDROID
  return QJniObject::callStaticMethod<jboolean>(kShim, "allowed", "(Ljava/lang/String;)Z",
                                                QJniObject::fromString(url).object<jstring>());
#else
  Q_UNUSED(url);
  return false;
#endif
}

void InAppPlayer::leaveFullScreen()
{
#ifdef Q_OS_ANDROID
  if (m_id != 0 && m_fullScreen)
    QJniObject::callStaticMethod<void>(kShim, "windowed", "(J)V", jlong(m_id));
#endif
}

void InAppPlayer::stop()
{
  if (m_id == 0)
    return;
  const qint64 id = m_id;
  m_id = 0;
  players().remove(id);
  // Qt takes the view off its window first (posted to Android's main
  // thread), then Java releases the player after it.
  dropWindow();
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kShim, "release", "(J)V", jlong(id));
#endif
  setFullScreen(false);
  setState(Idle);
}

void InAppPlayer::dropWindow()
{
  if (!m_window)
    return;
  QWindow* gone = m_window;
  m_window = nullptr;
  // The container lets it go first.
  emit windowChanged();
  gone->hide();
  delete gone;
#ifdef Q_OS_ANDROID
  m_view = QJniObject();
#endif
}

#ifdef Q_OS_ANDROID
void InAppPlayer::onViewMade(qint64 id, const QJniObject& view)
{
  if (id != m_id || !view.isValid())
    return;
  dropWindow();
  // Qt's Android foreign window: the view in a layout of its own, placed
  // where the window is put.
  // The view's reference lives as long as the window, which keeps the raw
  // handle to make itself again.
  m_view = view;
  m_window = QWindow::fromWinId(reinterpret_cast<WId>(m_view.object()));
  if (!m_window) {
    setState(Failed);
    return;
  }
  emit windowChanged();
}

void InAppPlayer::onEvent(qint64 id, int event)
{
  if (id != m_id)
    return;
  switch (event) {
  case kReady:
    if (m_state == Loading)
      setState(Ready);
    break;
  case kFailed: {
    // The view is dead or never came: off the window.
    dropWindow();
    setFullScreen(false);
    setState(Failed);
    break;
  }
  case kFullScreen:
    setFullScreen(true);
    break;
  case kWindowed:
    setFullScreen(false);
    break;
  default:
    break;
  }
}
#endif

void InAppPlayer::setState(State state)
{
  if (state == m_state)
    return;
  m_state = state;
  emit stateChanged();
}

void InAppPlayer::setFullScreen(bool on)
{
  if (on == m_fullScreen)
    return;
  m_fullScreen = on;
  emit fullScreenChanged();
}
