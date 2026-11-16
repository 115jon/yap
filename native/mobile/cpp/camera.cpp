// Native's camera: the phone's own camera in a call. The Android shim
// (android/src/dev/yap/qtproto/voice/CameraShim.java) runs libwebrtc's
// Camera2Capturer and reads each frame back as I420 on its camera thread;
// the planes cross here straight into the core (rust/src/camera.rs,
// yap_mobile_camera_frame), which turns them upright, converts them once
// for the call and shows them in the self-view. What the capture does
// (started, flipped, failed) comes back to the QML on Qt's thread
// (CameraLink.qml). The camera permission is asked as the microphone's is.

#include "native.h"

#include "videohub.h"

#include <QtCore/QMetaObject>

#include <cstddef>
#include <cstdint>

#ifdef Q_OS_ANDROID
#include <QtCore/QFuture>
#include <QtCore/QJniEnvironment>
#include <QtCore/QJniObject>
#include <QtCore/private/qandroidextras_p.h>
#include <QtCore/qnativeinterface.h>
#endif

#ifdef Q_OS_ANDROID
extern "C" bool yap_mobile_camera_frame(const uint8_t* y, size_t yLen, uint32_t strideY, const uint8_t* u,
                                        size_t uLen, uint32_t strideU, const uint8_t* v, size_t vLen,
                                        uint32_t strideV, uint32_t width, uint32_t height, uint16_t rotation);

namespace {

constexpr char kCamera[] = "dev/yap/qtproto/voice/CameraShim";

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

// The camera thread, once per frame; the buffers are the shim's I420 frame,
// released after this returns.
jboolean jniFrame(JNIEnv* env, jclass, jobject y, jint strideY, jobject u, jint strideU, jobject v, jint strideV,
                  jint width, jint height, jint rotation)
{
  if (width <= 0 || height <= 0 || strideY <= 0 || strideU <= 0 || strideV <= 0 || rotation < 0)
    return JNI_FALSE;
  size_t yLen = 0;
  size_t uLen = 0;
  size_t vLen = 0;
  const uint8_t* py = bytes(env, y, &yLen);
  const uint8_t* pu = bytes(env, u, &uLen);
  const uint8_t* pv = bytes(env, v, &vLen);
  if (!py || !pu || !pv)
    return JNI_FALSE;
  return yap_mobile_camera_frame(py, yLen, uint32_t(strideY), pu, uLen, uint32_t(strideU), pv, vLen,
                                 uint32_t(strideV), uint32_t(width), uint32_t(height), uint16_t(rotation % 360))
           ? JNI_TRUE
           : JNI_FALSE;
}

// The shim's main thread: the capture started (with the camera it uses),
// switched cameras, stopped, or failed.
void jniStarted(JNIEnv*, jclass, jboolean front)
{
  if (Native* native = Native::instance()) {
    const bool isFront = front == JNI_TRUE;
    QMetaObject::invokeMethod(
      native, [native, isFront] { native->onCameraStarted(isFront); }, Qt::QueuedConnection);
  }
}

void jniFailed(JNIEnv*, jclass, jstring reason)
{
  const QString why = reason ? QJniObject(reason).toString() : QStringLiteral("unknown");
  if (Native* native = Native::instance()) {
    QMetaObject::invokeMethod(
      native, [native, why] { native->onCameraFailed(why); }, Qt::QueuedConnection);
  }
}

// Android's main thread: the screen went on or off.
void jniScreen(JNIEnv*, jclass, jboolean on)
{
  if (Native* native = Native::instance()) {
    const bool isOn = on == JNI_TRUE;
    QMetaObject::invokeMethod(
      native, [native, isOn] { native->onScreen(isOn); }, Qt::QueuedConnection);
  }
}

} // namespace
#endif

void Native::installCamera()
{
#ifdef Q_OS_ANDROID
  const JNINativeMethod methods[] = {
    { "nativeFrame", "(Ljava/nio/ByteBuffer;ILjava/nio/ByteBuffer;ILjava/nio/ByteBuffer;IIII)Z",
      reinterpret_cast<void*>(jniFrame) },
    { "nativeStarted", "(Z)V", reinterpret_cast<void*>(jniStarted) },
    { "nativeFailed", "(Ljava/lang/String;)V", reinterpret_cast<void*>(jniFailed) },
    { "nativeScreen", "(Z)V", reinterpret_cast<void*>(jniScreen) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kCamera, methods, std::size(methods))) {
    qWarning("[camera] could not register the shim's callbacks");
    return;
  }
  QJniObject::callStaticMethod<void>(kCamera, "install", "(Landroid/content/Context;)V",
                                     QNativeInterface::QAndroidApplication::context().object());
#endif
}

void Native::setCameraCapture(bool on)
{
  if (on == m_cameraCapture)
    return;
  m_cameraCapture = on;
  // The heat is read while the camera runs, a self-view or not.
  VideoHub::instance()->setCapturing(VideoHub::Camera, on);
#ifdef Q_OS_ANDROID
  if (on) {
    QJniObject::callStaticMethod<void>(kCamera, "start", "(Landroid/content/Context;Z)V",
                                       QNativeInterface::QAndroidApplication::context().object(),
                                       jboolean(m_cameraFront));
  } else {
    QJniObject::callStaticMethod<void>(kCamera, "stop");
  }
#endif
  emit cameraCaptureChanged();
}

void Native::setCameraFacing(bool front)
{
  if (front == m_cameraFront)
    return;
  if (m_cameraCapture) {
    // The capture says which camera it took (onCameraStarted).
    flipCamera();
    return;
  }
  m_cameraFront = front;
  emit cameraFrontChanged();
}

void Native::flipCamera()
{
#ifdef Q_OS_ANDROID
  if (m_cameraCapture)
    QJniObject::callStaticMethod<void>(kCamera, "flip");
#endif
}

bool Native::cameraAllowed() const
{
#ifdef Q_OS_ANDROID
  return QJniObject::callStaticMethod<jboolean>(kCamera, "allowed", "(Landroid/content/Context;)Z",
                                                QNativeInterface::QAndroidApplication::context().object());
#else
  return false;
#endif
}

void Native::requestCamera()
{
#ifdef Q_OS_ANDROID
  if (m_askingCamera)
    return;
  // As the microphone: answered without asking when already allowed.
  if (cameraAllowed()) {
    QMetaObject::invokeMethod(this, [this] { emit cameraAnswered(true); }, Qt::QueuedConnection);
    return;
  }
  m_askingCamera = true;
  QtAndroidPrivate::requestPermission(QStringLiteral("android.permission.CAMERA"))
    .then(this, [this](QtAndroidPrivate::PermissionResult result) {
      m_askingCamera = false;
      emit cameraAnswered(result == QtAndroidPrivate::Authorized);
    });
#else
  QMetaObject::invokeMethod(this, [this] { emit cameraAnswered(false); }, Qt::QueuedConnection);
#endif
}

void Native::onCameraStarted(bool front)
{
  if (front != m_cameraFront) {
    m_cameraFront = front;
    emit cameraFrontChanged();
  }
}

void Native::onCameraFailed(const QString& reason)
{
  emit cameraFailed(reason);
}

void Native::onScreen(bool on)
{
  if (on == m_screenOn)
    return;
  m_screenOn = on;
  emit screenOnChanged();
}

void Native::setThermalStatus(int status)
{
  if (status == m_thermalStatus)
    return;
  m_thermalStatus = status;
  emit thermalStatusChanged();
}
