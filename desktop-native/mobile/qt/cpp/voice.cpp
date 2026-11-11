// Native's voice calls: the Android shim
// (android/src/dev/yap/qtproto/voice/VoiceShim.java) and the core's side
// (rust/src/voice.rs). The call itself is the core's; here are libwebrtc's
// start-up (it needs the JVM and the application context), the microphone
// permission, the audio routes for the QML, and what the call notification
// shows. Qt's event loop sleeps while the app is in the background, so the
// two directions that must work then skip it: the core tells the shim that
// a call started or ended from its own thread (yap_mobile_call_active), and
// the notification's Mute and Leave and an audio focus loss go from the
// shim's thread straight to the core.

#include "native.h"

#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QMetaObject>

#ifdef Q_OS_ANDROID
#include <QtCore/QFuture>
#include <QtCore/QJniEnvironment>
#include <QtCore/QJniObject>
#include <QtCore/private/qandroidextras_p.h>
#include <QtCore/qnativeinterface.h>
#endif

#ifdef Q_OS_ANDROID
extern "C" {
bool yap_mobile_voice_init(void* jvm, void* context);
void yap_mobile_voice_mute(bool muted);
void yap_mobile_voice_deafen(bool deafened);
void yap_mobile_voice_leave();
void yap_mobile_voice_quit();
}

namespace {

constexpr char kVoice[] = "dev/yap/qtproto/voice/VoiceShim";

// On the thread Java called from (VoiceShim.install, itself called from
// Qt's thread below): libwebrtc looks its classes up from here.
jboolean jniInit(JNIEnv* env, jclass, jobject context)
{
  JavaVM* vm = nullptr;
  if (env->GetJavaVM(&vm) != JNI_OK || !vm || !context)
    return JNI_FALSE;
  return yap_mobile_voice_init(vm, context) ? JNI_TRUE : JNI_FALSE;
}

// Android's main thread; the core's command channel takes them from any.
void jniMute(JNIEnv*, jclass, jboolean muted)
{
  yap_mobile_voice_mute(muted == JNI_TRUE);
}

void jniDeafen(JNIEnv*, jclass, jboolean deafened)
{
  yap_mobile_voice_deafen(deafened == JNI_TRUE);
}

void jniLeave(JNIEnv*, jclass)
{
  yap_mobile_voice_leave();
}

// Blocks Android's main thread for up to 800 ms, on the way out only.
void jniQuit(JNIEnv*, jclass)
{
  yap_mobile_voice_quit();
}

void jniRoutes(JNIEnv*, jclass, jstring json)
{
  const QByteArray routes = json ? QJniObject(json).toString().toUtf8() : QByteArray();
  if (Native* native = Native::instance()) {
    QMetaObject::invokeMethod(
      native, [native, routes] { native->onAudioRoutes(routes); }, Qt::QueuedConnection);
  }
}

} // namespace

// From the core (rust/src/voice.rs), on one of its threads: a call runs, or
// the last one ended. QJniObject attaches the thread and finds the class
// through Qt's class loader.
extern "C" void yap_mobile_call_active(bool active)
{
  QJniObject::callStaticMethod<void>(kVoice, "setActive", "(Z)V", jboolean(active));
}
#endif

void Native::installVoice()
{
#ifdef Q_OS_ANDROID
  const JNINativeMethod methods[] = {
    { "nativeInit", "(Landroid/content/Context;)Z", reinterpret_cast<void*>(jniInit) },
    { "nativeMute", "(Z)V", reinterpret_cast<void*>(jniMute) },
    { "nativeDeafen", "(Z)V", reinterpret_cast<void*>(jniDeafen) },
    { "nativeLeave", "()V", reinterpret_cast<void*>(jniLeave) },
    { "nativeQuit", "()V", reinterpret_cast<void*>(jniQuit) },
    { "nativeRoutes", "(Ljava/lang/String;)V", reinterpret_cast<void*>(jniRoutes) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kVoice, methods, std::size(methods))) {
    qWarning("[voice] could not register the shim's callbacks");
    return;
  }
  const bool ok = QJniObject::callStaticMethod<jboolean>(
    kVoice, "install", "(Landroid/app/Activity;)Z", QNativeInterface::QAndroidApplication::context().object());
  if (!ok)
    qWarning("[voice] libwebrtc did not start: calls will not connect");
#endif
}

void Native::requestMicrophone()
{
#ifdef Q_OS_ANDROID
  if (m_askingMicrophone)
    return;
  m_askingMicrophone = true;
  // Answers at once when the permission is already decided either way.
  QtAndroidPrivate::requestPermission(QStringLiteral("android.permission.RECORD_AUDIO"))
    .then(this, [this](QtAndroidPrivate::PermissionResult result) {
      m_askingMicrophone = false;
      emit microphoneAnswered(result == QtAndroidPrivate::Authorized);
    });
#else
  QMetaObject::invokeMethod(this, [this] { emit microphoneAnswered(false); }, Qt::QueuedConnection);
#endif
}

void Native::openAppSettings() const
{
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kVoice, "openAppSettings", "(Landroid/app/Activity;)V",
                                     QNativeInterface::QAndroidApplication::context().object());
#endif
}

void Native::setAudioRoute(const QString& id) const
{
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kVoice, "setRoute", "(Ljava/lang/String;)V",
                                     QJniObject::fromString(id).object<jstring>());
#else
  Q_UNUSED(id);
#endif
}

void Native::setCallState(const QString& channel, bool muted, bool deafened) const
{
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kVoice, "setState", "(Ljava/lang/String;ZZ)V",
                                     QJniObject::fromString(channel).object<jstring>(), jboolean(muted),
                                     jboolean(deafened));
#else
  Q_UNUSED(channel);
  Q_UNUSED(muted);
  Q_UNUSED(deafened);
#endif
}

void Native::callTouched() const
{
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kVoice, "userChanged");
#endif
}

void Native::onAudioRoutes(const QByteArray& json)
{
  const QVariantList routes = QJsonDocument::fromJson(json).array().toVariantList();
  if (routes == m_audioRoutes)
    return;
  m_audioRoutes = routes;
  emit audioRoutesChanged();
}
