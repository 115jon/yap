// The phone's video hub (videohub.h).

#include "videohub.h"

#include "videoitem.h"

#include <QtCore/QMetaObject>
#include <QtCore/QMutexLocker>
#include <QtGui/QGuiApplication>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniObject>
#include <QtCore/qcoreapplication_platform.h>

extern "C" void yap_mobile_video_limits(int thermal, bool metered, bool background);
#endif

#include <cstring>
#include <utility>

QImage VideoFeed::borrow(QSize size)
{
  {
    QMutexLocker lock(&m_mutex);
    QImage spare = std::exchange(m_spare, QImage());
    // Only one nobody else holds (the renderer drops its copy once it
    // uploaded it): writing into a shared one would copy it first.
    if (spare.size() == size && spare.isDetached())
      return spare;
  }
  return QImage(size, QImage::Format_RGBA8888_Premultiplied);
}

void VideoFeed::publish(QImage image, int rotation)
{
  {
    QMutexLocker lock(&m_mutex);
    m_spare = std::exchange(m_latest.image, std::move(image));
    m_latest.rotation = rotation;
    ++m_latest.serial;
    if (m_posted)
      return;
    m_posted = true;
  }
  QMetaObject::invokeMethod(this, &VideoFeed::deliver, Qt::QueuedConnection);
}

VideoFrame VideoFeed::latest()
{
  QMutexLocker lock(&m_mutex);
  return m_latest;
}

void VideoFeed::attach(VideoView* view)
{
  m_views.append(view);
  m_attached.fetch_add(1, std::memory_order_relaxed);
}

void VideoFeed::detach(VideoView* view)
{
  if (m_views.removeOne(view))
    m_attached.fetch_sub(1, std::memory_order_relaxed);
  if (m_attached.load(std::memory_order_relaxed) > 0)
    return;
  // Nobody shows the key: its frames go (the next one starts afresh).
  QMutexLocker lock(&m_mutex);
  m_latest.image = QImage();
  m_latest.serial = 0;
  m_spare = QImage();
}

void VideoFeed::deliver()
{
  QSize shown;
  {
    QMutexLocker lock(&m_mutex);
    m_posted = false;
    if (m_latest.serial == 0)
      return;
    shown = m_latest.image.size();
    if (m_latest.rotation == 90 || m_latest.rotation == 270)
      shown.transpose();
  }
  for (const QPointer<VideoView>& view : std::as_const(m_views)) {
    if (view)
      view->frameArrived(shown);
  }
}

VideoHub::VideoHub(QObject* parent)
  : QObject(parent)
{
  m_poll.setInterval(10000);
  connect(&m_poll, &QTimer::timeout, this, &VideoHub::reportLimits);
  connect(qGuiApp, &QGuiApplication::applicationStateChanged, this, &VideoHub::reportLimits);
}

VideoHub* VideoHub::instance()
{
  static VideoHub* hub = new VideoHub(qGuiApp);
  return hub;
}

void VideoHub::attach(const QString& key, VideoView* view)
{
  if (key.isEmpty())
    return;
  VideoFeed* feed;
  {
    QMutexLocker lock(&m_mutex);
    feed = m_feeds.value(key);
    if (!feed) {
      feed = new VideoFeed(this);
      m_feeds.insert(key, feed);
    }
  }
  feed->attach(view);
  if (m_views++ == 0) {
    reportLimits();
    m_poll.start();
  }
}

void VideoHub::detach(const QString& key, VideoView* view)
{
  if (key.isEmpty())
    return;
  VideoFeed* feed;
  {
    QMutexLocker lock(&m_mutex);
    feed = m_feeds.value(key);
  }
  if (!feed)
    return;
  feed->detach(view);
  if (--m_views == 0)
    m_poll.stop();
}

VideoFeed* VideoHub::wantedFeed(const QString& key)
{
  QMutexLocker lock(&m_mutex);
  VideoFeed* feed = m_feeds.value(key);
  return feed && feed->wanted() ? feed : nullptr;
}

void VideoHub::reportLimits()
{
#ifdef Q_OS_ANDROID
  const bool background = qGuiApp->applicationState() != Qt::ApplicationActive;
  int thermal = -1;
  bool metered = false;
  // Only asked while something shows video (the poll runs then); the app
  // state is reported whenever it changes.
  if (m_views > 0) {
    const QJniObject context = QNativeInterface::QAndroidApplication::context();
    // PowerManager.getCurrentThermalStatus is API 29.
    if (QNativeInterface::QAndroidApplication::sdkVersion() >= 29) {
      const QJniObject power = context.callObjectMethod(
        "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", QJniObject::fromString(QStringLiteral("power")).object<jstring>());
      if (power.isValid())
        thermal = power.callMethod<jint>("getCurrentThermalStatus");
    }
    const QJniObject connectivity = context.callObjectMethod(
      "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", QJniObject::fromString(QStringLiteral("connectivity")).object<jstring>());
    if (connectivity.isValid())
      metered = connectivity.callMethod<jboolean>("isActiveNetworkMetered");
  } else {
    thermal = m_reported[0];
    metered = m_reported[1] == 1;
  }
  const int now[3] = { thermal, metered ? 1 : 0, background ? 1 : 0 };
  if (std::memcmp(now, m_reported, sizeof now) == 0)
    return;
  std::memcpy(m_reported, now, sizeof now);
  yap_mobile_video_limits(thermal, metered, background);
#endif
}

// From the Rust call (rust/src/video.rs), on a media thread: write the frame
// into a buffer the hub lends, if anything shows `key`.
extern "C" void yap_mobile_video_frame(const char* key, size_t keyLen, uint32_t width, uint32_t height,
                                       uint16_t rotation,
                                       bool (*fill)(void* context, uint8_t* pixels, size_t len, uint32_t stride),
                                       void* context)
{
  if (width == 0 || height == 0 || width > 8192 || height > 8192)
    return;
  VideoFeed* feed = VideoHub::instance()->wantedFeed(QString::fromUtf8(key, qsizetype(keyLen)));
  if (!feed)
    return;
  QImage image = feed->borrow(QSize(int(width), int(height)));
  if (image.isNull())
    return;
  if (!fill(context, image.bits(), size_t(image.sizeInBytes()), uint32_t(image.bytesPerLine())))
    return;
  feed->publish(std::move(image), rotation);
}
