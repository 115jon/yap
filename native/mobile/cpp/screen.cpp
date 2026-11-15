// Native's screen: the phone's screen share in a call. The system's
// consent dialog (MediaProjectionManager's intent, started for its result
// here) answers on the shim's side
// (android/src/dev/yap/qtproto/voice/ScreenShim.java), which keeps a yes
// for the next capture. The call starts and stops the capture from its own
// threads (rust/src/screen.rs: yap_mobile_screen_start and friends, below);
// libwebrtc's ScreenCapturerAndroid captures, each frame is read back as
// I420 on its thread and crosses here straight into the core, and the
// device's sound (AudioPlaybackCapture) does from the shim's audio thread.
// Android's own Stop and the call notification's Stop sharing go to the
// core without Qt's thread.

#include "native.h"

#include "videohub.h"

#include <QtCore/QMetaObject>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <iterator>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniEnvironment>
#include <QtCore/QJniObject>
#include <QtCore/private/qandroidextras_p.h>
#include <QtCore/qnativeinterface.h>

extern "C" {
bool yap_mobile_screen_frame(const uint8_t* y, size_t yLen, uint32_t strideY, const uint8_t* u, size_t uLen,
                             uint32_t strideU, const uint8_t* v, size_t vLen, uint32_t strideV, uint32_t width,
                             uint32_t height);
void yap_mobile_screen_audio(const int16_t* samples, size_t count);
void yap_mobile_screen_ended(int64_t capture);
void yap_mobile_share_stop();
}

namespace {

constexpr char kScreen[] = "dev/yap/qtproto/voice/ScreenShim";
// Qt keeps one result callback per request code: clear of the pickers'.
constexpr int kScreenConsent = 0x5962;

// A direct buffer's bytes; null and 0 for anything else.
const uint8_t* bytes(JNIEnv* env, jobject buffer, size_t* len)
{
  *len = 0;
  if (!buffer)
    return nullptr;
  void* data = env->GetDirectBufferAddress(buffer);
  const jlong capacity = env->GetDirectBufferCapacity(buffer);
  if (!data || capacity <= 0)
    return nullptr;
  *len = size_t(capacity);
  return static_cast<const uint8_t*>(data);
}

// The capture's thread, once per frame sent; the buffers are the shim's
// I420 frame, released after this returns.
jboolean jniFrame(JNIEnv* env, jclass, jobject y, jint strideY, jobject u, jint strideU, jobject v, jint strideV,
                  jint width, jint height)
{
  if (width <= 0 || height <= 0 || strideY <= 0 || strideU <= 0 || strideV <= 0)
    return JNI_FALSE;
  size_t yLen = 0;
  size_t uLen = 0;
  size_t vLen = 0;
  const uint8_t* py = bytes(env, y, &yLen);
  const uint8_t* pu = bytes(env, u, &uLen);
  const uint8_t* pv = bytes(env, v, &vLen);
  if (!py || !pu || !pv)
    return JNI_FALSE;
  return yap_mobile_screen_frame(py, yLen, uint32_t(strideY), pu, uLen, uint32_t(strideU), pv, vLen,
                                 uint32_t(strideV), uint32_t(width), uint32_t(height))
           ? JNI_TRUE
           : JNI_FALSE;
}

// The shim's audio thread: `length` bytes of interleaved 16-bit samples.
void jniAudio(JNIEnv* env, jclass, jobject pcm, jint length)
{
  size_t len = 0;
  const uint8_t* data = bytes(env, pcm, &len);
  if (!data || length <= 0)
    return;
  len = std::min(len, size_t(length));
  yap_mobile_screen_audio(reinterpret_cast<const int16_t*>(data), len / sizeof(int16_t));
}

// The shim's control thread: Android ended capture `capture`.
void jniEnded(JNIEnv*, jclass, jlong capture)
{
  yap_mobile_screen_ended(int64_t(capture));
}

// Android's main thread: the call notification's Stop sharing.
void jniStop(JNIEnv*, jclass)
{
  yap_mobile_share_stop();
}

QJniObject context()
{
  return QNativeInterface::QAndroidApplication::context();
}

} // namespace

