#include "native.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QDateTime>
#include <QtCore/QMetaObject>
#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>
#include <QtGui/QKeyEvent>

#include <atomic>
#include <utility>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniEnvironment>
#include <QtCore/QJniObject>
#include <QtCore/private/qandroidextras_p.h>
#include <QtCore/qnativeinterface.h>
#endif

extern "C" void yap_mobile_flush();

// The device's network for the core (rust/src/client.rs, from a Tokio
// thread): 1 up, 0 none, -1 unknown.
extern "C" int yap_mobile_network_state()
{
#ifdef Q_OS_ANDROID
  return QJniObject::callStaticMethod<jint>("dev/yap/qtproto/NetworkState", "get",
                                            "(Landroid/content/Context;)I",
                                            QNativeInterface::QAndroidApplication::context().object());
#else
  return -1;
#endif
}

namespace {

std::atomic<Native*> g_native { nullptr };

#ifdef Q_OS_ANDROID
constexpr char kShim[] = "dev/yap/qtproto/NativeShim";

// The shim calls these on Android's UI thread; each hops to the GUI thread.
template <typename F>
void post(F&& f)
{
  if (Native* native = g_native.load())
    QMetaObject::invokeMethod(native, std::forward<F>(f), Qt::QueuedConnection);
}

void jniBackProgress(JNIEnv*, jclass, jfloat progress)
{
  post([progress] { Native::instance()->onBackProgress(progress); });
}

void jniBackCancelled(JNIEnv*, jclass)
{
  post([] { Native::instance()->onBackCancelled(); });
}

void jniBackInvoked(JNIEnv*, jclass)
{
  post([] { Native::instance()->onBackInvoked(); });
}

void jniIme(JNIEnv*, jclass, jfloat dp)
{
  post([dp] { Native::instance()->onIme(dp); });
}

void jniMediaCopying(JNIEnv*, jclass)
{
  post([] { Native::instance()->onMediaCopying(); });
}

void jniMediaPicked(JNIEnv* env, jclass, jobjectArray paths, jint failed)
{
  QStringList list;
  const jsize n = paths ? env->GetArrayLength(paths) : 0;
  for (jsize i = 0; i < n; ++i) {
    auto path = static_cast<jstring>(env->GetObjectArrayElement(paths, i));
    list.append(QJniObject(path).toString());
    env->DeleteLocalRef(path);
  }
  post([list, failed] { Native::instance()->onMediaPicked(list, failed); });
}

// The picker's request code (any number of ours; Qt routes the result back).
constexpr int kPickMedia = 0x5961;
#endif

} // namespace

Native::Native(QObject* parent)
  : QObject(parent)
{
  g_native = this;
  // Before API 33 (and on a hardware Back key) Qt delivers Back as a key.
  qGuiApp->installEventFilter(this);
#ifdef Q_OS_ANDROID
  m_backByCallback = QNativeInterface::QAndroidApplication::sdkVersion() >= 33;
  const JNINativeMethod methods[] = {
    { "nativeBackProgress", "(F)V", reinterpret_cast<void*>(jniBackProgress) },
    { "nativeBackCancelled", "()V", reinterpret_cast<void*>(jniBackCancelled) },
    { "nativeBackInvoked", "()V", reinterpret_cast<void*>(jniBackInvoked) },
    { "nativeIme", "(F)V", reinterpret_cast<void*>(jniIme) },
    { "nativeMediaCopying", "()V", reinterpret_cast<void*>(jniMediaCopying) },
    { "nativeMediaPicked", "([Ljava/lang/String;I)V", reinterpret_cast<void*>(jniMediaPicked) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kShim, methods, std::size(methods)))
    qWarning("[native] could not register the shim's callbacks");
  QJniObject::callStaticMethod<void>(kShim, "install", "(Landroid/app/Activity;)V",
                                     QNativeInterface::QAndroidApplication::context().object());
  m_maxFling = QJniObject::callStaticMethod<jfloat>(kShim, "maxFlingVelocity");
  m_deceleration = QJniObject::callStaticMethod<jfloat>(kShim, "flingDeceleration");
  readConfiguration();
  applyBars();
  installPush();
  installVoice();
  // Text size, Remove animations and the wallpaper accent can change while
  // the app is away; read them again when it comes back, and the keyboard's
  // inset, which can go stale behind another activity (the photo picker).
  connect(qGuiApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState s) {
    if (s != Qt::ApplicationActive)
      m_leftSincePick = true;
    if (s == Qt::ApplicationActive) {
      readConfiguration();
      // The permission or the app's switch may have changed in Settings.
      readNotificationsEnabled();
      QJniObject::callStaticMethod<void>(kShim, "refreshIme");
    }
  });
