// Native's voice calls: the Android shim
// (android/src/dev/yap/qtproto/voice/VoiceShim.java) and the core's side
// (rust/src/voice.rs). The call itself is the core's; here are libwebrtc's
// start-up (it needs the JVM and the application context), the microphone
// permission, the audio routes for the QML, what the call notification
// shows, and Settings' mic test (MicTest.java). Two directions do not go through Qt's thread, so they work whatever
// it is doing (and in a process the system started for the service alone):
// the core tells the shim that a call started or ended from its own thread
// (yap_mobile_call_active), and the notification's Mute and Leave and an
// audio focus loss go from the shim's thread straight to the core.

#include "native.h"

#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QMetaObject>

#include <algorithm>
#include <cmath>

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
void yap_mobile_speaker_muted(bool muted);
void yap_mobile_voice_leave();
void yap_mobile_voice_quit();
}

namespace {

constexpr char kVoice[] = "dev/yap/qtproto/voice/VoiceShim";
constexpr char kMicTest[] = "dev/yap/qtproto/voice/MicTest";

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

void jniSpeakerMuted(JNIEnv*, jclass, jboolean muted)
{
  yap_mobile_speaker_muted(muted == JNI_TRUE);
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

// The mic test's thread, every 20 ms.
void jniMicLevel(JNIEnv*, jclass, jdouble rms)
{
  if (Native* native = Native::instance()) {
    const double value = rms;
    QMetaObject::invokeMethod(
      native, [native, value] { native->onMicLevel(value); }, Qt::QueuedConnection);
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
    { "nativeSpeakerMuted", "(Z)V", reinterpret_cast<void*>(jniSpeakerMuted) },
    { "nativeLeave", "()V", reinterpret_cast<void*>(jniLeave) },
    { "nativeQuit", "()V", reinterpret_cast<void*>(jniQuit) },
    { "nativeRoutes", "(Ljava/lang/String;)V", reinterpret_cast<void*>(jniRoutes) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kVoice, methods, std::size(methods))) {
    qWarning("[voice] could not register the shim's callbacks");
    return;
  }
  const JNINativeMethod micTest[] = {
    { "nativeLevel", "(D)V", reinterpret_cast<void*>(jniMicLevel) },
  };
  if (!env.registerNativeMethods(kMicTest, micTest, std::size(micTest)))
    qWarning("[voice] could not register the mic test's callback");
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
  // Already allowed: answered without asking. Asking anyway can fail while
  // Qt's accessibility bridge holds its lock ("Failed to acquire deadlock
  // protector for requestPermissionsInternal()"), which reads as a refusal.
  if (QJniObject::callStaticMethod<jboolean>(kVoice, "microphoneAllowed", "(Landroid/content/Context;)Z",
                                             QNativeInterface::QAndroidApplication::context().object())) {
    QMetaObject::invokeMethod(this, [this] { emit microphoneAnswered(true); }, Qt::QueuedConnection);
    return;
  }
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

void Native::setPreferredAudioRoute(const QString& id) const
{
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kVoice, "setPreferredRoute", "(Ljava/lang/String;)V",
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

void Native::setMicTest(bool on)
{
  if (on == m_micTest)
    return;
  m_micTest = on;
  m_micTestRms = 0;
  if (m_micTestLevel != 0) {
    m_micTestLevel = 0;
    emit micTestLevelChanged();
  }
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kMicTest, on ? "start" : "stop");
#endif
  emit micTestChanged();
}

void Native::onMicLevel(double rms)
{
  if (!m_micTest)
    return;
  if (rms < 0) {
    setMicTest(false);
    emit micTestFailed();
    return;
  }
  // MicTestWidget's RMS_SMOOTHING, then getVoiceActivityLevelPercent
  // (yap-voice vad::level_percent): the position where dB + 100 sits.
  const double now = rms * 100;
  m_micTestRms = m_micTestRms > 0 ? m_micTestRms * 0.72 + now * 0.28 : now;
  const qreal level =
    m_micTestRms <= 0 ? 0 : std::clamp(20 * std::log10(m_micTestRms / 100) + 100, 0.0, 100.0);
  if (std::abs(level - m_micTestLevel) < 0.01)
    return;
  m_micTestLevel = level;
  emit micTestLevelChanged();
}

void Native::onAudioRoutes(const QByteArray& json)
{
  const QVariantList routes = QJsonDocument::fromJson(json).array().toVariantList();
  if (routes == m_audioRoutes)
    return;
  m_audioRoutes = routes;
  emit audioRoutesChanged();
}
