#include "wheelscroll.h"

#include <QtGui/QGuiApplication>
#include <QtGui/QStyleHints>
#include <QtGui/QWheelEvent>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <utility>

namespace {

// Chromium: a line is 100/3 px, a page 87.5 % of the view; 150 ms ease-out.
constexpr double kPixelsPerLine = 100.0 / 3.0;
constexpr double kPageFraction = 0.875;
constexpr double kDurationMs = 150.0;

// Flickable's public properties for one axis (QQuickFlickable is private
// API; these are its QML properties).
struct Axis
{
  const char* pos;
  const char* origin;
  const char* content;
  const char* startMargin;
  const char* endMargin;
};
constexpr Axis kVertical{"contentY", "originY", "contentHeight", "topMargin", "bottomMargin"};
constexpr Axis kHorizontal{"contentX", "originX", "contentWidth", "leftMargin", "rightMargin"};

const Axis& axisOf(bool vertical) { return vertical ? kVertical : kHorizontal; }

double prop(const QObject* o, const char* name) { return o->property(name).toDouble(); }

double viewSize(const QQuickItem* f, bool vertical) { return vertical ? f->height() : f->width(); }

// The positions the view can rest at: originY - topMargin up to
// originY + contentHeight + bottomMargin - height (Flickable's extents).
std::pair<double, double> range(const QQuickItem* f, bool vertical)
{
  const Axis& a = axisOf(vertical);
  const double origin = prop(f, a.origin);
  const double lo = origin - prop(f, a.startMargin);
  const double hi = origin + prop(f, a.content) + prop(f, a.endMargin) - viewSize(f, vertical);
  return {lo, std::max(lo, hi)};
}

// Flickable.flickableDirection: Auto (0) flicks an axis whose content size
// differs from the view's, AutoFlickIfNeeded (0xc) one whose content is
// larger, otherwise the Horizontal (1) / Vertical (2) bits.
bool canFlick(const QQuickItem* f, bool vertical)
{
  const int dir = f->property("flickableDirection").toInt();
  const double content = prop(f, axisOf(vertical).content);
  const double view = viewSize(f, vertical);
  if (dir == 0)
    return content != view;
  if (dir == 0xc)
    return content > view;
  return (dir & (vertical ? 2 : 1)) != 0;
}

}  // namespace

WheelScroll::WheelScroll(QObject* parent) : QObject(parent) { m_clock.start(); }

bool WheelScroll::active(QObject* flickable) const
{
  auto* item = qobject_cast<QQuickItem*>(flickable);
  return item && m_runs.contains(item);
}

bool WheelScroll::eventFilter(QObject* watched, QEvent* event)
{
  switch (event->type()) {
  case QEvent::Wheel:
    if (watched->inherits("QQuickFlickable"))
      return wheel(static_cast<QQuickItem*>(watched), static_cast<QWheelEvent*>(event));
    break;
  case QEvent::UpdateRequest:
    // The frame starts: place each view before polish and sync.
    if (!m_runs.isEmpty())
      if (auto* window = qobject_cast<QQuickWindow*>(watched))
        step(window);
    break;
  default:
    break;
  }
  return false;
}