#endif
}

Native::~Native()
{
  g_native = nullptr;
}

Native* Native::instance()
{
  return g_native.load();
}

void Native::setHandlesBack(bool on)
{
  if (m_handlesBack == on)
    return;
  m_handlesBack = on;
  applyBack();
  emit handlesBackChanged();
}

QObject* Native::backLayer()
{
  m_backLayers.removeAll(QPointer<QObject>());
  return m_backLayers.isEmpty() ? nullptr : m_backLayers.last().data();
}

void Native::applyBack()
{
  const bool take = m_handlesBack || backLayer();
  if (std::exchange(m_backTaken, take) == take)
    return;
#ifdef Q_OS_ANDROID
  // Registered while the app has somewhere to go back to (a page to pop, a
  // layer to close). At the root the system plays its own back-to-home
  // preview, unless it would finish the activity there; then the shim keeps
  // the callback and the app leaves by moving its task back
  // (NativeShim.keepsTaskAtRoot).
  QJniObject::callStaticMethod<void>(kShim, "setHandlesBack", "(Z)V", jboolean(take));
#endif
}

void Native::pushBackLayer(QObject* layer)
{
  if (!layer)
    return;
  m_backLayers.removeAll(QPointer<QObject>(layer));
  m_backLayers.append(QPointer<QObject>(layer));
  // A layer destroyed while up gives the gesture back.
  connect(layer, &QObject::destroyed, this, &Native::applyBack, Qt::UniqueConnection);
  applyBack();
}

void Native::popBackLayer(QObject* layer)
{
  m_backLayers.removeAll(QPointer<QObject>(layer));
  applyBack();
}

void Native::setLightBars(bool on)
{
  if (m_lightBars == on)
    return;
  m_lightBars = on;
  applyBars();
  emit lightBarsChanged();
}

void Native::holdDarkBars(QObject* holder)
{
  if (!holder || m_darkHolds.contains(QPointer<QObject>(holder)))
    return;
  m_darkHolds.append(QPointer<QObject>(holder));
  connect(holder, &QObject::destroyed, this, &Native::applyBars, Qt::UniqueConnection);
  applyBars();
}

void Native::releaseDarkBars(QObject* holder)
{
  m_darkHolds.removeAll(QPointer<QObject>(holder));
  applyBars();
}

void Native::applyBars()
{
  m_darkHolds.removeAll(QPointer<QObject>());
  const bool light = m_lightBars && m_darkHolds.isEmpty();
  if (std::exchange(m_barsLight, int(light)) == int(light))
    return;
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kShim, "setLightBars", "(Z)V", jboolean(light));
#endif
}

void Native::readConfiguration()
{
#ifdef Q_OS_ANDROID
  m_fontScale = QJniObject::callStaticMethod<jfloat>(kShim, "fontScale");
  m_reducedMotion = QJniObject::callStaticMethod<jboolean>(kShim, "reducedMotion");
  const jint argb = QJniObject::callStaticMethod<jint>(kShim, "accent");
  m_accent = argb ? QColor::fromRgba(QRgb(argb)) : QColor();
#endif
  emit configurationChanged();
}

