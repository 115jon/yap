#include "animatedpicture.h"

#include <QtCore/QThreadPool>
#include <QtGui/QGuiApplication>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkReply>
#include <QtNetwork/QNetworkRequest>
#include <QtQml/QQmlEngine>
#include <QtQuick/QQuickWindow>
#include <QtQuick/QSGImageNode>
#include <QtQuick/QSGTexture>

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
// A picture larger than this is not an avatar decoration or an effect.
constexpr qint64 kMaxDownload = 24 * 1024 * 1024;
// One animation's decoded frames (yap-anim stops at the same size).
constexpr qsizetype kMaxAnimation = 48 * 1024 * 1024;

AnimationPtr decodeBytes(const QByteArray& bytes, QSize box)
{
  void* anim = yap_anim_decode(reinterpret_cast<const uint8_t*>(bytes.constData()),
                               size_t(bytes.size()), uint32_t(qMax(0, box.width())),
                               uint32_t(qMax(0, box.height())));
  if (!anim)
    return nullptr;
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
                             QSize box, const QString& key)
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
  connect(reply, &QNetworkReply::finished, this, [this, reply, box, key] {
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
    QThreadPool::globalInstance()->start([self, bytes, box, key] {
      AnimationPtr data = decodeBytes(bytes, box);
      QMetaObject::invokeMethod(
        AnimationCache::instance(),
        [self, data, key] {
          if (!self)
            return;
          self->m_pending.remove(key);
          if (data)
            self->insert(key, data);
          else
            self->m_failed.insert(key);
          emit self->loaded(key);
        },
        Qt::QueuedConnection);
    });
  });
}

void AnimationCache::insert(const QString& key, AnimationPtr data)
{
  const QSize size = data->frames.isEmpty() ? QSize() : data->frames.first().size();
  std::fprintf(stderr, "[anim] %s: %lld frames at %dx%d, %.1f MB (cache %.1f MB)\n",
               qPrintable(QUrl(key.section(QLatin1Char('|'), 0, 0)).fileName()),
               static_cast<long long>(data->frames.size()), size.width(), size.height(),
               double(data->bytes) / (1024 * 1024),
               double(m_bytes + data->bytes) / (1024 * 1024));
  m_bytes += data->bytes;
  m_entries.insert(key, std::move(data));
  m_order.append(key);
  trim();
}

void AnimationCache::trim()
{
  // Entries an item still holds stay alive through its own pointer; the
  // cache only forgets them.
  while (m_bytes > kCacheBudget && m_order.size() > 1) {
    const QString oldest = m_order.takeFirst();
    if (auto it = m_entries.find(oldest); it != m_entries.end()) {
      m_bytes -= it.value()->bytes;
      m_entries.erase(it);
    }
  }
}

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
  if (m_finished) {
    m_finished = false;
    emit finishedChanged();
  }
  m_frameDirty = true;
  update();
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
  const QString key = m_source.toString() + QLatin1Char('|') + QString::number(box.width())
                      + QLatin1Char('x') + QString::number(box.height());
  if (key == m_key && (m_anim || m_status == Loading))
    return;
  m_key = key;
  if (AnimationPtr data = AnimationCache::instance()->find(key)) {
    onLoaded(key);
    return;
  }
  setStatus(Loading);
  QQmlEngine* engine = qmlEngine(this);
  AnimationCache::instance()->request(engine ? engine->networkAccessManager() : nullptr,
                                      m_source, box, key);
  if (AnimationCache::instance()->failed(key))
    setStatus(Error);
}

void AnimatedPicture::onLoaded(const QString& key)
{
  if (key != m_key)
    return;
  AnimationPtr data = AnimationCache::instance()->find(key);
  if (!data) {
    if (AnimationCache::instance()->failed(key))
      setStatus(Error);
    return;
  }
  const bool fresh = data != m_anim;
  m_anim = data;
  if (fresh) {
    m_frame = 0;
    m_played = 0;
    if (m_finished) {
      m_finished = false;
      emit finishedChanged();
    }
  }
  if (implicitWidth() <= 0 && !m_anim->frames.isEmpty()) {
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

bool AnimatedPicture::onScreen() const
{
  QQuickWindow* win = window();
  if (!win)
    return false;
  QRectF shown = mapRectToScene(boundingRect());
  shown &= QRectF(0, 0, win->width(), win->height());
  for (QQuickItem* up = parentItem(); up && !shown.isEmpty(); up = up->parentItem()) {
    if (up->clip())
      shown &= up->mapRectToScene(up->boundingRect());
  }
  return !shown.isEmpty();
}

bool AnimatedPicture::shouldRun() const
{
  if (!m_playing || !m_anim || m_anim->frames.size() < 2 || m_finished)
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
    return;
  }
  if (!onScreen()) {
    m_timer.stop();
    // Scrolling back renders frames: look again after each one.
    if (m_window)
      m_frameWatch = connect(m_window, &QQuickWindow::afterAnimating, this, [this] {
        if (onScreen())
          schedule();
      });
    return;
  }
  if (!m_timer.isActive())
    m_timer.start(m_anim->delays.value(m_frame, 100));
}

void AnimatedPicture::advance()
{
  if (!m_anim || !shouldRun())
    return;
  if (!onScreen()) {
    schedule();
    return;
  }
  const int count = int(m_anim->frames.size());
  int next = m_frame + 1;
  if (next >= count) {
    ++m_played;
    const int plays = playCount();
    if (plays > 0 && m_played >= plays) {
      m_finished = true;
      emit finishedChanged();
      return;
    }
    next = 0;
  }
  m_frame = next;
  m_frameDirty = true;
  update();
  m_timer.start(m_anim->delays.value(m_frame, 100));
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
  if (!m_anim || m_anim->frames.isEmpty() || width() <= 0 || height() <= 0) {
    delete node;
    return nullptr;
  }
  QQuickWindow* win = window();
  if (!win)
    return node;
  if (!node)
    node = new AnimationNode(win->createImageNode());
  node->reset(m_anim.get(), m_anim->frames.size());
  const int index = qBound(0, m_frame, int(m_anim->frames.size()) - 1);
  QSGTexture*& texture = node->textures[size_t(index)];
  if (!texture)
    texture = win->createTextureFromImage(m_anim->frames.at(index),
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
