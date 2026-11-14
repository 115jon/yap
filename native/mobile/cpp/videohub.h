#pragma once

// Remote call video on the phone: the newest frame of each shown key, for
// the VideoView items (videoitem.h) that draw it.
//
// The Rust call (rust/src/video.rs) hands every routed frame to
// yap_mobile_video_frame on a media thread. A key no VideoView shows is
// dropped there, before any conversion. Otherwise the hub lends an RGBA
// buffer of the frame's size (one the renderer has let go of, when there is
// one), Rust writes the frame into it with libyuv, and it becomes the key's
// newest frame. One queued call per key then asks its views to repaint on
// the GUI thread; a GUI thread that falls behind skips frames instead of
// queueing them. The render thread uploads the newest frame when it draws.
//
// The hub also reports what the phone lets a call pull (rust/src/video.rs,
// yap_voice::demand Limits::phone): the device's thermal status and whether
// its network is metered, checked every 10 s while a view exists, and
// whether the app is in the background, as it changes.

#include <QtCore/QHash>
#include <QtCore/QList>
#include <QtCore/QMutex>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QString>
#include <QtCore/QTimer>
#include <QtGui/QImage>

#include <atomic>

class VideoView;

// The newest frame of one key, as it is drawn.
struct VideoFrame
{
  QImage image; // RGBA8888, unturned
  int rotation = 0; // clockwise degrees to turn it for display
  quint64 serial = 0; // 0: none yet
};

class VideoFeed : public QObject
{
  Q_OBJECT

public:
  using QObject::QObject;

  // Any thread.
  bool wanted() const { return m_attached.load(std::memory_order_relaxed) > 0; }
  // A buffer for a `size` frame: the spare one when the renderer let go of
  // it, else a new one.
  QImage borrow(QSize size);
  // `image` (from borrow) is the newest frame; the last one becomes the
  // spare.
  void publish(QImage image, int rotation);
  // The newest frame (the render thread, while it syncs).
  VideoFrame latest();

  // GUI thread.
  void attach(VideoView* view);
  void detach(VideoView* view);

private:
  void deliver();

  QMutex m_mutex;
  VideoFrame m_latest;
  QImage m_spare;
  bool m_posted = false;
  std::atomic<int> m_attached{ 0 };
  QList<QPointer<VideoView>> m_views;
};

class VideoHub : public QObject
{
  Q_OBJECT

public:
  static VideoHub* instance();

  // GUI thread: `view` shows `key` (an empty key: nothing).
  void attach(const QString& key, VideoView* view);
  void detach(const QString& key, VideoView* view);

  // Any thread: the feed for `key` if something shows it.
  VideoFeed* wantedFeed(const QString& key);

private:
  explicit VideoHub(QObject* parent = nullptr);
  // The device's heat, its network's metering and the app's state, to the
  // call's video limits when one changed.
  void reportLimits();

  QMutex m_mutex;
  // Feeds live as long as the hub (keys are few), so a media thread never
  // sees one deleted.
  QHash<QString, VideoFeed*> m_feeds;
  int m_views = 0;
  QTimer m_poll;
  int m_reported[3] = { -2, -2, -2 };
};