void Native::haptic(Haptic kind) const
{
  // The QML keeps running in the background; nothing there is felt for.
  if (QGuiApplication::applicationState() == Qt::ApplicationSuspended)
    return;
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kShim, "haptic", "(I)V", jint(kind));
#else
  Q_UNUSED(kind);
#endif
}

void Native::share(const QString& text) const
{
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kShim, "share", "(Ljava/lang/String;)V",
                                     QJniObject::fromString(text).object<jstring>());
#else
  Q_UNUSED(text);
#endif
}

void Native::copy(const QString& text) const
{
  QGuiApplication::clipboard()->setText(text);
}

void Native::leave() const
{
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kShim, "leave");
#endif
}

void Native::flushCache() const
{
  yap_mobile_flush();
}

void Native::pickMedia(qreal limitBytes)
{
#ifdef Q_OS_ANDROID
  // One picker at a time (a double tap on +). A picker that never showed
  // (the app stayed in front for a few seconds) does not block the next.
  const qint64 now = QDateTime::currentMSecsSinceEpoch();
  if (m_pickingMedia || (m_pickerOpen && (m_leftSincePick || now - m_pickStartedMs < 3000)))
    return;
  const QJniObject intent = QJniObject::callStaticObjectMethod(
      kShim, "pickMediaIntent", "(J)Landroid/content/Intent;", jlong(limitBytes));
  if (!intent.isValid())
    return;
  m_pickerOpen = true;
  m_leftSincePick = false;
  m_pickStartedMs = now;
  // The result comes back on Android's UI thread; the shim copies the items
  // off it and reports through nativeMediaPicked.
  QtAndroidPrivate::startActivity(intent, kPickMedia, [](int, int resultCode, const QJniObject& data) {
    QJniObject::callStaticMethod<void>(kShim, "mediaPicked", "(ILandroid/content/Intent;)V",
                                       jint(resultCode), data.object());
  });
#else
  Q_UNUSED(limitBytes);
#endif
}

void Native::onMediaCopying()
{
  if (m_pickingMedia)
    return;
  m_pickingMedia = true;
  emit pickingMediaChanged();
}

void Native::onMediaPicked(const QStringList& paths, int failed)
{
  m_pickerOpen = false;
  if (m_pickingMedia) {
    m_pickingMedia = false;
    emit pickingMediaChanged();
  }
  emit mediaPicked(paths, failed);
}

void Native::onBackProgress(qreal progress)
{
  if (QObject* layer = backLayer()) {
    QMetaObject::invokeMethod(layer, "backProgressed", Q_ARG(QVariant, QVariant(progress)));
    return;
  }
  emit backProgressed(progress);
}

void Native::onBackCancelled()
{
  if (QObject* layer = backLayer()) {
    QMetaObject::invokeMethod(layer, "backCancelled");
    return;
  }
  emit backCancelled();
}

void Native::onBackInvoked()
{
  if (QObject* layer = backLayer()) {
    QMetaObject::invokeMethod(layer, "backInvoked");
    return;
  }
  emit backProgressed(0);
  emit backInvoked();
}

void Native::onIme(qreal dp)
{
  if (qFuzzyCompare(m_imeHeight + 1, dp + 1))
    return;
  m_imeHeight = dp;
  emit imeHeightChanged();
}

bool Native::eventFilter(QObject* watched, QEvent* event)
{
  // Back as a key. From API 33 the shim's callback (or the system's own
  // handler at the root) takes Back, yet Qt still hears the key too (a
  // Back key and the gesture on some phones): taking both stepped twice, a
  // page closed and the chat under it went to the drawer. Before API 33
  // the key is the only Back: a page to pop, or leaving at the root, never
  // Qt's default of closing the window.
  if (event->type() == QEvent::KeyPress || event->type() == QEvent::KeyRelease) {
    if (static_cast<QKeyEvent*>(event)->key() == Qt::Key_Back) {
      if (event->type() == QEvent::KeyRelease && !m_backByCallback)
        onBackInvoked();
      return true;
    }
  }
  return QObject::eventFilter(watched, event);
}
