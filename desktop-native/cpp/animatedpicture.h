#pragma once

// An animated picture Qt's image plugins cannot animate: APNG (avatar
// decorations, profile effects) and animated WebP (X GIFs). A still PNG or
// WebP works too, as one frame, and so does anything else Qt reads (a JPEG
// poster). A small GIF (a server icon) plays here too, decoded by Qt's
// reader, so it shares the frame grid below. Network GIFs that must loop
// use it (`stream`) instead of AnimatedImage: Qt 6.11's AnimatedImage plays
// a network GIF from its QNetworkReply, which cannot rewind, so it stops
// after one pass. A large APNG or WebP shows its first frame while the rest
// decode.
//
//   AnimatedPicture { source: url; playing: hovered; fillMode: ... }
//
// The bytes come through the QML engine's network stack (its disk cache),
// the Rust side (src/anim.rs, yap-anim) decodes every frame on a worker
// thread at the size the item is drawn (never larger than the file), and a
// shared cache keeps decoded animations by URL and size, so every row
// wearing the same decoration shares one copy.
//
// Frames advance only while `playing` is set, the app is active (not behind
// a game or minimised; the web's useAppActive) and some of the item is on
// screen: inside the window and every clipping ancestor (a scrolled list).
// Otherwise the timer stops; nothing ticks for a still picture.
//
// Frame changes land on a shared 30 Hz grid, so every picture due in the
// same slot advances in the same pass and they share one window render
// (each render redraws the whole window). A picture keeps its speed: a slot
// that comes after several of its frames skips to the one due by then.

#include <QtCore/QHash>
#include <QtCore/QPointer>
#include <QtCore/QSet>
#include <QtCore/QTimer>
#include <QtCore/QUrl>
#include <QtGui/QImage>
#include <QtQuick/QQuickItem>

#include <memory>

class QBuffer;
class QImageReader;
class QNetworkAccessManager;

struct AnimationData
{
  QList<QImage> frames;
  QList<int> delays;
  // How many times it plays; 0 is forever.
  int loops = 0;
  qsizetype bytes = 0;
  // A streamed GIF (`AnimatedPicture.stream`): the file itself, its size
  // and frame count; `frames` stays empty and each item decodes its own
  // frame as it plays.
  QByteArray raw;
  QSize rawSize;
  int rawFrames = 0;
};
using AnimationPtr = std::shared_ptr<const AnimationData>;

// Decoded animations by URL and size. GUI thread only.
class AnimationCache : public QObject
{
  Q_OBJECT

public:
  static AnimationCache* instance();

  AnimationPtr find(const QString& key);
  // A load of `key` failed lately. A failure is tried again once it is a
  // while old: one dropped request (the network not up yet) left a
  // picture blank until the app restarted.
  bool failed(const QString& key) const;
  // Starts loading `url` for a `box`-sized key unless it is cached or on
  // its way; `loaded(key)` follows.
  // `cover`: decode to cover the box (a cropped fill), not to fit it.
  // `raw`: a GIF is kept as its file (a streamed item decodes it), anything
  // else decodes as usual.
  void request(QNetworkAccessManager* network, const QUrl& url, QSize box,
               const QString& key, bool cover = false, bool raw = false);

signals:
  void loaded(const QString& key);

private:
  void insert(const QString& key, AnimationPtr data);
  void trim();

  QHash<QString, AnimationPtr> m_entries;
  // Least recently used first.
  QList<QString> m_order;
  QSet<QString> m_pending;
  // When each failed (ms since the epoch).
  QHash<QString, qint64> m_failed;
  qsizetype m_bytes = 0;
  // The part of m_bytes that is streamed GIF files.
  qsizetype m_rawBytes = 0;
};

// Whether `item` shows any part of itself: inside its window and every
// clipping ancestor (a scrolled list's viewport).
bool visibleInWindow(const QQuickItem* item);

// Says whether the item it fills is on screen, for animations Qt plays
// itself (AnimatedImage): a row in a list's cache buffer is visible but
// off screen, and every frame it plays still costs the window a render.
//
//   ViewportWatch { id: seen; anchors.fill: parent }
//   AnimatedImage { playing: seen.inView && ... }
//
// Checked after each frame the window renders (scrolling renders frames).
class ViewportWatch : public QQuickItem
{
  Q_OBJECT
  Q_PROPERTY(bool inView READ inView NOTIFY inViewChanged)

public:
  explicit ViewportWatch(QQuickItem* parent = nullptr);
  bool inView() const { return m_inView; }

signals:
  void inViewChanged();

protected:
  void itemChange(ItemChange change, const ItemChangeData& value) override;
  void geometryChange(const QRectF& next, const QRectF& previous) override;

private:
  void check();

