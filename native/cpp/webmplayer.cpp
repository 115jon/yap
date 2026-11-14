#include "webmplayer.h"

#include <QtCore/QHash>
#include <QtCore/QMetaObject>
#include <QtCore/QMutexLocker>
#include <QtGui/QGuiApplication>
#include <QtMultimedia/QVideoFrameFormat>
#include <QtMultimedia/QVideoSink>
#include <QtQuick/QQuickWindow>

#include <cstring>

// src/webm.rs
using WebmFrameFn = void (*)(uint64_t, int, uint32_t, uint32_t, const uint8_t*, uint32_t,
                             const uint8_t*, uint32_t, const uint8_t*, uint32_t, uint64_t);
using WebmEventFn = void (*)(uint64_t, int, uint64_t, uint32_t, uint32_t, bool,
                             const uint8_t*, size_t);
extern "C" void yap_webm_set_callbacks(WebmFrameFn frame, WebmEventFn event);
extern "C" uint64_t yap_webm_open(const uint8_t* url, size_t len, uint32_t max_w,
                                  uint32_t max_h, bool looping, bool playing,
                                  uint64_t start_ms);
extern "C" void yap_webm_play(uint64_t id);
extern "C" void yap_webm_pause(uint64_t id);
extern "C" void yap_webm_seek(uint64_t id, uint64_t position_ms);
extern "C" void yap_webm_set_looping(uint64_t id, bool looping);
extern "C" void yap_webm_close(uint64_t id);

namespace {

constexpr int kI420 = 0;
constexpr int kBgra = 1;
constexpr int kLoaded = 1;
constexpr int kEnded = 2;
constexpr int kFailed = 3;

// Live players by id. A player thread holds the lock while it copies a
// frame into one, so a player is never deleted under it.
QMutex& registryMutex()
{
  static QMutex mutex;
  return mutex;
}

QHash<uint64_t, WebmPlayer*>& registry()
{
  static QHash<uint64_t, WebmPlayer*> map;
  return map;
}

void copyPlane(uchar* dst, int dstStride, const uint8_t* src, uint32_t srcStride, int width,
               int rows)
{
  if (dstStride == int(srcStride)) {
    std::memcpy(dst, src, size_t(dstStride) * size_t(rows));
    return;
  }
  for (int row = 0; row < rows; ++row)
    std::memcpy(dst + size_t(row) * size_t(dstStride), src + size_t(row) * srcStride,
                size_t(width));
}

void frameCallback(uint64_t id, int kind, uint32_t width, uint32_t height, const uint8_t* p0,
                   uint32_t s0, const uint8_t* p1, uint32_t s1, const uint8_t* p2, uint32_t s2,
                   uint64_t pts)
{
  WebmPlayer::onFrame(id, kind, width, height, p0, s0, p1, s1, p2, s2, pts);
}

void eventCallback(uint64_t id, int kind, uint64_t duration, uint32_t width, uint32_t height,
                   bool alpha, const uint8_t* text, size_t len)
{
  const QString message =
    text ? QString::fromUtf8(reinterpret_cast<const char*>(text), qsizetype(len)) : QString();
  WebmPlayer::onEvent(id, kind, duration, width, height, alpha, message);
}

} // namespace

WebmPlayer::WebmPlayer(QObject* parent)
  : QObject(parent)
{
  static const bool registered = [] {
    yap_webm_set_callbacks(&frameCallback, &eventCallback);
    return true;
  }();
  Q_UNUSED(registered);
  connect(qGuiApp, &QGuiApplication::applicationStateChanged, this, &WebmPlayer::sync);
}

WebmPlayer::~WebmPlayer()
{
  close();
}

void WebmPlayer::componentComplete()
{
  m_complete = true;
  open();
}

void WebmPlayer::setSource(const QString& source)
{
  if (m_source == source)
    return;
  m_source = source;
  emit sourceChanged();
  if (m_complete)
    open();
}

void WebmPlayer::setVideoOutput(QQuickItem* output)
{
  if (m_output == output)
    return;
  if (m_output)
    disconnect(m_output, nullptr, this, nullptr);
  m_output = output;
  if (m_output)
    connect(m_output, &QQuickItem::windowChanged, this, &WebmPlayer::watchWindow);
  watchWindow();
  emit videoOutputChanged();
}

void WebmPlayer::setLoops(int loops)
{
  if (m_loops == loops)
    return;
  m_loops = loops;
  if (m_id != 0)
    yap_webm_set_looping(m_id, m_loops != 1);
  emit loopsChanged();
}

void WebmPlayer::setDecorative(bool decorative)
{
  if (m_decorative == decorative)
    return;
  m_decorative = decorative;
  emit decorativeChanged();
  sync();
}

float WebmPlayer::bufferProgress() const
{
  return m_status == QMediaPlayer::LoadingMedia || m_status == QMediaPlayer::NoMedia ? 0.f : 1.f;
}

