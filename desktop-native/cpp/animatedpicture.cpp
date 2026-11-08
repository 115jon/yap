#include "animatedpicture.h"

#include <QtCore/QBuffer>
#include <QtCore/QElapsedTimer>
#include <QtCore/QThreadPool>
#include <QtGui/QGuiApplication>
#include <QtGui/QImageReader>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkReply>
#include <QtNetwork/QNetworkRequest>
#include <QtQml/QQmlEngine>
#include <QtQuick/QQuickWindow>
#include <QtQuick/QSGImageNode>
#include <QtQuick/QSGTexture>
#include <QtQuick/private/qsgplaintexture_p.h>

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <vector>

// src/anim.rs
struct YapAnimInfo
{
  uint32_t width;
  uint32_t height;
  uint32_t frames;
  uint32_t loops;
};
extern "C" void* yap_anim_decode(const uint8_t* data, size_t len, uint32_t max_w,
                                 uint32_t max_h);
extern "C" void yap_anim_info(const void* anim, YapAnimInfo* out);
extern "C" const uint8_t* yap_anim_frame(const void* anim, uint32_t index,
                                         uint32_t* delay_ms);
extern "C" void yap_anim_release_frame(void* anim, uint32_t index);
extern "C" void yap_anim_free(void* anim);

namespace {

// Decoded frames kept for pictures no item shows any more (hidden items
// hand theirs back, so this is what browsing leaves behind): enough for
// the decorations of a few screens, not for a history of GIFs.
constexpr qsizetype kCacheBudget = 32 * 1024 * 1024;
// Streamed GIFs' files (chat GIFs): a few screens' worth. A file is cheap
// to read again from the disk cache, and chat brings new ones all the time
// (a long soak filled the whole budget above with them, ~30 MB).
constexpr qsizetype kRawBudget = 4 * 1024 * 1024;
// A picture larger than this is not an avatar decoration or an effect.
constexpr qint64 kMaxDownload = 24 * 1024 * 1024;
// One animation's decoded frames (yap-anim stops at the same size).
constexpr qsizetype kMaxAnimation = 48 * 1024 * 1024;
// A file this large shows its first frame before the whole decodes.
constexpr qsizetype kPreviewBytes = 64 * 1024;

// A GIF (server icons): Qt's own decoder, every frame, fitted into `box`
// like yap-anim's (never scaled up), so it plays on the shared grid.
AnimationPtr decodeGif(const QByteArray& bytes, QSize box)
{
  QBuffer buffer;
  buffer.setData(bytes);
  buffer.open(QIODevice::ReadOnly);
  QImageReader reader(&buffer, "gif");
  const QSize full = reader.size();
  if (!reader.canRead() || full.isEmpty())
    return nullptr;
  // A 0 side is unbounded (a cropped picture bounds its long side only).
  const qreal sx = box.width() > 0 ? qreal(box.width()) / full.width() : 1.0;
  const qreal sy = box.height() > 0 ? qreal(box.height()) / full.height() : 1.0;
  const qreal scale = qMin<qreal>(1.0, qMin(sx, sy));
  QSize target = full;
  if (scale < 1.0)
    target = QSize(qMax(1, qRound(full.width() * scale)), qMax(1, qRound(full.height() * scale)));
  auto data = std::make_shared<AnimationData>();
  while (reader.canRead()) {
    QImage frame = reader.read();
    if (frame.isNull())
      break;
    if (frame.size() != target)
      frame = frame.scaled(target, Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
    frame = frame.convertToFormat(QImage::Format_ARGB32_Premultiplied);
    if (!data->frames.isEmpty() && data->bytes + frame.sizeInBytes() > kMaxAnimation)
      break;
    data->bytes += frame.sizeInBytes();
    data->frames.append(frame);
    data->delays.append(qMax(reader.nextImageDelay(), 16));
  }
  // QMovie's count: -1 forever, else how many times it repeats.
  const int repeats = reader.loopCount();
  data->loops = repeats < 0 ? 0 : repeats + 1;
  if (data->frames.isEmpty())
    return nullptr;
  return data;
}

// The box to fit a picture into so it covers `box` (a cropped fill): the
// picture scaled by the larger of the two ratios, never up. The box itself
// when the header gives no size.
QSize coverBox(const QByteArray& bytes, QSize box)
{
  QBuffer buffer;
  buffer.setData(bytes);
  buffer.open(QIODevice::ReadOnly);
  const QSize full = QImageReader(&buffer).size();
  if (full.isEmpty() || box.isEmpty())
    return box;
  const qreal scale = qMin<qreal>(1.0, qMax(qreal(box.width()) / full.width(),
                                            qreal(box.height()) / full.height()));
  return QSize(qMax(1, int(std::ceil(full.width() * scale))),
               qMax(1, int(std::ceil(full.height() * scale))));
}

// A GIF kept as its file for streamed items: only its size, frame count
// and loops are read here (QGifHandler scans the blocks, no pixels).
AnimationPtr scanGif(const QByteArray& bytes)
{
  QBuffer buffer;
  buffer.setData(bytes);
  buffer.open(QIODevice::ReadOnly);
  QImageReader reader(&buffer, "gif");
  const QSize size = reader.size();
  const int count = reader.imageCount();
  if (!reader.canRead() || size.isEmpty() || count <= 0)
    return nullptr;
  auto data = std::make_shared<AnimationData>();
  const int repeats = reader.loopCount();
  data->loops = repeats < 0 ? 0 : repeats + 1;
  data->raw = bytes;
  data->rawSize = size;
  data->rawFrames = count;
  // The file, plus the frame and the reader's canvas each item holds.
  data->bytes = bytes.size();
  return data;
}

// Anything else Qt reads (a JPEG poster, a BMP): one still frame, fitted
// into `box` like the others (never scaled up), so the item can stand in
// for an AnimatedImage whatever the picture turns out to be.
AnimationPtr decodeStill(const QByteArray& bytes, QSize box)
{
  QBuffer buffer;
  buffer.setData(bytes);
  buffer.open(QIODevice::ReadOnly);
  QImageReader reader(&buffer);
  reader.setAutoTransform(true);
  const QSize full = reader.size();
  if (!full.isEmpty()) {
    const qreal sx = box.width() > 0 ? qreal(box.width()) / full.width() : 1.0;
    const qreal sy = box.height() > 0 ? qreal(box.height()) / full.height() : 1.0;
    const qreal scale = qMin<qreal>(1.0, qMin(sx, sy));
    if (scale < 1.0)
      reader.setScaledSize(QSize(qMax(1, qRound(full.width() * scale)),
                                 qMax(1, qRound(full.height() * scale))));
  }
  QImage frame = reader.read();
  if (frame.isNull())
    return nullptr;
  frame = frame.convertToFormat(QImage::Format_ARGB32_Premultiplied);
  auto data = std::make_shared<AnimationData>();
  data->bytes = frame.sizeInBytes();
  data->frames.append(std::move(frame));
  data->delays.append(100);
  data->loops = 1;
  return data;
}

// What Qt's reader shows of a PNG is its default image (IDAT). In an APNG
// that is the first frame only when an fcTL chunk comes before the first
// IDAT; otherwise it is a fallback outside the animation (some profile
// effects carry a solid placeholder there), never to be shown.
bool pngDefaultIsFirstFrame(const QByteArray& bytes)
{
  static const char kSignature[] = "\x89PNG\r\n\x1a\n";
  if (!bytes.startsWith(QByteArray::fromRawData(kSignature, 8)))
    return true;
  bool animated = false;
  qsizetype at = 8;
  while (at + 8 <= bytes.size()) {
    const auto* p = reinterpret_cast<const uchar*>(bytes.constData() + at);
    const quint32 length = (quint32(p[0]) << 24) | (quint32(p[1]) << 16) | (quint32(p[2]) << 8) | p[3];
    const QByteArray type = bytes.mid(at + 4, 4);
    if (type == "acTL")
      animated = true;
    else if (type == "fcTL")
      return true;
    else if (type == "IDAT")
      return !animated;
    at += 12 + qsizetype(length);
  }
  return !animated;
}

AnimationPtr decodeBytes(const QByteArray& bytes, QSize box)
{
  if (bytes.startsWith("GIF8"))
    return decodeGif(bytes, box);
  void* anim = yap_anim_decode(reinterpret_cast<const uint8_t*>(bytes.constData()),
                               size_t(bytes.size()), uint32_t(qMax(0, box.width())),
                               uint32_t(qMax(0, box.height())));
  if (!anim)
    return decodeStill(bytes, box);
  YapAnimInfo info{};
  yap_anim_info(anim, &info);
  auto data = std::make_shared<AnimationData>();
  data->loops = int(info.loops);
  for (uint32_t i = 0; i < info.frames; ++i) {
    uint32_t delay = 0;
    const uint8_t* rgba = yap_anim_frame(anim, i, &delay);
    if (!rgba)
      break;
    const qsizetype size = qsizetype(info.width) * info.height * 4;
    if (!data->frames.isEmpty() && data->bytes + size > kMaxAnimation)
      break;
    QImage frame(rgba, int(info.width), int(info.height), int(info.width) * 4,
                 QImage::Format_RGBA8888_Premultiplied);
    data->frames.append(frame.copy());
    // The Rust frame is done with: never hold both copies of the whole.
    yap_anim_release_frame(anim, i);
    data->delays.append(int(qMax<uint32_t>(delay, 16)));
    data->bytes += size;
  }
  yap_anim_free(anim);
  if (data->frames.isEmpty())
    return nullptr;
  return data;
}

// What the item draws: one image node, and a texture per frame made the
// first time that frame shows. Deleted on the render thread with the node.
class AnimationNode : public QSGNode
{
public:
  explicit AnimationNode(QSGImageNode* image)
    : image(image)
  {
    appendChildNode(image);
  }
  ~AnimationNode() override { qDeleteAll(textures); }

  void reset(const void* owner, qsizetype count)
  {
    if (owner == anim && textures.size() == count)
      return;
    qDeleteAll(textures);
    textures = std::vector<QSGTexture*>(size_t(count), nullptr);
    anim = owner;
  }

  QSGImageNode* image;
  std::vector<QSGTexture*> textures;
  const void* anim = nullptr;
};

} // namespace

AnimationCache* AnimationCache::instance()
{
  static auto* cache = new AnimationCache;
  return cache;
}

AnimationPtr AnimationCache::find(const QString& key)
{
  auto it = m_entries.constFind(key);
  if (it == m_entries.constEnd())
    return nullptr;
  m_order.removeOne(key);
  m_order.append(key);
  return it.value();
}

void AnimationCache::request(QNetworkAccessManager* network, const QUrl& url,
                             QSize box, const QString& key, bool cover, bool raw)
{
  if (m_entries.contains(key) || m_pending.contains(key) || m_failed.contains(key))
    return;
  if (!network || !url.isValid())
    return;
  m_pending.insert(key);
  QNetworkRequest request(url);
  request.setAttribute(QNetworkRequest::CacheLoadControlAttribute,
                       QNetworkRequest::PreferCache);
  QNetworkReply* reply = network->get(request);
  connect(reply, &QNetworkReply::downloadProgress, reply,
          [reply](qint64 received, qint64) {
            if (received > kMaxDownload)
              reply->abort();
          });
  connect(reply, &QNetworkReply::finished, this, [this, reply, box, key, cover, raw] {
    reply->deleteLater();
    if (reply->error() != QNetworkReply::NoError) {
      std::fprintf(stderr, "[anim] %s: %s\n", qPrintable(reply->url().host()),
                   qPrintable(reply->errorString()));
      m_pending.remove(key);
      m_failed.insert(key);
      emit loaded(key);
      return;
    }
    const QByteArray bytes = reply->readAll();
    QPointer<AnimationCache> self(this);
    QThreadPool::globalInstance()->start([self, bytes, box, key, cover, raw] {
      // `last`: the whole animation (or the failure); otherwise a first
      // frame to show while the rest decode.
      auto post = [self, key](AnimationPtr data, bool last) {
        QMetaObject::invokeMethod(
          AnimationCache::instance(),
          [self, data, key, last] {
            if (!self || !self->m_pending.contains(key))
              return;
            if (last)
              self->m_pending.remove(key);
            if (data)
              self->insert(key, data);
            else if (last)
              self->m_failed.insert(key);
            if (data || last)
              emit self->loaded(key);
          },
          Qt::QueuedConnection);
      };
      const bool gif = bytes.startsWith("GIF8");
      if (raw && gif) {
        post(scanGif(bytes), true);
        return;
      }
      const QSize target = cover ? coverBox(bytes, box) : box;
      // A large APNG or WebP (a banner, a profile effect, a decoration)
      // takes a while to decode whole: its first frame (Qt's reader, one
      // frame) shows meanwhile, as a browser paints it before the rest.
      // An APNG whose default image is not a frame has no preview.
      if (!gif && bytes.size() > kPreviewBytes && pngDefaultIsFirstFrame(bytes))
        if (AnimationPtr first = decodeStill(bytes, target))
          post(first, false);
      post(decodeBytes(bytes, target), true);
    });
  });
}

void AnimationCache::insert(const QString& key, AnimationPtr data)
{
  // The whole animation replaces its first-frame preview.
  if (auto old = m_entries.find(key); old != m_entries.end()) {
    m_bytes -= old.value()->bytes;
    if (!old.value()->raw.isEmpty())
      m_rawBytes -= old.value()->bytes;
    m_entries.erase(old);
    m_order.removeOne(key);
  }
  const QSize size = data->frames.isEmpty() ? data->rawSize : data->frames.first().size();
  std::fprintf(stderr, "[anim] %s: %lld frames at %dx%d, %.1f MB (cache %.1f MB)\n",
               qPrintable(QUrl(key.section(QLatin1Char('|'), 0, 0)).fileName()),
               static_cast<long long>(data->frames.isEmpty() ? data->rawFrames : data->frames.size()),
               size.width(), size.height(),
               double(data->bytes) / (1024 * 1024),
               double(m_bytes + data->bytes) / (1024 * 1024));
  m_bytes += data->bytes;
  if (!data->raw.isEmpty())
    m_rawBytes += data->bytes;
  m_entries.insert(key, std::move(data));
  m_order.append(key);
  trim();
}

void AnimationCache::trim()
{
  // Entries an item still holds stay alive through its own pointer; the
  // cache only forgets them.
  auto forget = [this](qsizetype index) {
    const QString key = m_order.takeAt(index);
    if (auto it = m_entries.find(key); it != m_entries.end()) {
      m_bytes -= it.value()->bytes;
      if (!it.value()->raw.isEmpty())
        m_rawBytes -= it.value()->bytes;
      m_entries.erase(it);
    }
  };
  // Streamed files first, oldest first, within their own budget.
  for (qsizetype i = 0; m_rawBytes > kRawBudget && i < m_order.size() - 1;) {
    auto it = m_entries.constFind(m_order.at(i));
    if (it != m_entries.constEnd() && !it.value()->raw.isEmpty())
      forget(i);
    else
      ++i;
  }
  while (m_bytes > kCacheBudget && m_order.size() > 1)
    forget(0);
}

namespace {

// The shared grid frame changes are put on (see the header).
constexpr qint64 kSlotMs = 33;

qint64 animationNow()
{
  static const QElapsedTimer clock = [] {
    QElapsedTimer started;
    started.start();
    return started;
  }();
  return clock.elapsed();
}

qint64 slotAfter(qint64 at)
{
  return (at + kSlotMs - 1) / kSlotMs * kSlotMs;
}

} // namespace

AnimatedPicture::AnimatedPicture(QQuickItem* parent)
  : QQuickItem(parent)
{
  setFlag(ItemHasContents, true);
  m_timer.setSingleShot(true);
  m_timer.setTimerType(Qt::PreciseTimer);
  connect(&m_timer, &QTimer::timeout, this, &AnimatedPicture::advance);
  connect(AnimationCache::instance(), &AnimationCache::loaded, this,
          &AnimatedPicture::onLoaded);
  connect(qGuiApp, &QGuiApplication::applicationStateChanged, this,
          &AnimatedPicture::schedule);
}

AnimatedPicture::~AnimatedPicture()
{
  disconnect(m_frameWatch);
}

void AnimatedPicture::setStream(bool stream)
{
  if (m_stream == stream)
    return;
  m_stream = stream;
  emit streamChanged();
  m_key.clear();
  load();
}

int AnimatedPicture::frameCount() const
{
  if (!m_anim)
    return 0;
  return m_anim->frames.isEmpty() ? m_anim->rawFrames : int(m_anim->frames.size());
}

int AnimatedPicture::delayOf(int frame) const
{
  if (!m_anim)
    return 100;
  if (m_anim->frames.isEmpty())
    return m_currentDelay;
  return m_anim->delays.value(frame, 100);
}

bool AnimatedPicture::openStream()
{
  m_reader.reset();
  m_buffer = std::make_unique<QBuffer>();
  m_buffer->setData(m_anim->raw);
  m_buffer->open(QIODevice::ReadOnly);
  m_reader = std::make_unique<QImageReader>(m_buffer.get(), "gif");
  return decodeNext();
}

bool AnimatedPicture::decodeNext()
{
  if (!m_reader || !m_reader->canRead())
    return false;
  QImage frame = m_reader->read();
  if (frame.isNull())
    return false;
  // The GPU scales it as it draws; only a GIF far larger than its tile
  // (twice over) is scaled down here, so the texture stays small.
  const qreal dpr = window() ? window()->effectiveDevicePixelRatio() : 1.0;
  const QSize box(int(std::ceil(width() * dpr)), int(std::ceil(height() * dpr)));
  if (box.width() > 0 && box.height() > 0 && frame.width() > 2 * box.width() &&
      frame.height() > 2 * box.height())
    frame = frame.scaled(frame.size().scaled(box, Qt::KeepAspectRatioByExpanding),
                         Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
  m_current = std::move(frame);
  m_currentDelay = qMax(m_reader->nextImageDelay(), 16);
  return true;
}

void AnimatedPicture::setSource(const QUrl& source)
{
  if (m_source == source)
    return;
  m_source = source;
  emit sourceChanged();
  load();
}

void AnimatedPicture::setPlaying(bool playing)
{
  if (m_playing == playing)
    return;
  m_playing = playing;
  emit playingChanged();
  schedule();
}

void AnimatedPicture::setFillMode(FillMode mode)
{
  if (m_fillMode == mode)
    return;
  m_fillMode = mode;
  emit fillModeChanged();
  // A cropped picture decodes to cover its box, not to fit it.
  load();
  update();
}

void AnimatedPicture::setLoops(int loops)
{
  if (m_loops == loops)
    return;
  m_loops = loops;
  emit loopsChanged();
  schedule();
}

void AnimatedPicture::restart()
{
  m_frame = 0;
  m_played = 0;
  if (m_anim && m_anim->frames.isEmpty())
    openStream();
  if (m_finished) {
    m_finished = false;
    emit finishedChanged();
  }
  m_frameDirty = true;
  update();
  // The first frame gets its whole time again.
  m_timer.stop();
  m_due = 0;
  schedule();
}

void AnimatedPicture::setStatus(Status status)
{
  if (m_status == status)
    return;
  m_status = status;
  emit statusChanged();
}

int AnimatedPicture::playCount() const
{
  if (m_loops >= 0)
    return m_loops;
  return m_anim ? m_anim->loops : 0;
}

void AnimatedPicture::load()
{
  if (!isComponentComplete())
    return;
  if (m_source.isEmpty()) {
    m_key.clear();
    m_anim.reset();
    m_timer.stop();
    setStatus(Null);
    update();
    return;
  }
  // Shown items only: a picture in a closed popup or a page not shown
  // (the profile editor's preview, a shop card) decodes when it first
  // shows; a profile effect alone is tens of MB of frames.
  if (!isVisible() || !window())
    return;
  const qreal dpr = window()->effectiveDevicePixelRatio();
  if (width() <= 0 || height() <= 0)
    return;
  // Sizes round up to 16 px so neighbouring sizes share one decode.
  auto bucket = [](qreal v) { return (int(std::ceil(v)) + 15) / 16 * 16; };
  const QSize box(bucket(width() * dpr), bucket(height() * dpr));
  // A cropped picture decodes to cover the box, a fitted one to fit inside
  // it (fitting a square banner into 340 x 96 decoded it at 96 x 96 and
  // stretched that).
  const bool cover = m_fillMode == PreserveAspectCrop;
  const QString key = m_source.toString() + QLatin1Char('|') + QString::number(box.width())
                      + QLatin1Char('x') + QString::number(box.height())
                      + (cover ? QStringLiteral("c") : QString())
                      + (m_stream ? QStringLiteral("|raw") : QString());
  if (key == m_key && (m_anim || m_status == Loading))
    return;
  m_key = key;
  if (AnimationPtr data = AnimationCache::instance()->find(key)) {
    onLoaded(key);
    return;
  }
  // The same picture at a new size keeps showing (and stays Ready) until
  // the new decode lands: a layout that waits for Ready (the viewer's box)
  // would otherwise shrink back and ask for the old size again.
  if (!sameSourceShown())
    setStatus(Loading);
  QQmlEngine* engine = qmlEngine(this);
  AnimationCache::instance()->request(engine ? engine->networkAccessManager() : nullptr,
                                      m_source, box, key, cover, m_stream);
  if (AnimationCache::instance()->failed(key) && !sameSourceShown())
    setStatus(Error);
}

bool AnimatedPicture::sameSourceShown() const
{
  return m_anim && m_animSource == m_source;
}

void AnimatedPicture::onLoaded(const QString& key)
{
  if (key != m_key)
    return;
  AnimationPtr data = AnimationCache::instance()->find(key);
  if (!data) {
    if (AnimationCache::instance()->failed(key) && !sameSourceShown())
      setStatus(Error);
    return;
  }
  const bool fresh = data != m_anim;
  m_anim = data;
  m_animSource = m_source;
  if (fresh) {
    m_frame = 0;
    m_played = 0;
    m_timer.stop();
    m_due = 0;
    if (m_finished) {
      m_finished = false;
      emit finishedChanged();
    }
    if (m_anim->frames.isEmpty() && !openStream()) {
      m_anim.reset();
      m_reader.reset();
      m_buffer.reset();
      setStatus(Error);
      return;
    }
  }
  if (m_anim->frames.isEmpty()) {
    // Streamed: the file's own size, as AnimatedImage reports it.
    if (implicitWidth() <= 0)
      setImplicitSize(m_anim->rawSize.width(), m_anim->rawSize.height());
  } else if (implicitWidth() <= 0) {
    const qreal dpr = window() ? window()->effectiveDevicePixelRatio() : 1.0;
    setImplicitSize(m_anim->frames.first().width() / dpr,
                    m_anim->frames.first().height() / dpr);
  }
  setStatus(Ready);
  emit statusChanged();
  m_frameDirty = true;
  update();
  schedule();
}

bool visibleInWindow(const QQuickItem* item)
{
  QQuickWindow* win = item->window();
  if (!win || !item->isVisible())
    return false;
  QRectF shown = item->mapRectToScene(item->boundingRect());
  shown &= QRectF(0, 0, win->width(), win->height());
  for (QQuickItem* up = item->parentItem(); up && !shown.isEmpty(); up = up->parentItem()) {
    if (up->clip())
      shown &= up->mapRectToScene(up->boundingRect());
  }
  return !shown.isEmpty();
}

bool AnimatedPicture::onScreen() const
{
  return visibleInWindow(this);
}

ViewportWatch::ViewportWatch(QQuickItem* parent)
  : QQuickItem(parent)
{
}

void ViewportWatch::itemChange(ItemChange change, const ItemChangeData& value)
{
  QQuickItem::itemChange(change, value);
  if (change == ItemSceneChange) {
    disconnect(m_frames);
    if (value.window)
      m_frames = connect(value.window, &QQuickWindow::afterAnimating, this,
                         &ViewportWatch::check);
    check();
  } else if (change == ItemVisibleHasChanged) {
    check();
  }
}

void ViewportWatch::geometryChange(const QRectF& next, const QRectF& previous)
{
  QQuickItem::geometryChange(next, previous);
  check();
}

void ViewportWatch::check()
{
  const bool shown = visibleInWindow(this);
  if (shown == m_inView)
    return;
  m_inView = shown;
  emit inViewChanged();
}

bool AnimatedPicture::shouldRun() const
{
  if (!m_playing || !m_anim || frameCount() < 2 || m_finished)
    return false;
  if (!isVisible() || opacity() <= 0)
    return false;
  QQuickWindow* win = window();
  if (!win || !win->isVisible() || win->visibility() == QWindow::Minimized)
    return false;
  return QGuiApplication::applicationState() == Qt::ApplicationActive;
}

void AnimatedPicture::schedule()
{
  disconnect(m_frameWatch);
  if (!shouldRun()) {
    m_timer.stop();
    m_due = 0;
    return;
  }
  if (!onScreen()) {
    m_timer.stop();
    m_due = 0;
    // Scrolling back renders frames: look again after each one.
    if (m_window)
      m_frameWatch = connect(m_window, &QQuickWindow::afterAnimating, this, [this] {
        if (onScreen())
          schedule();
      });
    return;
  }
  if (!m_timer.isActive()) {
    const qint64 now = animationNow();
    if (m_due == 0)
      m_due = now + delayOf(m_frame);
    startTimer(now);
  }
}

void AnimatedPicture::startTimer(qint64 now)
{
  m_timer.start(int(qMax<qint64>(1, slotAfter(m_due) - now)));
}

void AnimatedPicture::advance()
{
  if (!m_anim || !shouldRun())
    return;
  if (!onScreen()) {
    schedule();
    return;
  }
  const qint64 now = animationNow();
  const int count = frameCount();
  const bool streamed = m_anim->frames.isEmpty();
  bool moved = false;
  // Every frame due by now: the slot may come after several.
  for (int skipped = 0; m_due <= now && skipped < count; ++skipped) {
    int next = m_frame + 1;
    if (next >= count) {
      ++m_played;
      const int plays = playCount();
      if (plays > 0 && m_played >= plays) {
        m_finished = true;
        m_due = 0;
        emit finishedChanged();
        break;
      }
      next = 0;
    }
    // Streamed: frames are deltas, so each one due is decoded in turn; the
    // first comes from a fresh reader (also when the file ends early).
    if (streamed && (next == 0 || !decodeNext())) {
      next = 0;
      if (!openStream())
        break;
    }
    m_frame = next;
    moved = true;
    m_due += delayOf(m_frame);
  }
  if (moved) {
    m_frameDirty = true;
    update();
  }
  if (m_finished)
    return;
  // Far behind (a long stall): carry on from now.
  if (m_due <= now)
    m_due = now + delayOf(m_frame);
  startTimer(now);
}

void AnimatedPicture::watchWindow(QQuickWindow* window)
{
  disconnect(m_frameWatch);
  m_window = window;
  if (window) {
    connect(window, &QWindow::visibilityChanged, this, &AnimatedPicture::schedule,
            Qt::UniqueConnection);
  }
}

void AnimatedPicture::geometryChange(const QRectF& next, const QRectF& previous)
{
  QQuickItem::geometryChange(next, previous);
  if (next.size() != previous.size())
    load();
  update();
}

void AnimatedPicture::itemChange(ItemChange change, const ItemChangeData& value)
{
  QQuickItem::itemChange(change, value);
  if (change == ItemSceneChange) {
    watchWindow(value.window);
    load();
  } else if (change == ItemVisibleHasChanged) {
    if (isVisible())
      load();
    else
      release();
    schedule();
  }
}

void AnimatedPicture::release()
{
  // Hidden: the frames go back to the cache, which keeps them within its
  // budget for the next show (or forgets them), and the node with its
  // textures goes too. A load still in flight is forgotten: showing again
  // asks the cache afresh.
  m_key.clear();
  m_timer.stop();
  m_reader.reset();
  m_buffer.reset();
  m_current = QImage();
  if (m_anim) {
    m_anim.reset();
    update();
  }
}

void AnimatedPicture::componentComplete()
{
  QQuickItem::componentComplete();
  load();
}

void AnimatedPicture::releaseResources()
{
  // The node (and its textures) goes with the scene graph.
}

QSGNode* AnimatedPicture::updatePaintNode(QSGNode* old, UpdatePaintNodeData*)
{
  auto* node = static_cast<AnimationNode*>(old);
  const bool streamed = m_anim && m_anim->frames.isEmpty();
  if (!m_anim || (streamed ? m_current.isNull() : m_anim->frames.isEmpty()) || width() <= 0 ||
      height() <= 0) {
    delete node;
    return nullptr;
  }
  QQuickWindow* win = window();
  if (!win)
    return node;
  if (!node)
    node = new AnimationNode(win->createImageNode());
  node->reset(m_anim.get(), streamed ? 1 : m_anim->frames.size());
  const int index = streamed ? 0 : qBound(0, m_frame, int(m_anim->frames.size()) - 1);
  QSGTexture*& texture = node->textures[size_t(index)];
  if (streamed && texture && m_frameDirty) {
    // One texture, given each frame in turn. A plain texture of the same
    // size uploads into its QRhiTexture again; making a new one per frame
    // cost a D3D11 texture create and release each time (performance pass
    // 7: ~5 % of the GUI thread in the animated idle, plus driver work).
    if (auto* plain = qobject_cast<QSGPlainTexture*>(texture);
        plain && plain->textureSize() == m_current.size()) {
      plain->setImage(m_current);
    } else {
      delete texture;
      texture = nullptr;
    }
  }
  if (!texture)
    texture = win->createTextureFromImage(streamed ? m_current : m_anim->frames.at(index),
                                          QQuickWindow::TextureHasAlphaChannel);
  m_frameDirty = false;
  QSGImageNode* image = node->image;
  image->setOwnsTexture(false);
  image->setTexture(texture);
  image->setFiltering(QSGTexture::Linear);

  const QSizeF picture = texture->textureSize();
  QRectF target = boundingRect();
  QRectF from(QPointF(0, 0), picture);
  if (m_fillMode != Stretch && picture.width() > 0 && picture.height() > 0) {
    const qreal sx = target.width() / picture.width();
    const qreal sy = target.height() / picture.height();
    if (m_fillMode == PreserveAspectFit) {
      const qreal s = qMin(sx, sy);
      const QSizeF size(picture.width() * s, picture.height() * s);
      target = QRectF(QPointF((width() - size.width()) / 2, (height() - size.height()) / 2),
                      size);
    } else {
      const qreal s = qMax(sx, sy);
      const QSizeF shown(target.width() / s, target.height() / s);
      from = QRectF(QPointF((picture.width() - shown.width()) / 2,
                            (picture.height() - shown.height()) / 2),
                    shown);
    }
  }
  image->setRect(target);
  image->setSourceRect(from);
  return node;
}