// From the core (rust/src/screen.rs), on a blocking thread of its own:
// starts the capture (up to a few seconds: the service's type changes on
// Android's main thread first). QJniObject attaches the thread and finds
// the class through Qt's class loader.
extern "C" int64_t yap_mobile_screen_start(uint32_t longSide, uint32_t shortSide, uint32_t fps, bool audio)
{
  return int64_t(QJniObject::callStaticMethod<jlong>(kScreen, "start", "(Landroid/content/Context;IIIZ)J",
                                                     context().object(), jint(longSide), jint(shortSide),
                                                     jint(fps), jboolean(audio)));
}

extern "C" void yap_mobile_screen_stop(int64_t capture)
{
  QJniObject::callStaticMethod<void>(kScreen, "stop", "(J)V", jlong(capture));
}

extern "C" void yap_mobile_screen_format(int64_t capture, uint32_t longSide, uint32_t shortSide, uint32_t fps)
{
  QJniObject::callStaticMethod<void>(kScreen, "format", "(JIII)V", jlong(capture), jint(longSide),
                                     jint(shortSide), jint(fps));
}
#endif

void Native::installScreen()
{
#ifdef Q_OS_ANDROID
  const JNINativeMethod methods[] = {
    { "nativeFrame", "(Ljava/nio/ByteBuffer;ILjava/nio/ByteBuffer;ILjava/nio/ByteBuffer;III)Z",
      reinterpret_cast<void*>(jniFrame) },
    { "nativeAudio", "(Ljava/nio/ByteBuffer;I)V", reinterpret_cast<void*>(jniAudio) },
    { "nativeEnded", "(J)V", reinterpret_cast<void*>(jniEnded) },
    { "nativeStop", "()V", reinterpret_cast<void*>(jniStop) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kScreen, methods, std::size(methods)))
    qWarning("[share] could not register the shim's callbacks");
#endif
}

void Native::setScreenCapture(bool on)
{
  if (on == m_screenCapture)
    return;
  m_screenCapture = on;
  VideoHub::instance()->setCapturing(VideoHub::Screen, on);
  emit screenCaptureChanged();
}

bool Native::shareAudioSupported() const
{
#ifdef Q_OS_ANDROID
  return QJniObject::callStaticMethod<jboolean>(kScreen, "audioSupported", "(Landroid/content/Context;)Z",
                                                context().object());
#else
  return false;
#endif
}

void Native::requestScreenShare()
{
#ifdef Q_OS_ANDROID
  if (m_askingScreen)
    return;
  const QJniObject intent = QJniObject::callStaticObjectMethod(
    kScreen, "consentIntent", "(Landroid/content/Context;)Landroid/content/Intent;", context().object());
  if (!intent.isValid()) {
    QMetaObject::invokeMethod(this, [this] { emit screenShareAnswered(false); }, Qt::QueuedConnection);
    return;
  }
  m_askingScreen = true;
  // The answer comes back on Android's UI thread; the shim keeps a yes.
  QtAndroidPrivate::startActivity(intent, kScreenConsent, [](int, int resultCode, const QJniObject& data) {
    const bool granted = QJniObject::callStaticMethod<jboolean>(kScreen, "consented", "(ILandroid/content/Intent;)Z",
                                                                jint(resultCode), data.object());
    if (Native* native = Native::instance()) {
      QMetaObject::invokeMethod(
        native, [native, granted] { native->onScreenShareAnswered(granted); }, Qt::QueuedConnection);
    }
  });
#else
  QMetaObject::invokeMethod(this, [this] { emit screenShareAnswered(false); }, Qt::QueuedConnection);
#endif
}

void Native::onScreenShareAnswered(bool granted)
{
  m_askingScreen = false;
  emit screenShareAnswered(granted);
}