QSize WebmPlayer::maxSize() const
{
  // Nameplates are 448 x 84 and profile media is small; a chat video is
  // shown at most 640 x 400 inline (larger in the viewer).
  return m_decorative ? QSize(960, 540) : QSize(1920, 1080);
}

void WebmPlayer::open()
{
  close();
  m_loaded = false;
  m_error.clear();
  m_duration = 0;
  m_position = 0;
  m_size = QSize();
  m_alpha = false;
  emit durationChanged();
  emit positionChanged();
  if (m_source.isEmpty()) {
    setStatus(QMediaPlayer::NoMedia);
    return;
  }
  const QByteArray url = m_source.toUtf8();
  const QSize box = maxSize();
  m_running = m_wanted && shouldRun();
  m_id = yap_webm_open(reinterpret_cast<const uint8_t*>(url.constData()), size_t(url.size()),
                       uint32_t(box.width()), uint32_t(box.height()), m_loops != 1, m_running, 0);
  if (m_id == 0) {
    setStatus(QMediaPlayer::InvalidMedia);
    m_error = tr("The player could not start.");
    emit errorOccurred(QMediaPlayer::ResourceError, m_error);
    return;
  }
  {
    QMutexLocker lock(&registryMutex());
    registry().insert(m_id, this);
  }
  setStatus(QMediaPlayer::LoadingMedia);
}

void WebmPlayer::close()
{
  if (m_id == 0)
    return;
  {
    QMutexLocker lock(&registryMutex());
    registry().remove(m_id);
  }
  yap_webm_close(m_id);
  m_id = 0;
  m_running = false;
  {
    QMutexLocker lock(&m_mutex);
    m_latest = QVideoFrame();
  }
  // A reused row must not show the last source's picture while the next
  // one loads.
  if (QVideoSink* sink = videoSink())
    sink->setVideoFrame(QVideoFrame());
}

void WebmPlayer::play()
{
  m_wanted = true;
  if (m_status == QMediaPlayer::EndOfMedia && m_id != 0) {
    yap_webm_seek(m_id, 0);
    m_position = 0;
    emit positionChanged();
    setStatus(QMediaPlayer::BufferedMedia);
  }
  setState(QMediaPlayer::PlayingState);
  sync();
}

void WebmPlayer::pause()
{
  m_wanted = false;
  setState(QMediaPlayer::PausedState);
  sync();
}

void WebmPlayer::stop()
{
  m_wanted = false;
  sync();
  setPosition(0);
  setState(QMediaPlayer::StoppedState);
}

void WebmPlayer::setPosition(qint64 position)
{
  position = qBound<qint64>(0, position, m_duration > 0 ? m_duration : position);
  if (m_id != 0)
    yap_webm_seek(m_id, uint64_t(position));
  m_position = position;
  emit positionChanged();
  if (m_status == QMediaPlayer::EndOfMedia)
    setStatus(QMediaPlayer::BufferedMedia);
}

void WebmPlayer::setState(QMediaPlayer::PlaybackState state)
{
  if (m_state == state)
    return;
  m_state = state;
  emit playbackStateChanged();
}

void WebmPlayer::setStatus(QMediaPlayer::MediaStatus status)
{
  if (m_status == status)
    return;
  m_status = status;
  emit mediaStatusChanged();
}

void WebmPlayer::watchWindow()
{
  disconnect(m_frameWatch);
  disconnect(m_visibilityWatch);
  m_window = m_output ? m_output->window() : nullptr;
  if (m_window) {
    // Scrolling, resizing and popups all render a frame: look again then.
    m_frameWatch = connect(m_window, &QQuickWindow::afterAnimating, this, [this] {
      if (m_wanted)
        sync();
    });
    m_visibilityWatch =
      connect(m_window, &QWindow::visibilityChanged, this, &WebmPlayer::sync);
  }
  sync();
}

bool WebmPlayer::onScreen() const
{
  QQuickItem* item = m_output;
  QQuickWindow* win = m_window;
  if (!item || !win || !item->isVisible())
    return false;
  QRectF shown = item->mapRectToScene(item->boundingRect());
  shown &= QRectF(0, 0, win->width(), win->height());
  for (QQuickItem* up = item->parentItem(); up && !shown.isEmpty(); up = up->parentItem()) {
    if (!up->isVisible())
      return false;
    if (up->clip())
      shown &= up->mapRectToScene(up->boundingRect());
  }
  return !shown.isEmpty();
}

bool WebmPlayer::shouldRun() const
{
  QQuickWindow* win = m_window;
  if (!win || !win->isVisible() || win->visibility() == QWindow::Minimized)
    return false;
  // YAP_DECOR_IGNORE_FOCUS=1 (tests): decorations play in an unfocused
  // window too, so a harness can watch them without taking focus.
  static const bool ignoreFocus = qEnvironmentVariableIntValue("YAP_DECOR_IGNORE_FOCUS") != 0;
  if (m_decorative && !ignoreFocus
      && QGuiApplication::applicationState() != Qt::ApplicationActive)
    return false;
  return onScreen();
}

