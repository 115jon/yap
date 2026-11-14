// Native's links and shares into the app: the Android shims
// (android/src/dev/yap/qtproto/LinkShim.java, fed by LinkActivity's filters
// through YapActivity, and ShareShim.java, fed by ShareActivity's) hand over
// the URL of each VIEW intent and what each SEND brought, a cold start's
// included; they are kept here for the QML (Main.qml opens a link once
// signed in, through Invites.openLink, and asks where a share goes,
// SharePickerPage).

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
constexpr char kShares[] = "dev/yap/qtproto/ShareShim";

// From the activity's thread: onto the GUI thread.
void jniLink(JNIEnv*, jclass, jstring url)
{
  const QString link = url ? QJniObject(url).toString() : QString();
  if (link.isEmpty())
    return;
  if (Native* native = Native::instance())
    QMetaObject::invokeMethod(native, [native, link] { native->onLinkRequest(link); }, Qt::QueuedConnection);
}

void jniShare(JNIEnv*, jclass, jstring text, jint files, jint dropped)
{
  const QString body = text ? QJniObject(text).toString() : QString();
  if (Native* native = Native::instance())
    QMetaObject::invokeMethod(
      native, [native, body, files, dropped] { native->onShareRequest(body, files, dropped); },
      Qt::QueuedConnection);
}

// From the copy's thread.
void jniShareCopied(JNIEnv* env, jclass, jobjectArray paths, jint failed)
{
  QStringList list;
  const jsize n = paths ? env->GetArrayLength(paths) : 0;
  for (jsize i = 0; i < n; ++i) {
    auto path = static_cast<jstring>(env->GetObjectArrayElement(paths, i));
    list.append(QJniObject(path).toString());
    env->DeleteLocalRef(path);
  }
  if (Native* native = Native::instance())
    QMetaObject::invokeMethod(native, [native, list, failed] { native->onShareCopied(list, failed); },
                              Qt::QueuedConnection);
}
#endif

} // namespace

void Native::installLinks()
{
#ifdef Q_OS_ANDROID
  const JNINativeMethod methods[] = {
    { "nativeLink", "(Ljava/lang/String;)V", reinterpret_cast<void*>(jniLink) },
  };
  const JNINativeMethod shares[] = {
    { "nativeShare", "(Ljava/lang/String;II)V", reinterpret_cast<void*>(jniShare) },
    { "nativeShareCopied", "([Ljava/lang/String;I)V", reinterpret_cast<void*>(jniShareCopied) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kLinks, methods, std::size(methods))) {
    qWarning("[link] could not register the shim's callback");
  } else {
    // A link the app was started with comes now (queued: after this
    // constructor, once instance() is set).
    QJniObject::callStaticMethod<void>(kLinks, "install");
  }
  if (!env.registerNativeMethods(kShares, shares, std::size(shares))) {
    qWarning("[share] could not register the shim's callbacks");
    return;
  }
  QJniObject::callStaticMethod<void>(kShares, "install");
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

QVariantMap Native::takeShareRequest()
{
  return std::exchange(m_shareRequest, {});
}

void Native::copyShare(qreal limitBytes) const
{
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kShares, "copy", "(J)V", static_cast<jlong>(limitBytes));
#else
  Q_UNUSED(limitBytes);
#endif
}

void Native::dropShare()
{
  m_shareRequest.clear();
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kShares, "drop");
#endif
}

void Native::onShareRequest(const QString& text, int files, int dropped)
{
  // The latest wins: the shim holds only its files.
  m_shareRequest = {
    { QStringLiteral("text"), text },
    { QStringLiteral("files"), files },
    { QStringLiteral("dropped"), dropped },
  };
  emit shareRequested();
}

void Native::onShareCopied(const QStringList& paths, int failed)
{
  emit shareCopied(paths, failed);
}
