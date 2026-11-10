#include "native.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QMetaObject>
#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>
#include <QtGui/QKeyEvent>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniEnvironment>
#include <QtCore/QJniObject>
#include <QtCore/qnativeinterface.h>
#endif

extern "C" void yap_mobile_flush();

namespace {

Native* g_native = nullptr;

#ifdef Q_OS_ANDROID
constexpr char kShim[] = "dev/yap/qtproto/NativeShim";

// The shim calls these on Android's UI thread; each hops to the GUI thread.
template <typename F>
void post(F&& f)
{
  if (Native* native = g_native)
    QMetaObject::invokeMethod(native, std::forward<F>(f), Qt::QueuedConnection);
}

void jniBackProgress(JNIEnv*, jclass, jfloat progress)
{
  post([progress] { g_native->onBackProgress(progress); });
}

void jniBackCancelled(JNIEnv*, jclass)
{
  post([] { g_native->onBackCancelled(); });
}

void jniBackInvoked(JNIEnv*, jclass)
{
  post([] { g_native->onBackInvoked(); });
}

void jniIme(JNIEnv*, jclass, jfloat dp)
{
  post([dp] { g_native->onIme(dp); });
}
#endif

} // namespace

Native::Native(QObject* parent)
  : QObject(parent)
{
  g_native = this;
  // Before API 33 (and on a hardware Back key) Qt delivers Back as a key.
  qGuiApp->installEventFilter(this);
#ifdef Q_OS_ANDROID
  const JNINativeMethod methods[] = {
    { "nativeBackProgress", "(F)V", reinterpret_cast<void*>(jniBackProgress) },
    { "nativeBackCancelled", "()V", reinterpret_cast<void*>(jniBackCancelled) },
    { "nativeBackInvoked", "()V", reinterpret_cast<void*>(jniBackInvoked) },
    { "nativeIme", "(F)V", reinterpret_cast<void*>(jniIme) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kShim, methods, std::size(methods)))
    qWarning("[native] could not register the shim's callbacks");
  QJniObject::callStaticMethod<void>(kShim, "install", "(Landroid/app/Activity;)V",
                                     QNativeInterface::QAndroidApplication::context().object());
  m_maxFling = QJniObject::callStaticMethod<jfloat>(kShim, "maxFlingVelocity");
  m_deceleration = QJniObject::callStaticMethod<jfloat>(kShim, "flingDeceleration");
  readConfiguration();
  // Text size, Remove animations and the wallpaper accent can change while
  // the app is away; read them again when it comes back.
  connect(qGuiApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState s) {
    if (s == Qt::ApplicationActive)
      readConfiguration();
  });
#endif
}

Native* Native::instance()
{
  return g_native;
}

void Native::setHandlesBack(bool on)
{
  if (m_handlesBack == on)
    return;
  m_handlesBack = on;
#ifdef Q_OS_ANDROID
  // Registered only while the app has somewhere to go back to, so at the
  // root the system plays its own back-to-home preview.
  QJniObject::callStaticMethod<void>(kShim, "setHandlesBack", "(Z)V", jboolean(on));
#endif
  emit handlesBackChanged();
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

void Native::onBackProgress(qreal progress)
{
  emit backProgressed(progress);
}

void Native::onBackCancelled()
{
  emit backCancelled();
}

void Native::onBackInvoked()
{
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
  if (event->type() == QEvent::KeyRelease && m_handlesBack) {
    if (static_cast<QKeyEvent*>(event)->key() == Qt::Key_Back) {
      onBackInvoked();
      return true;
    }
  }
  return QObject::eventFilter(watched, event);
}
