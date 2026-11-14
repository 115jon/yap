#pragma once

// A WebM player for what Media Foundation cannot open here (VP8/VP9 WebM:
// animated nameplates, profile media, webm attachments and links), with
// MediaPlayer's QML surface so EmbedVideo drives either one:
//
//   VideoOutput { id: out }
//   WebmPlayer { source: url; videoOutput: out; loops: MediaPlayer.Infinite }
//
// src/webm.rs runs a yap-webm player thread per source: fetch (with the
// session only for the API's private media), demux, libvpx decode, pace.
// Frames come back on that thread and are copied once into a QVideoFrame
// (I420 as videohub.cpp does for call video; alpha streams as premultiplied
// BGRA) and the newest one goes to the output's QVideoSink on the GUI
// thread; a GUI thread that falls behind skips frames.
//
// Qt 6.6's video node blends only below full opacity ("respect video
// formats with Alpha" is a TODO in qsgvideonode_p.cpp), so an output that
// shows a transparent stream sets `opacity: 0.99`.
//
// The thread decodes only while it should: playing, the window shown and
// not minimised, some of the output on screen (inside the window and every
// clipping ancestor: a scrolled list), and for `decorative` players the app
// active (the web's useAppActive pauses decorations behind a game or in the
// background). Otherwise it pauses and holds its frame.

#include <QtCore/QMutex>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QSize>
#include <QtMultimedia/QMediaPlayer>
#include <QtMultimedia/QVideoFrame>
#include <QtQml/QQmlParserStatus>
#include <QtQuick/QQuickItem>

#include <cstdint>

class QVideoSink;
class QQuickWindow;

class WebmPlayer : public QObject, public QQmlParserStatus
{
  Q_OBJECT
  Q_INTERFACES(QQmlParserStatus)
  Q_PROPERTY(QString source READ source WRITE setSource NOTIFY sourceChanged)
  Q_PROPERTY(QQuickItem* videoOutput READ videoOutput WRITE setVideoOutput NOTIFY videoOutputChanged)
  // MediaPlayer's: 1 plays once, MediaPlayer.Infinite (-1) loops.
  Q_PROPERTY(int loops READ loops WRITE setLoops NOTIFY loopsChanged)
  Q_PROPERTY(bool decorative READ decorative WRITE setDecorative NOTIFY decorativeChanged)
  Q_PROPERTY(QMediaPlayer::PlaybackState playbackState READ playbackState NOTIFY playbackStateChanged)
  Q_PROPERTY(QMediaPlayer::MediaStatus mediaStatus READ mediaStatus NOTIFY mediaStatusChanged)
  Q_PROPERTY(qint64 duration READ duration NOTIFY durationChanged)
  Q_PROPERTY(qint64 position READ position WRITE setPosition NOTIFY positionChanged)
  Q_PROPERTY(bool seekable READ seekable NOTIFY durationChanged)
  Q_PROPERTY(float bufferProgress READ bufferProgress NOTIFY mediaStatusChanged)
  Q_PROPERTY(bool hasAlpha READ hasAlpha NOTIFY durationChanged)
  Q_PROPERTY(QSize videoSize READ videoSize NOTIFY durationChanged)
  Q_PROPERTY(QString errorString READ errorString NOTIFY errorOccurred)

public:
  explicit WebmPlayer(QObject* parent = nullptr);
  ~WebmPlayer() override;

  QString source() const { return m_source; }
  void setSource(const QString& source);
  QQuickItem* videoOutput() const { return m_output; }
  void setVideoOutput(QQuickItem* output);
  int loops() const { return m_loops; }
  void setLoops(int loops);
  bool decorative() const { return m_decorative; }
  void setDecorative(bool decorative);
  QMediaPlayer::PlaybackState playbackState() const { return m_state; }
  QMediaPlayer::MediaStatus mediaStatus() const { return m_status; }
  qint64 duration() const { return m_duration; }
  qint64 position() const { return m_position; }
  void setPosition(qint64 position);
  bool seekable() const { return m_duration > 0; }
  float bufferProgress() const;
  bool hasAlpha() const { return m_alpha; }
  QSize videoSize() const { return m_size; }
  QString errorString() const { return m_error; }

  Q_INVOKABLE void play();
  Q_INVOKABLE void pause();
  Q_INVOKABLE void stop();

  void classBegin() override {}
  void componentComplete() override;

  // src/webm.rs callbacks (any thread).
  static void onFrame(uint64_t id, int kind, uint32_t width, uint32_t height,
                      const uint8_t* p0, uint32_t s0, const uint8_t* p1, uint32_t s1,
                      const uint8_t* p2, uint32_t s2, uint64_t pts);
  static void onEvent(uint64_t id, int kind, uint64_t duration, uint32_t width,
                      uint32_t height, bool alpha, const QString& text);

signals:
  void sourceChanged();
  void videoOutputChanged();
  void loopsChanged();
  void decorativeChanged();
  void playbackStateChanged();
  void mediaStatusChanged();
  void durationChanged();
  void positionChanged();
  void errorOccurred(QMediaPlayer::Error error, const QString& errorString);

private:
  void open();
  void close();
  QVideoSink* videoSink() const;
  void push(QVideoFrame frame, qint64 pts);
  void deliver();
  void handle(int kind, qint64 duration, QSize size, bool alpha, const QString& text);
  void setState(QMediaPlayer::PlaybackState state);
  void setStatus(QMediaPlayer::MediaStatus status);
  // Runs or holds the decoder for what is on screen now.
  void sync();
  bool shouldRun() const;
  bool onScreen() const;
  void watchWindow();
  QSize maxSize() const;

  QString m_source;
  QPointer<QQuickItem> m_output;
  QPointer<QQuickWindow> m_window;
  QMetaObject::Connection m_frameWatch;
  QMetaObject::Connection m_visibilityWatch;
  int m_loops = 1;
  bool m_decorative = false;
  bool m_complete = false;
  // What QML asked for; the decoder runs when this and shouldRun() agree.
  bool m_wanted = false;
  bool m_running = false;
  bool m_loaded = false;
  QMediaPlayer::PlaybackState m_state = QMediaPlayer::StoppedState;
  QMediaPlayer::MediaStatus m_status = QMediaPlayer::NoMedia;
  qint64 m_duration = 0;
  qint64 m_position = 0;
  bool m_alpha = false;
  QSize m_size;
  QString m_error;
  uint64_t m_id = 0;

  QMutex m_mutex;
  QVideoFrame m_latest;
  qint64 m_latestPts = 0;
  bool m_posted = false;
};
