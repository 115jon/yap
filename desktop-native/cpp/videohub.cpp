#include "videohub.h"

#include <QtCore/QByteArray>
#include <QtCore/QMetaObject>
#include <QtCore/QMutexLocker>
#include <QtMultimedia/QVideoFrameFormat>
#include <QtMultimedia/private/qhwvideobuffer_p.h>
#include <QtMultimedia/private/qvideoframe_p.h>
#include <QtQuick/QQuickGraphicsDevice>
#include <QtQuick/QQuickWindow>
#include <rhi/qrhi.h>

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iterator>
#include <memory>

#ifdef Q_OS_WIN
#include <d3d11.h>
#include <wrl/client.h>
using Microsoft::WRL::ComPtr;
#endif

// From the patched webrtc-sys (patches/webrtc-sys/0004, mf_h264_decoder.cpp;
// the struct's layout is repeated there).
struct YapD3D11Frame
{
  void* texture;  // ID3D11Texture2D*: NV12, on the shared device
  int width;
  int height;
  void* hold;
  void (*release)(void* hold);
};
extern "C" void yap_mf_set_shared_device(void* device);
extern "C" bool yap_d3d11_frame(const void* frame, YapD3D11Frame* out);

namespace {

VideoHub* g_hub = nullptr;
#ifdef Q_OS_WIN
// The device the window renders with and the decoder decodes on; never
// released (the window and the decoders use it until the process ends).
ID3D11Device* g_device = nullptr;

// One decoded picture as Qt sees it: an NV12 texture on our device, which
// VideoOutput samples as it is (two views of the one texture: R8 for luma,
// R8G8 for chroma). Holding it holds the decoder's frame, and with it the
// texture, out of the decoder's pool.
class D3D11FrameBuffer : public QHwVideoBuffer
{
public:
  explicit D3D11FrameBuffer(const YapD3D11Frame& frame)
    : QHwVideoBuffer(QVideoFrame::RhiTextureHandle)
    , m_frame(frame)
  {
  }
  ~D3D11FrameBuffer() override { m_frame.release(m_frame.hold); }

  QVideoFrameFormat format() const override
  {
    return QVideoFrameFormat(QSize(m_frame.width, m_frame.height), QVideoFrameFormat::Format_NV12);
  }

  quint64 textureHandle(QRhi& rhi, int plane) override
  {
    if (plane > 1 || rhi.backend() != QRhi::D3D11)
      return 0;
    const auto* native = static_cast<const QRhiD3D11NativeHandles*>(rhi.nativeHandles());
    if (!native || native->dev != g_device)
      return 0;
    return quint64(reinterpret_cast<quintptr>(m_frame.texture));
  }

  // Only for a consumer without our device (Qt then uploads): the pixels,
  // read back once through a staging copy.
  MapData map(QVideoFrame::MapMode mode) override
  {
    MapData data;
    if (mode != QVideoFrame::ReadOnly || !readBack())
      return data;
    data.planeCount = 2;
    data.bytesPerLine[0] = data.bytesPerLine[1] = m_pitch;
    data.data[0] = reinterpret_cast<uchar*>(m_pixels.data());
    data.data[1] = data.data[0] + qsizetype(m_pitch) * m_frame.height;
    data.dataSize[0] = m_pitch * m_frame.height;
    data.dataSize[1] = m_pitch * (m_frame.height / 2);
    return data;
  }

private:
  bool readBack()
  {
    if (!m_pixels.isEmpty())
      return true;
    auto* texture = static_cast<ID3D11Texture2D*>(m_frame.texture);
    D3D11_TEXTURE2D_DESC desc{};
    texture->GetDesc(&desc);
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    desc.MiscFlags = 0;
    ComPtr<ID3D11Device> device;
    texture->GetDevice(&device);
    ComPtr<ID3D11Texture2D> staging;
    if (FAILED(device->CreateTexture2D(&desc, nullptr, &staging)))
      return false;
    ComPtr<ID3D11DeviceContext> context;
    device->GetImmediateContext(&context);
    context->CopyResource(staging.Get(), texture);
    D3D11_MAPPED_SUBRESOURCE mapped{};
    if (FAILED(context->Map(staging.Get(), 0, D3D11_MAP_READ, 0, &mapped)))
      return false;
    m_pitch = int(mapped.RowPitch);
    const qsizetype rows = m_frame.height + m_frame.height / 2;
    m_pixels = QByteArray(static_cast<const char*>(mapped.pData), m_pitch * rows);
    context->Unmap(staging.Get(), 0);
    return true;
  }

