// Native's links into the app: the Android shim
// (android/src/dev/yap/qtproto/LinkShim.java, fed by LinkActivity's filters
// through YapActivity) hands over the URL of each VIEW intent, a cold
// start's included; it is kept here for the QML (Main.qml opens it once
// signed in, through Invites.openLink).

#include "native.h"

#include <QtCore/QMetaObject>

#include <utility>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniEnvironment>
#include <QtCore/QJniObject>
#endif

namespace {

#ifdef Q_OS_ANDROID
constexpr char kLinks[] = "dev/yap/qtproto/LinkShim";

// From the activity's thread: onto the GUI thread.
void jniLink(JNIEnv*, jclass, jstring url)
{
  const QString link = url ? QJniObject(url).toString() : QString();
  if (link.isEmpty())
    return;
  if (Native* native = Native::instance())
    QMetaObject::invokeMethod(native, [native, link] { native->onLinkRequest(link); }, Qt::QueuedConnection);
}
#endif

} // namespace

void Native::installLinks()
{
#ifdef Q_OS_ANDROID
  const JNINativeMethod methods[] = {
    { "nativeLink", "(Ljava/lang/String;)V", reinterpret_cast<void*>(jniLink) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kLinks, methods, std::size(methods))) {
    qWarning("[link] could not register the shim's callback");
    return;
  }
  // A link the app was started with comes now (queued: after this
  // constructor, once instance() is set).
  QJniObject::callStaticMethod<void>(kLinks, "install");
#endif
}

QString Native::takeLinkRequest()
{
  return std::exchange(m_linkRequest, {});
}

void Native::onLinkRequest(const QString& url)
{
  // The latest wins, as a second link before sign-in would.
  m_linkRequest = url;
  emit linkRequested();
}
