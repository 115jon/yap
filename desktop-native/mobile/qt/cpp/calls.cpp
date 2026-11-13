// Native's DM call rings: the Android shim
// (android/src/dev/yap/qtproto/push/IncomingCalls.java). The ring itself is
// the shim's, whichever way it came (a push with the app killed or in the
// background, or the core's CALL_RING handed down by the QML): the
// ringtone in front, the incoming notification and its full-screen intent
// otherwise, Answer and Decline from there. Here: the ring the shim knows
// for the QML's incoming sheet, the QML's answers back to it, and a call
// answered outside the app for the QML to join.

#include "native.h"

#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QMetaObject>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniEnvironment>
#include <QtCore/QJniObject>
#include <QtCore/qcoreapplication_platform.h>
#include <QtCore/qnativeinterface.h>
#endif

namespace {

QVariantMap parse(const QByteArray& json)
{
  return json.isEmpty() ? QVariantMap() : QJsonDocument::fromJson(json).object().toVariantMap();
}

#ifdef Q_OS_ANDROID
constexpr char kCalls[] = "dev/yap/qtproto/push/IncomingCalls";

QByteArray utf8(jstring s)
{
  return s ? QJniObject(s).toString().toUtf8() : QByteArray();
}

// Android's main thread.
void jniRing(JNIEnv*, jclass, jstring json)
{
  const QByteArray call = utf8(json);
  if (Native* native = Native::instance())
    QMetaObject::invokeMethod(native, [native, call] { native->onRing(call); }, Qt::QueuedConnection);
}

void jniRingEnded(JNIEnv*, jclass, jstring callId)
{
  const QString id = QString::fromUtf8(utf8(callId));
  if (Native* native = Native::instance())
    QMetaObject::invokeMethod(native, [native, id] { native->onRingEnded(id); }, Qt::QueuedConnection);
}

void jniAnswered(JNIEnv*, jclass, jstring)
{
  // The call itself is taken with takeAnsweredCall (once, whoever asks).
  if (Native* native = Native::instance())
    QMetaObject::invokeMethod(native, [native] { native->onAnswered(); }, Qt::QueuedConnection);
}

QString toJson(const QVariantMap& map)
{
  return QString::fromUtf8(QJsonDocument(QJsonObject::fromVariantMap(map)).toJson(QJsonDocument::Compact));
}

QJniObject jstr(const QString& s)
{
  return QJniObject::fromString(s);
}
#endif

} // namespace

void Native::installCalls()
{
#ifdef Q_OS_ANDROID
  const JNINativeMethod methods[] = {
    { "nativeRing", "(Ljava/lang/String;)V", reinterpret_cast<void*>(jniRing) },
    { "nativeRingEnded", "(Ljava/lang/String;)V", reinterpret_cast<void*>(jniRingEnded) },
    { "nativeAnswered", "(Ljava/lang/String;)V", reinterpret_cast<void*>(jniAnswered) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kCalls, methods, std::size(methods))) {
    qWarning("[calls] could not register the shim's callbacks");
    return;
  }
  QJniObject::callStaticMethod<void>(kCalls, "install", "(Landroid/content/Context;)V", QNativeInterface::QAndroidApplication::context().object());
  // A ring that started the app (its full-screen intent). An Answer tapped
  // before Qt was up waits in takeAnsweredCall.
  m_incomingCall =
    parse(QJniObject::callStaticObjectMethod(kCalls, "current", "()Ljava/lang/String;").toString().toUtf8());
#endif
}

void Native::ringIncoming(const QVariantMap& call) const
{
#ifdef Q_OS_ANDROID
  const QJniObject json = jstr(toJson(call));
  QJniObject::callStaticMethod<void>(kCalls, "ringFromApp", "(Landroid/content/Context;Ljava/lang/String;)V",
                                     QNativeInterface::QAndroidApplication::context().object(), json.object<jstring>());
#else
  Q_UNUSED(call);
#endif
}

void Native::stopIncoming(const QString& callId, const QString& reason) const
{
#ifdef Q_OS_ANDROID
  const QJniObject id = jstr(callId);
  const QJniObject why = jstr(reason);
  QJniObject::callStaticMethod<void>(kCalls, "stopFromApp",
                                     "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)V", QNativeInterface::QAndroidApplication::context().object(),
                                     id.object<jstring>(), why.object<jstring>());
#else
  Q_UNUSED(callId);
  Q_UNUSED(reason);
#endif
}

void Native::answerIncoming(const QString& callId, bool tellServer) const
{
#ifdef Q_OS_ANDROID
  const QJniObject id = jstr(callId);
  QJniObject::callStaticMethod<void>(kCalls, "answer", "(Landroid/content/Context;Ljava/lang/String;ZZ)V", QNativeInterface::QAndroidApplication::context().object(),
                                     id.object<jstring>(), jboolean(tellServer), jboolean(true));
#else
  Q_UNUSED(callId);
  Q_UNUSED(tellServer);
#endif
}

void Native::declineIncoming(const QString& callId, bool tellServer) const
{
#ifdef Q_OS_ANDROID
  const QJniObject id = jstr(callId);
  QJniObject::callStaticMethod<void>(kCalls, "declineFromApp", "(Landroid/content/Context;Ljava/lang/String;Z)V",
                                     QNativeInterface::QAndroidApplication::context().object(), id.object<jstring>(), jboolean(tellServer));
#else
  Q_UNUSED(callId);
  Q_UNUSED(tellServer);
#endif
}

QVariantMap Native::takeAnsweredCall()
{
#ifdef Q_OS_ANDROID
  return parse(QJniObject::callStaticObjectMethod(kCalls, "takeAnswered", "()Ljava/lang/String;").toString().toUtf8());
#else
  return {};
#endif
}

void Native::onRing(const QByteArray& json)
{
  const QVariantMap call = parse(json);
  if (call == m_incomingCall)
    return;
  m_incomingCall = call;
  emit incomingCallChanged();
}

void Native::onRingEnded(const QString& callId)
{
  if (m_incomingCall.value(QStringLiteral("callId")).toString() != callId)
    return;
  m_incomingCall.clear();
  emit incomingCallChanged();
}

void Native::onAnswered()
{
  emit callAnswered();
}