  YapD3D11Frame m_frame;
  QByteArray m_pixels;
  int m_pitch = 0;
};
#endif

void copyPlane(uchar* dst, int dstStride, const uint8_t* src, uint32_t srcStride, int width, int rows)
{
  if (dstStride == static_cast<int>(srcStride)) {
    std::memcpy(dst, src, static_cast<size_t>(dstStride) * rows);
    return;
  }
  for (int row = 0; row < rows; ++row)
    std::memcpy(dst + static_cast<size_t>(row) * dstStride, src + static_cast<size_t>(row) * srcStride, width);
}

QVideoFrame::RotationAngle rotationAngle(uint16_t degrees)
{
  switch (degrees) {
    case 90:
      return QVideoFrame::Rotation90;
    case 180:
      return QVideoFrame::Rotation180;
    case 270:
      return QVideoFrame::Rotation270;
    default:
      return QVideoFrame::Rotation0;
  }
}

}  // namespace

void VideoFeed::push(QVideoFrame frame)
{
  bool post = false;
  {
    QMutexLocker lock(&m_mutex);
    m_latest = std::move(frame);
    post = !m_posted;
    m_posted = true;
  }
  if (post)
    QMetaObject::invokeMethod(this, &VideoFeed::deliver, Qt::QueuedConnection);
}

void VideoFeed::deliver()
{
  QVideoFrame frame;
  {
    QMutexLocker lock(&m_mutex);
    frame = std::move(m_latest);
    m_latest = QVideoFrame();
    m_posted = false;
  }
  for (const QPointer<QVideoSink>& sink : std::as_const(m_sinks)) {
    if (sink)
      sink->setVideoFrame(frame);
  }
}

void VideoFeed::attach(QVideoSink* sink)
{
  m_sinks.removeAll(nullptr);
  if (!sink || m_sinks.contains(sink))
    return;
  m_sinks.append(sink);
  m_attached.store(static_cast<int>(m_sinks.size()), std::memory_order_relaxed);
}

void VideoFeed::detach(QVideoSink* sink)
{
  m_sinks.removeAll(sink);
  m_sinks.removeAll(nullptr);
  m_attached.store(static_cast<int>(m_sinks.size()), std::memory_order_relaxed);
}

VideoHub::VideoHub(QObject* parent) : QObject(parent)
{
  g_hub = this;
}

VideoHub* VideoHub::instance()
{
  return g_hub;
}

void VideoHub::shareDevice(QQuickWindow* window)
{
#ifdef Q_OS_WIN
  if (!window || g_device || qEnvironmentVariable("YAP_ZERO_COPY") == QLatin1String("0") ||
      QQuickWindow::graphicsApi() != QSGRendererInterface::Direct3D11)
    return;
  // As Qt would make it (BGRA for its text and images, the first hardware
  // adapter), plus video support for the decoder; multithread protected,
  // since the decoder's thread and the GUI thread both use it.
  const D3D_FEATURE_LEVEL levels[] = { D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0 };
  ComPtr<ID3D11Device> device;
  ComPtr<ID3D11DeviceContext> context;
  if (FAILED(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr,
                               D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                               levels, UINT(std::size(levels)), D3D11_SDK_VERSION, &device,
                               nullptr, &context))) {
    std::fprintf(stderr, "[video] no D3D11 video device; decoded video is copied\n");
    return;
  }
  ComPtr<ID3D10Multithread> multithread;
  if (FAILED(context.As(&multithread))) {
    std::fprintf(stderr, "[video] no multithread protection; decoded video is copied\n");
    return;
  }
  multithread->SetMultithreadProtected(TRUE);
  window->setGraphicsDevice(QQuickGraphicsDevice::fromDeviceAndContext(device.Get(), context.Get()));
  // Both kept for the process's life: Qt does not take a reference.
  g_device = device.Detach();
  context.Detach();
  yap_mf_set_shared_device(g_device);