void WebmPlayer::sync()
{
  const bool run = m_wanted && m_id != 0 && m_status != QMediaPlayer::InvalidMedia
                   && m_status != QMediaPlayer::EndOfMedia && shouldRun();
  if (run == m_running)
    return;
  m_running = run;
  if (m_id == 0)
    return;
  if (run)
    yap_webm_play(m_id);
  else
    yap_webm_pause(m_id);
}

QVideoSink* WebmPlayer::videoSink() const
{
  return m_output ? m_output->property("videoSink").value<QVideoSink*>() : nullptr;
}

void WebmPlayer::push(QVideoFrame frame, qint64 pts)
{
  bool post = false;
  {
    QMutexLocker lock(&m_mutex);
    m_latest = std::move(frame);
    m_latestPts = pts;
    post = !m_posted;
    m_posted = true;
  }
  if (post)
    QMetaObject::invokeMethod(this, &WebmPlayer::deliver, Qt::QueuedConnection);
}

void WebmPlayer::deliver()
{
  QVideoFrame frame;
  qint64 pts = 0;
  {
    QMutexLocker lock(&m_mutex);
    frame = std::move(m_latest);
    m_latest = QVideoFrame();
    pts = m_latestPts;
    m_posted = false;
  }
  if (!frame.isValid())
    return;
  if (QVideoSink* sink = videoSink())
    sink->setVideoFrame(frame);
  if (m_position != pts) {
    m_position = pts;
    emit positionChanged();
  }
  if (m_status == QMediaPlayer::LoadingMedia || m_status == QMediaPlayer::LoadedMedia)
    setStatus(QMediaPlayer::BufferedMedia);
  sync();
}

void WebmPlayer::handle(int kind, qint64 duration, QSize size, bool alpha, const QString& text)
{
  switch (kind) {
  case kLoaded:
    m_loaded = true;
    m_duration = duration;
    m_size = size;
    m_alpha = alpha;
    emit durationChanged();
    if (m_status == QMediaPlayer::LoadingMedia)
      setStatus(QMediaPlayer::LoadedMedia);
    break;
  case kEnded:
    m_wanted = false;
    m_running = false;
    m_position = m_duration;
    emit positionChanged();
    setStatus(QMediaPlayer::EndOfMedia);
    setState(QMediaPlayer::StoppedState);
    break;
  case kFailed:
    m_wanted = false;
    m_running = false;
    m_error = text;
    setStatus(QMediaPlayer::InvalidMedia);
    setState(QMediaPlayer::StoppedState);
    emit errorOccurred(QMediaPlayer::ResourceError, text);
    break;
  default:
    break;
  }
}

void WebmPlayer::onFrame(uint64_t id, int kind, uint32_t width, uint32_t height,
                         const uint8_t* p0, uint32_t s0, const uint8_t* p1, uint32_t s1,
                         const uint8_t* p2, uint32_t s2, uint64_t pts)
{
  if (width == 0 || height == 0 || width > 8192 || height > 8192 || !p0)
    return;
  QMutexLocker lock(&registryMutex());
  WebmPlayer* player = registry().value(id);
  if (!player)
    return;
  const int w = int(width);
  const int h = int(height);
  if (kind == kI420) {
    if (!p1 || !p2)
      return;
    QVideoFrame frame(QVideoFrameFormat(QSize(w, h), QVideoFrameFormat::Format_YUV420P));
    if (!frame.map(QVideoFrame::WriteOnly))
      return;
    const int cw = (w + 1) / 2;
    const int ch = (h + 1) / 2;
    copyPlane(frame.bits(0), frame.bytesPerLine(0), p0, s0, w, h);
    copyPlane(frame.bits(1), frame.bytesPerLine(1), p1, s1, cw, ch);
    copyPlane(frame.bits(2), frame.bytesPerLine(2), p2, s2, cw, ch);
    frame.unmap();
    player->push(std::move(frame), qint64(pts));
  } else if (kind == kBgra) {
    QVideoFrame frame(
      QVideoFrameFormat(QSize(w, h), QVideoFrameFormat::Format_BGRA8888_Premultiplied));
    if (!frame.map(QVideoFrame::WriteOnly))
      return;
    copyPlane(frame.bits(0), frame.bytesPerLine(0), p0, s0, w * 4, h);
    frame.unmap();
    player->push(std::move(frame), qint64(pts));
  }
}

void WebmPlayer::onEvent(uint64_t id, int kind, uint64_t duration, uint32_t width,
                         uint32_t height, bool alpha, const QString& text)
{
  QMetaObject::invokeMethod(
    qGuiApp,
    [id, kind, duration, width, height, alpha, text] {
      WebmPlayer* player = nullptr;
      {
        QMutexLocker lock(&registryMutex());
        player = registry().value(id);
      }
      if (player)
        player->handle(kind, qint64(duration), QSize(int(width), int(height)), alpha, text);
    },
    Qt::QueuedConnection);
}
