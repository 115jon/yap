#include "webembed.h"

#include <QtCore/QHash>
#include <QtGui/QGuiApplication>
#include <QtQuick/QQuickWindow>

#include <cmath>
#include <cstdio>

// src/webembed.rs
extern "C" void yap_web_set_callback(void (*callback)(uint64_t, int, const char16_t*, size_t));
extern "C" uint64_t yap_web_open(intptr_t parent, const char16_t* url, size_t len, bool muted);
extern "C" void yap_web_place(uint64_t id, int item_x, int item_y, int item_w, int item_h,
                              int clip_x, int clip_y, int clip_w, int clip_h, bool visible,
                              double scale);
extern "C" void yap_web_parent_moved(uint64_t id);
extern "C" void yap_web_close(uint64_t id);

namespace {

constexpr int kReady = 1;
constexpr int kFailed = 2;
constexpr int kExternal = 3;
constexpr int kCrashed = 4;

// Set once WebView2 could not run here: every player then starts Failed, so
// a click opens the link at once instead of trying again.
bool& unavailable()
{
  static bool value = false;
  return value;
}

QHash<uint64_t, QPointer<WebEmbed>>& players()
{
  static QHash<uint64_t, QPointer<WebEmbed>> map;
  return map;
}

// The one live player.
QPointer<WebEmbed>& current()
{
  static QPointer<WebEmbed> item;
  return item;
}

void callback(uint64_t id, int event, const char16_t* text, size_t len)
{
  // Rust may call while an open is still on the stack: hand it over later.
  const QString value = QString::fromUtf16(text, qsizetype(len));
  QMetaObject::invokeMethod(
    qGuiApp, [id, event, value] { WebEmbed::onEvent(id, event, value); },
    Qt::QueuedConnection);
}

bool mutedForTests()
{
  return qEnvironmentVariableIntValue("YAP_EMBED_MUTED") != 0;
}

QQuickItem* overlayOf(QQuickWindow* window)
{
  if (!window || !window->contentItem())
    return nullptr;
  for (QQuickItem* child : window->contentItem()->childItems()) {
    if (qstrcmp(child->metaObject()->className(), "QQuickOverlay") == 0)
      return child;
  }
  return nullptr;
}

} // namespace

PlayerCover* PlayerCover::instance()
{
  static auto* cover = new PlayerCover;
  return cover;
}

void PlayerCover::add(QQuickItem* item)
{
  m_items.removeAll(nullptr);
  if (item && !m_items.contains(item))
    m_items.append(item);
}

void PlayerCover::remove(QQuickItem* item)
{
  m_items.removeAll(item);
  m_items.removeAll(nullptr);
}

QRectF PlayerCover::trim(QRectF shown) const
{
  for (const QPointer<QQuickItem>& item : m_items) {
    if (shown.isEmpty())
      break;
    if (!item || !item->isVisible() || item->opacity() <= 0)
      continue;
    const QRectF band = item->mapRectToScene(item->boundingRect());
    if (!band.intersects(shown))
      continue;
    if (band.top() <= shown.top() && band.bottom() < shown.bottom())
      shown.setTop(band.bottom());
    else if (band.bottom() >= shown.bottom() && band.top() > shown.top())
      shown.setBottom(band.top());
    else
      shown = QRectF();
  }
  return shown;
}

WebEmbed::WebEmbed(QQuickItem* parent)
  : QQuickItem(parent)
{
  static const bool registered = [] {
    yap_web_set_callback(&callback);
    return true;
  }();
  Q_UNUSED(registered);
  if (unavailable())
    m_state = Failed;
}

WebEmbed::~WebEmbed()
{
  close();
}

void WebEmbed::setUrl(const QString& url)
{
  if (m_url == url)
    return;
  m_url = url;
  emit urlChanged();
  // A pooled row reused for another link must not start playing it: the old
  // player goes and the new one waits for its click.
  close();
  if (m_active) {
    m_active = false;
    emit activeChanged();
  }
}

void WebEmbed::setActive(bool active)
{
  if (m_active == active)
    return;
  m_active = active;
  emit activeChanged();
  sync();
}

void WebEmbed::setClipTo(QQuickItem* item)
{
  if (m_clipTo == item)
    return;
  m_clipTo = item;
  emit clipToChanged();
  place();
}

void WebEmbed::setCovered(bool covered)
{
  if (m_covered == covered)
    return;
  m_covered = covered;
  emit coveredChanged();
  place();
}

void WebEmbed::setState(State state)
{
  if (m_state == state)
    return;
  m_state = state;
  emit stateChanged();
}

void WebEmbed::sync()
{
  if (m_active && !m_url.isEmpty() && window())
    open();
  else
    close();
}

