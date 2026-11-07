#pragma once

// An animated picture Qt's image plugins cannot animate: APNG (avatar
// decorations, profile effects) and animated WebP (X GIFs). A still PNG or
// WebP works too, as one frame.
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

#include <QtCore/QHash>
#include <QtCore/QPointer>
#include <QtCore/QSet>
#include <QtCore/QTimer>
#include <QtCore/QUrl>
#include <QtGui/QImage>
#include <QtQuick/QQuickItem>

#include <memory>

class QNetworkAccessManager;

struct AnimationData
{
  QList<QImage> frames;
  QList<int> delays;
  // How many times it plays; 0 is forever.
  int loops = 0;
  qsizetype bytes = 0;
};
using AnimationPtr = std::shared_ptr<const AnimationData>;

// Decoded animations by URL and size. GUI thread only.
class AnimationCache : public QObject
{
  Q_OBJECT

public:
  static AnimationCache* instance();

  AnimationPtr find(const QString& key);
  bool failed(const QString& key) const { return m_failed.contains(key); }
  // Starts loading `url` for a `box`-sized key unless it is cached or on
  // its way; `loaded(key)` follows.
  void request(QNetworkAccessManager* network, const QUrl& url, QSize box,
               const QString& key);

signals:
  void loaded(const QString& key);

private:
  void insert(const QString& key, AnimationPtr data);
  void trim();

  QHash<QString, AnimationPtr> m_entries;
  // Least recently used first.
  QList<QString> m_order;
  QSet<QString> m_pending;
  QSet<QString> m_failed;
  qsizetype m_bytes = 0;
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

public:
  enum FillMode { PreserveAspectFit, PreserveAspectCrop, Stretch };
  Q_ENUM(FillMode)
  enum Status { Null, Loading, Ready, Error };
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
  int frameCount() const { return m_anim ? int(m_anim->frames.size()) : 0; }
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
  bool onScreen() const;
  bool shouldRun() const;
  void watchWindow(QQuickWindow* window);
  int playCount() const;

  QUrl m_source;
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
  QTimer m_timer;
  QPointer<QQuickWindow> m_window;
  QMetaObject::Connection m_frameWatch;
};
