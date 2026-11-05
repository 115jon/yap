#pragma once

// Remote call video into Qt Multimedia's VideoOutput.
//
//   VideoHub.attach(key, videoOutput.videoSink)   show `key`'s frames there
//   VideoHub.detach(key, videoOutput.videoSink)   stop
//
// `key` is a track alias (`cam-video-<participant>`). The Rust call hands
// every decoded frame to `yap_video_frame` on a media thread. A key nobody
// shows is dropped there, before any copy. Otherwise the I420 planes are
// copied once into a QVideoFrame (Format_YUV420P; VideoOutput converts it to
// RGB on the GPU) and kept as the key's latest frame; one queued call per
// key carries the newest frame to the GUI thread, where its sinks get it. A
// GUI thread that falls behind skips frames instead of queueing them.

#include <QtCore/QHash>
#include <QtCore/QList>
#include <QtCore/QMutex>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QString>
#include <QtMultimedia/QVideoFrame>
#include <QtMultimedia/QVideoSink>

#include <atomic>

class VideoFeed : public QObject
{
  Q_OBJECT

public:
  using QObject::QObject;

  // Any thread.
  bool wanted() const { return m_attached.load(std::memory_order_relaxed) > 0; }
  void push(QVideoFrame frame);

  // GUI thread.
  void attach(QVideoSink* sink);
  void detach(QVideoSink* sink);

private:
  void deliver();

  QMutex m_mutex;
  QVideoFrame m_latest;
  bool m_posted = false;
  std::atomic<int> m_attached{ 0 };
  QList<QPointer<QVideoSink>> m_sinks;
};

class VideoHub : public QObject
{
  Q_OBJECT

public:
  explicit VideoHub(QObject* parent = nullptr);
  static VideoHub* instance();

  Q_INVOKABLE void attach(const QString& key, QVideoSink* sink);
  Q_INVOKABLE void detach(const QString& key, QVideoSink* sink);

  // Any thread: the feed for `key` if something shows it.
  VideoFeed* wantedFeed(const QString& key);

private:
  QMutex m_mutex;
  // Feeds live as long as the hub (keys are few and short-lived calls
  // reuse none), so a media thread never sees one deleted.
  QHash<QString, VideoFeed*> m_feeds;
};