void WebEmbed::open()
{
  if (m_id != 0)
    return;
  // No WebView2 here: QML opens the link on a click instead.
  if (m_state == Failed)
    return;
  if (current() && current() != this) {
    WebEmbed* other = current();
    other->close();
    other->m_active = false;
    emit other->activeChanged();
    emit other->evicted();
  }
  QQuickWindow* win = window();
  const auto parent = intptr_t(win->winId());
  const std::u16string url = m_url.toStdU16String();
  m_id = yap_web_open(parent, url.data(), url.size(), mutedForTests());
  if (m_id == 0) {
    fail();
    return;
  }
  players().insert(m_id, this);
  current() = this;
  setState(Loading);
  m_window = win;
  m_frames = connect(win, &QQuickWindow::afterAnimating, this, &WebEmbed::place);
  connect(win, &QWindow::visibilityChanged, this, &WebEmbed::place, Qt::UniqueConnection);
  // The window moving on screen leaves the player where it is in it, but
  // WebView2 must hear of it (its popups and accessibility).
  for (auto signal : { &QWindow::xChanged, &QWindow::yChanged })
    connect(win, signal, this, &WebEmbed::parentMoved, Qt::UniqueConnection);
  place();
  // A still window renders nothing: make one frame so placement follows.
  win->update();
}

void WebEmbed::parentMoved()
{
  if (m_id != 0)
    yap_web_parent_moved(m_id);
}

void WebEmbed::close()
{
  disconnect(m_frames);
  if (m_id != 0) {
    players().remove(m_id);
    yap_web_close(m_id);
    m_id = 0;
  }
  if (current() == this)
    current() = nullptr;
  if (m_state != Failed)
    setState(Idle);
}

void WebEmbed::fail()
{
  unavailable() = true;
  close();
  setState(Failed);
  if (m_active) {
    m_active = false;
    emit activeChanged();
  }
  emit failed();
}

void WebEmbed::stop()
{
  close();
  if (m_active) {
    m_active = false;
    emit activeChanged();
  }
}

bool WebEmbed::popupOver(const QRectF& shown) const
{
  QQuickItem* overlay = overlayOf(window());
  if (!overlay)
    return false;
  for (QQuickItem* child : overlay->childItems()) {
    if (child->isVisible() && child->opacity() > 0
        && child->mapRectToScene(child->boundingRect()).intersects(shown))
      return true;
  }
  return false;
}

void WebEmbed::place()
{
  if (m_id == 0)
    return;
  QQuickWindow* win = window();
  if (!win)
    return;
  const QRectF item = mapRectToScene(boundingRect());
  QRectF shown = item & QRectF(0, 0, win->width(), win->height());
  for (QQuickItem* up = parentItem(); up && !shown.isEmpty(); up = up->parentItem()) {
    if (up->clip())
      shown &= up->mapRectToScene(up->boundingRect());
  }
  if (m_clipTo)
    shown &= m_clipTo->mapRectToScene(m_clipTo->boundingRect());
  shown = PlayerCover::instance()->trim(shown);
  const bool visible = isVisible() && !m_covered && win->isVisible()
                       && win->visibility() != QWindow::Minimized && !shown.isEmpty()
                       && !popupOver(shown);
  const qreal dpr = win->effectiveDevicePixelRatio();
  auto px = [dpr](qreal v) { return int(std::lround(v * dpr)); };
  const int ix = px(item.x());
  const int iy = px(item.y());
  const int cx = px(shown.x());
  const int cy = px(shown.y());
  yap_web_place(m_id, ix, iy, px(item.right()) - ix, px(item.bottom()) - iy, cx, cy,
                px(shown.right()) - cx, px(shown.bottom()) - cy, visible, dpr);
}

void WebEmbed::onEvent(uint64_t id, int event, const QString& text)
{
  WebEmbed* item = players().value(id);
  if (!item)
    return;
  switch (event) {
  case kReady:
    item->setState(Ready);
    item->place();
    break;
  case kFailed:
    std::fprintf(stderr, "[webembed] player unavailable; a click opens the link\n");
    item->fail();
    break;
  case kCrashed:
    // Back to the poster; the link is not opened for the user.
    std::fprintf(stderr, "[webembed] player crashed; back to the poster\n");
    item->stop();
    break;
  case kExternal:
    emit item->externalRequested(text);
    break;
  default:
    break;
  }
}

void WebEmbed::itemChange(ItemChange change, const ItemChangeData& value)
{
  QQuickItem::itemChange(change, value);
  if (change == ItemSceneChange) {
    close();
    if (value.window)
      sync();
  } else if (change == ItemVisibleHasChanged) {
    place();
  }
}

void WebEmbed::geometryChange(const QRectF& next, const QRectF& previous)
{
  QQuickItem::geometryChange(next, previous);
  place();
}
