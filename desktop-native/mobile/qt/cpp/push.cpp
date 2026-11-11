// Native's push notifications: the Android shim
// (android/src/dev/yap/qtproto/push/PushShim.java) and the core's side
// (rust/src/push.rs). The shim hands over FCM's token, a notification's
// tap, and its Reply and Mark as read while the app runs; with the app
// killed its service and receiver work on their own. Here: the
// conversation on screen, the permission, and the tap's conversation for
// the QML (Main.qml opens it once signed in).

#include "native.h"

#include <QtCore/QMetaObject>
#include <QtCore/QStandardPaths>

#include <utility>

#ifdef Q_OS_ANDROID
#include <QtCore/QFuture>
#include <QtCore/QJniEnvironment>
#include <QtCore/QJniObject>
#include <QtCore/private/qandroidextras_p.h>
#include <QtCore/qnativeinterface.h>
#endif

extern "C" {
void yap_mobile_push_token(const char* token);
void yap_mobile_notification_reply(const char* channel_id, const char* message_id, const char* content);
void yap_mobile_notification_mark_read(const char* channel_id);
const char* yap_mobile_api_base();
}

namespace {

#ifdef Q_OS_ANDROID
constexpr char kPush[] = "dev/yap/qtproto/push/PushShim";

QString text(jstring s)
{
  return s ? QJniObject(s).toString() : QString();
}

// FCM's callbacks come on its own threads; the core's channel takes them
// from any thread.
void jniToken(JNIEnv*, jclass, jstring token)
{
  yap_mobile_push_token(text(token).toUtf8().constData());
}

void jniOpen(JNIEnv*, jclass, jstring channel, jstring server, jstring user)
{
  const QString channelId = text(channel);
  const QString serverId = text(server);
  const QString userId = text(user);
  if (Native* native = Native::instance()) {
    QMetaObject::invokeMethod(
      native, [native, channelId, serverId, userId] { native->onOpenRequest(channelId, serverId, userId); },
      Qt::QueuedConnection);
  }
}

void jniReply(JNIEnv*, jclass, jstring channel, jstring message, jstring content)
{
  yap_mobile_notification_reply(text(channel).toUtf8().constData(), text(message).toUtf8().constData(),
                                text(content).toUtf8().constData());
}

void jniMarkRead(JNIEnv*, jclass, jstring channel)
{
  yap_mobile_notification_mark_read(text(channel).toUtf8().constData());
}
#endif

} // namespace

void Native::installPush()
{
#ifdef Q_OS_ANDROID
  const JNINativeMethod methods[] = {
    { "nativeToken", "(Ljava/lang/String;)V", reinterpret_cast<void*>(jniToken) },
    { "nativeOpen", "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
      reinterpret_cast<void*>(jniOpen) },
    { "nativeReply", "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
      reinterpret_cast<void*>(jniReply) },
    { "nativeMarkRead", "(Ljava/lang/String;)V", reinterpret_cast<void*>(jniMarkRead) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kPush, methods, std::size(methods))) {
    qWarning("[push] could not register the shim's callbacks");
    return;
  }
  // The core's data folder (main.cpp) holds the session file the service
  // reads with the app killed.
  const QString data = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
  QJniObject::callStaticMethod<void>(kPush, "install",
                                     "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)V",
                                     QNativeInterface::QAndroidApplication::context().object(),
                                     QJniObject::fromString(data).object<jstring>(),
                                     QJniObject::fromString(QString::fromUtf8(yap_mobile_api_base())).object<jstring>());
  readNotificationsEnabled();
#endif
}

void Native::readNotificationsEnabled()
{
#ifdef Q_OS_ANDROID
  const bool enabled = QJniObject::callStaticMethod<jboolean>(kPush, "notificationsEnabled");
  if (enabled == m_notificationsEnabled)
    return;
  m_notificationsEnabled = enabled;
  emit notificationsEnabledChanged();
#endif
}

void Native::setActiveConversation(const QString& channelId)
{
  if (channelId == m_activeConversation)
    return;
  m_activeConversation = channelId;
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kPush, "setActiveConversation", "(Ljava/lang/String;)V",
                                     QJniObject::fromString(channelId).object<jstring>());
  // The first conversation opened asks for the permission (API 33+, once).
  if (!channelId.isEmpty() && !m_notificationsEnabled
      && QJniObject::callStaticMethod<jboolean>(kPush, "neverAsked"))
    requestNotifications();
#endif
  emit activeConversationChanged();
}

void Native::requestNotifications()
{
#ifdef Q_OS_ANDROID
  readNotificationsEnabled();
  if (m_notificationsEnabled)
    return;
  if (!QJniObject::callStaticMethod<jboolean>(kPush, "canAsk")) {
    // Refused for good, or before API 33 (off in Settings): only the app's
    // notification settings can turn them on.
    QJniObject::callStaticMethod<void>(kPush, "openNotificationSettings");
    return;
  }
  QJniObject::callStaticMethod<void>(kPush, "markAsked");
  QtAndroidPrivate::requestPermission(QStringLiteral("android.permission.POST_NOTIFICATIONS"))
    .then(this, [this](QtAndroidPrivate::PermissionResult) { readNotificationsEnabled(); });
#endif
}

QVariantMap Native::takeOpenRequest()
{
  return std::exchange(m_openRequest, {});
}

void Native::forgetNotifications() const
{
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kPush, "forgetNotifications");
#endif
}

void Native::onOpenRequest(const QString& channelId, const QString& serverId, const QString& userId)
{
  m_openRequest = { { QStringLiteral("channelId"), channelId },
                    { QStringLiteral("serverId"), serverId },
                    { QStringLiteral("userId"), userId } };
  emit openRequested();
}