#else
  Q_UNUSED(window);
#endif
}

void VideoHub::attach(const QString& key, QVideoSink* sink)
{
  if (key.isEmpty() || !sink)
    return;
  VideoFeed* feed = nullptr;
  {
    QMutexLocker lock(&m_mutex);
    feed = m_feeds.value(key);
    if (!feed) {
      feed = new VideoFeed(this);
      m_feeds.insert(key, feed);
    }
  }
  feed->attach(sink);
}

void VideoHub::detach(const QString& key, QVideoSink* sink)
{
  QMutexLocker lock(&m_mutex);
  if (VideoFeed* feed = m_feeds.value(key))
    feed->detach(sink);
}

VideoFeed* VideoHub::wantedFeed(const QString& key)
{
  QMutexLocker lock(&m_mutex);
  VideoFeed* feed = m_feeds.value(key);
  return feed && feed->wanted() ? feed : nullptr;
}

// One remote frame still on the GPU (src/video.rs): `buffer` is the C++
// buffer behind it, valid for this call. True if it was taken (or nobody
// shows the key); false and Rust reads it back and calls yap_video_frame.
extern "C" bool yap_video_native_frame(const char* key, size_t keyLen, const void* buffer, uint16_t rotation)
{
#ifdef Q_OS_WIN
  VideoHub* hub = VideoHub::instance();
  if (!hub || !g_device)
    return false;
  VideoFeed* feed = hub->wantedFeed(QString::fromUtf8(key, static_cast<qsizetype>(keyLen)));
  if (!feed)
    return true;
  YapD3D11Frame native{};
  if (!yap_d3d11_frame(buffer, &native))
    return false;
  auto wrapped = std::make_unique<D3D11FrameBuffer>(native);
  QVideoFrameFormat format = wrapped->format();
  QVideoFrame frame = QVideoFramePrivate::createFrame(std::move(wrapped), std::move(format));
  frame.setRotation(QtVideo::Rotation(rotationAngle(rotation)));
  feed->push(std::move(frame));
  return true;
#else
  Q_UNUSED(key);
  Q_UNUSED(keyLen);
  Q_UNUSED(buffer);
  Q_UNUSED(rotation);
  return false;
#endif
}

// One decoded remote frame from the Rust call (src/video.rs). Planes are
// valid for this call only; they are copied before it returns.
extern "C" void yap_video_frame(const char* key,
                                size_t keyLen,
                                uint32_t width,
                                uint32_t height,
                                uint16_t rotation,
                                const uint8_t* y,
                                uint32_t strideY,
                                const uint8_t* u,
                                uint32_t strideU,
                                const uint8_t* v,
                                uint32_t strideV)
{
  VideoHub* hub = VideoHub::instance();
  if (!hub || width == 0 || height == 0 || width > 8192 || height > 8192)
    return;
  VideoFeed* feed = hub->wantedFeed(QString::fromUtf8(key, static_cast<qsizetype>(keyLen)));
  if (!feed)
    return;
  const int w = static_cast<int>(width);
  const int h = static_cast<int>(height);
  QVideoFrame frame(QVideoFrameFormat(QSize(w, h), QVideoFrameFormat::Format_YUV420P));
  if (!frame.map(QVideoFrame::WriteOnly))
    return;
  const int cw = (w + 1) / 2;
  const int ch = (h + 1) / 2;
  copyPlane(frame.bits(0), frame.bytesPerLine(0), y, strideY, w, h);
  copyPlane(frame.bits(1), frame.bytesPerLine(1), u, strideU, cw, ch);
  copyPlane(frame.bits(2), frame.bytesPerLine(2), v, strideV, cw, ch);
  frame.unmap();
  frame.setRotationAngle(rotationAngle(rotation));
  feed->push(std::move(frame));
}