bool WheelScroll::wheel(QQuickItem* item, QWheelEvent* event)
{
  // As Qt's own handler: a view that is not interactive takes no wheel.
  if (!item->property("interactive").toBool() || !item->window())
    return false;
  const QPoint angle = event->angleDelta();
  const QPoint pixel = event->pixelDelta();
  // A touchpad that reports pixels and phases: those pixels, 1:1.
  const bool pixels = !pixel.isNull() && event->phase() != Qt::NoScrollPhase;
  const QPoint raw = pixels ? pixel : angle;
  bool vertical;
  int units;
  if (raw.y() != 0) {
    // Shift turns a plain wheel sideways (Chromium on Windows).
    vertical = !(event->modifiers() & Qt::ShiftModifier) || raw.x() != 0;
    units = raw.y();
  } else if (raw.x() != 0) {
    vertical = false;
    units = raw.x();
  } else {
    return false;
  }
  if (!canFlick(item, vertical))
    return false;

  double px;
  bool animate;
  if (pixels) {
    px = -units;
    animate = false;
  } else {
    const int lines = QGuiApplication::styleHints()->wheelScrollLines();
    const double perNotch = lines > 0 && lines < 100 ? lines * kPixelsPerLine
                                                     : viewSize(item, vertical) * kPageFraction;
    px = -units / 120.0 * perNotch;
    // Whole notches ease; a precise wheel or a touchpad's small steps move
    // at once, as Chromium's precise deltas do.
    animate = units % 120 == 0;
  }

  const Axis& a = axisOf(vertical);
  const auto [lo, hi] = range(item, vertical);
  const double current = prop(item, a.pos);
  auto it = m_runs.find(item);
  if (it != m_runs.end() && it->vertical != vertical) {
    finish(item);
    it = m_runs.end();
  }
  const bool running = it != m_runs.end();
  const double base = running ? it->target : current;
  // Already at that end: the wheel goes on to the view around this one,
  // unless this one is still moving (it keeps the gesture, as Chromium's
  // latching does).
  if ((px < 0 && base <= lo + 0.5) || (px > 0 && base >= hi - 0.5)) {
    if (!running)
      return false;
    event->accept();
    return true;
  }
  const double target = std::clamp(base + px, lo, hi);

  if (!running) {
    Run run;
    run.item = item;
    run.vertical = vertical;
    run.last = current;
    it = m_runs.insert(item, run);
    // A view that goes mid-run leaves no entry behind.
    connect(item, &QObject::destroyed, this, [this, item] { m_runs.remove(item); },
            Qt::SingleShotConnection);
    QMetaObject::invokeMethod(item, "movementStarted");
    it = m_runs.find(item);
    if (it == m_runs.end()) {
      event->accept();
      return true;
    }
  }
  it->from = current;
  it->last = current;
  it->target = target;
  it->start = m_clock.elapsed();
  if (!animate) {
    // Straight there, within the run so `active` reads true meanwhile.
    item->setProperty(a.pos, target);
    finish(item);
  } else {
    watchWindow(item->window());
    item->window()->update();
  }
  event->accept();
  return true;
}

void WheelScroll::step(QQuickWindow* window)
{
  const qint64 now = m_clock.elapsed();
  const QList<QQuickItem*> items = m_runs.keys();
  for (QQuickItem* key : items) {
    auto it = m_runs.find(key);
    if (it == m_runs.end())
      continue;
    QQuickItem* f = it->item;
    if (!f) {
      m_runs.erase(it);
      continue;
    }
    if (f->window() != window)
      continue;
    const char* pos = axisOf(it->vertical).pos;
    // Something else moved the view (a jump, the end followed, the bar):
    // that wins.
    if (std::abs(prop(f, pos) - it->last) > 0.5) {
      finish(f);
      continue;
    }
    const double t = std::clamp((now - it->start) / kDurationMs, 0.0, 1.0);
    const double eased = 1.0 - std::pow(1.0 - t, 3.0);
    // The extents again: a ListView's change as its rows load.
    const auto [lo, hi] = range(f, it->vertical);
    const double target = std::clamp(it->target, lo, hi);
    f->setProperty(pos, it->from + (target - it->from) * eased);
    // What ran on that change may have ended the run or the view.
    it = m_runs.find(key);
    if (it == m_runs.end())
      continue;
    if (!it->item) {
      m_runs.erase(it);
      continue;
    }
    it->last = prop(f, pos);
    if (t >= 1.0)
      finish(f);
  }
}

void WheelScroll::finish(QQuickItem* item)
{
  if (!m_runs.remove(item))
    return;
  disconnect(item, &QObject::destroyed, this, nullptr);
  // Back inside if the extents shrank meanwhile.
  QMetaObject::invokeMethod(item, "returnToBounds");
  QMetaObject::invokeMethod(item, "movementEnded");
}

void WheelScroll::watchWindow(QQuickWindow* window)
{
  if (m_windows.contains(window))
    return;
  // The next frame is asked for once this one has begun: an update asked
  // before the basic render loop starts a frame is cleared by it, and that
  // frame would not present (qsgrenderloop.cpp renderWindow).
  m_windows.insert(window, connect(window, &QQuickWindow::afterAnimating, this, [this, window] {
    for (const Run& run : std::as_const(m_runs))
      if (run.item && run.item->window() == window) {
        window->update();
        return;
      }
  }));
  connect(window, &QObject::destroyed, this, [this, window] { m_windows.remove(window); });
}
