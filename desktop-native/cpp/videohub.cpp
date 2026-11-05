#include "videohub.h"

#include <QtCore/QMetaObject>
#include <QtCore/QMutexLocker>
#include <QtMultimedia/QVideoFrameFormat>

#include <cstdint>
#include <cstring>

namespace {

VideoHub* g_hub = nullptr;

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