  bool m_inView = false;
  QMetaObject::Connection m_frames;
};

class AnimatedPicture : public QQuickItem
{
  Q_OBJECT
  Q_PROPERTY(QUrl source READ source WRITE setSource NOTIFY sourceChanged)
  Q_PROPERTY(bool playing READ playing WRITE setPlaying NOTIFY playingChanged)
  Q_PROPERTY(FillMode fillMode READ fillMode WRITE setFillMode NOTIFY fillModeChanged)
  // Override the file's play count: 0 forever, -1 (default) the file's own.
  Q_PROPERTY(int loops READ loops WRITE setLoops NOTIFY loopsChanged)
  Q_PROPERTY(Status status READ status NOTIFY statusChanged)
  Q_PROPERTY(int frameCount READ frameCount NOTIFY statusChanged)
  // The last loop ended (a play-once effect rests on its last frame).
  Q_PROPERTY(bool finished READ finished NOTIFY finishedChanged)
  // A GIF is decoded one frame at a time as it plays (chat GIFs): the item
  // holds the file and the frame on screen, never every frame. Frames land
  // on the shared grid like the others. Other formats decode as usual.
  Q_PROPERTY(bool stream READ stream WRITE setStream NOTIFY streamChanged)

public:
  enum FillMode { PreserveAspectFit, PreserveAspectCrop, Stretch };
  Q_ENUM(FillMode)
  // Image.Status's order, so QML can compare either with Image.Ready.
  enum Status { Null, Ready, Loading, Error };
  Q_ENUM(Status)

  explicit AnimatedPicture(QQuickItem* parent = nullptr);
  ~AnimatedPicture() override;

  QUrl source() const { return m_source; }
  void setSource(const QUrl& source);
  bool playing() const { return m_playing; }
  void setPlaying(bool playing);
  FillMode fillMode() const { return m_fillMode; }
  void setFillMode(FillMode mode);
  int loops() const { return m_loops; }
  void setLoops(int loops);
  Status status() const { return m_status; }
  int frameCount() const;
  bool stream() const { return m_stream; }
  void setStream(bool stream);
  bool finished() const { return m_finished; }

  // Back to the first frame and play again (a replayed effect).
  Q_INVOKABLE void restart();
  // Diagnostics (YAP_DEBUG_ANIMATIONS): the frame timer is running.
  bool ticking() const { return m_timer.isActive(); }

signals:
  void sourceChanged();
  void playingChanged();
  void fillModeChanged();
  void loopsChanged();
  void statusChanged();
  void finishedChanged();
  void streamChanged();

protected:
  QSGNode* updatePaintNode(QSGNode* old, UpdatePaintNodeData*) override;
  void geometryChange(const QRectF& next, const QRectF& previous) override;
  void itemChange(ItemChange change, const ItemChangeData& value) override;
  void componentComplete() override;
  void releaseResources() override;

private:
  void load();
  void release();
  void onLoaded(const QString& key);
  void setStatus(Status status);
  void schedule();
  void advance();
  void startTimer(qint64 now);
  bool onScreen() const;
  bool shouldRun() const;
  void watchWindow(QQuickWindow* window);
  int playCount() const;
  int delayOf(int frame) const;
  // Streamed: the next frame into m_current; false at the end of the file.
  bool decodeNext();
  bool openStream();
  // What shows is `m_source` (at another size, or its first frame).
  bool sameSourceShown() const;

  QUrl m_source;
  // The source `m_anim` was loaded for.
  QUrl m_animSource;
  bool m_playing = false;
  FillMode m_fillMode = PreserveAspectFit;
  int m_loops = -1;
  Status m_status = Null;
  bool m_finished = false;
  QString m_key;
  AnimationPtr m_anim;
  int m_frame = 0;
  int m_played = 0;
  // A frame change waits for the render thread to pick it up.
  bool m_frameDirty = false;
  // When the shown frame ends, on the shared animation clock (0: not
  // running).
  qint64 m_due = 0;
  QTimer m_timer;
  QPointer<QQuickWindow> m_window;
  bool m_stream = false;
  // Streamed: the reader over m_anim->raw, the frame it read last and that
  // frame's delay.
  std::unique_ptr<QBuffer> m_buffer;
  std::unique_ptr<QImageReader> m_reader;
  QImage m_current;
  int m_currentDelay = 100;
  QMetaObject::Connection m_frameWatch;
};
