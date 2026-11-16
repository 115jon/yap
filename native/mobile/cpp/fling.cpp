#include "fling.h"

#include <QtCore/QElapsedTimer>
#include <QtCore/QMetaObject>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtGui/QEventPoint>
#include <QtGui/QPointingDevice>
#include <QtQuick/QQuickWindow>
#include <QtQuick/private/qquickdeliveryagent_p_p.h>
#include <QtQuick/private/qquickflickable_p.h>
#include <QtQuick/private/qquickflickable_p_p.h>
#include <QtQuick/private/qquickwindow_p.h>

#include <cmath>
#include <cstring>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniEnvironment>
#include <QtCore/QJniObject>
#include <QtCore/qnativeinterface.h>
#include <sys/system_properties.h>
#endif

namespace {

#ifdef Q_OS_ANDROID
constexpr char kShim[] = "dev/yap/qtproto/FlingShim";
#endif

// OverScroller's fling (SplineOverScroller), in dp: how far a fling at `v`
// dp/s goes and for how long, with ViewConfiguration's scroll friction.
constexpr double kFriction = 0.015;
constexpr double kInflexion = 0.35;
constexpr double kPhysical = 9.80665 * 39.37 * 160.0 * 0.84;
const double kRate = std::log(0.78) / std::log(0.9);

double splineLog(double v)
{
  return std::log(kInflexion * std::abs(v) / (kFriction * kPhysical));
}
double splineDistance(double v)
{
  return kFriction * kPhysical * std::exp(kRate / (kRate - 1.0) * splineLog(v));
}
double splineDuration(double v)
{
  return 1000.0 * std::exp(splineLog(v) / (kRate - 1.0));
}

// A list that snaps (to an item, or a page) moves at its own speed to the
// next stop: it keeps Qt's fling.
bool snaps(const QQuickFlickable* flickable)
{
  return flickable->property("snapMode").toInt() != 0 || flickable->property("highlightRangeMode").toInt() == 2;
}

struct Lift
{
  bool valid = false;
  qreal x = 0;
  qreal y = 0;
};

class Fling : public QObject
{
public:
  explicit Fling(QQuickWindow* window)
    : QObject(window)
    , m_window(window)
  {
#ifdef Q_OS_ANDROID
    char mode[PROP_VALUE_MAX] = {};
    __system_property_get("debug.yap.fling", mode);
    m_log = std::strcmp(mode, "log") == 0 || std::strcmp(mode, "qt") == 0;
    m_qt = std::strcmp(mode, "qt") == 0;
    QJniObject::callStaticMethod<void>(kShim, "install", "(Landroid/app/Activity;Z)V",
                                       QNativeInterface::QAndroidApplication::context().object(), jboolean(m_log));
    m_max = QJniObject::callStaticMethod<jfloat>(kShim, "maxVelocity");
    m_min = QJniObject::callStaticMethod<jfloat>(kShim, "minVelocity");
#endif
    // A constant friction that goes as far as OverScroller from a typical
    // flick, for the flings Qt still starts itself.
    m_typical = 2500.0 * 2500.0 / (2.0 * splineDistance(2500.0));
    if (m_log)
      qInfo("[fling] %s flings, max %.0f dp/s, min %.0f dp/s", m_qt ? "Qt's" : "Android's", m_max, m_min);
    window->installEventFilter(this);
  }

protected:
  bool eventFilter(QObject*, QEvent* event) override
  {
    switch (event->type()) {
    case QEvent::TouchBegin:
      m_described = false;
      ++m_touches;
      break;
    case QEvent::TouchUpdate:
      if (!m_described) {
        // The press went out already: who has the touch, before it moves.
        m_described = true;
        auto* touch = static_cast<QTouchEvent*>(event);
        prepare(touch);
        if (m_log)
          describe(touch, "move");
      }
      break;
    case QEvent::TouchEnd:
      lifted(static_cast<QTouchEvent*>(event));
      break;
    case QEvent::TouchCancel:
      take();
      break;
    default:
      break;
    }
    return false;
  }

private:
  static Lift take()
  {
    Lift lift;
#ifdef Q_OS_ANDROID
    const QJniObject array = QJniObject::callStaticMethod<jfloatArray>(kShim, "take");
    if (!array.isValid())
      return lift;
    QJniEnvironment env;
    jfloat values[3] = {};
    env->GetFloatArrayRegion(array.object<jfloatArray>(), 0, 3, values);
    lift.valid = values[0] > 0;
    lift.x = values[1];
    lift.y = values[2];
#endif
    return lift;
  }

  // Every list that may take this touch (the grabbers and what holds
  // them): the content stays under the finger from the step the drag
  // starts (Qt otherwise holds it back by however far that step went past
  // the threshold, up to a whole frame's travel in a quick flick), and
  // Qt's own fling, where it still runs, has Android's top speed.
  void prepare(QTouchEvent* event)
  {
    if (event->pointCount() < 1)
      return;
    const QEventPoint& point = event->points().first();
    QList<QObject*> grabbers { event->exclusiveGrabber(point) };
    for (const auto& passive : event->passiveGrabbers(point))
      grabbers << passive.data();
    for (QObject* grabber : std::as_const(grabbers)) {
      QQuickItem* item = qobject_cast<QQuickItem*>(grabber);
      if (!item && grabber)
        item = qobject_cast<QQuickItem*>(grabber->parent());
      for (; item; item = item->parentItem()) {
        auto* flickable = qobject_cast<QQuickFlickable*>(item);
        if (!flickable)
          continue;
        flickable->setSynchronousDrag(true);
        if (snaps(flickable))
          continue;
        // Each fling of ours sets its own friction; Qt's start from this.
        flickable->setMaximumFlickVelocity(m_max);
        flickable->setFlickDeceleration(m_typical);
      }
    }
  }

  static QString nameOf(QObject* object)
  {
    if (!object)
      return QStringLiteral("none");
    QString name = QString::fromLatin1(object->metaObject()->className());
    if (!object->objectName().isEmpty())
      name += u'(' + object->objectName() + u')';
    if (auto* item = qobject_cast<QQuickItem*>(object->parent()))
      name += QStringLiteral(" in ") + QString::fromLatin1(item->metaObject()->className());
    return name;
  }

  void describe(QTouchEvent* event, const char* when)
  {
    if (event->pointCount() < 1)
      return;
    const QEventPoint& point = event->points().first();
    QStringList passive;
    for (const auto& grabber : event->passiveGrabbers(point))
      passive << nameOf(grabber);
    qInfo("[fling] %s at %.0f,%.0f: exclusive %s, passive [%s]", when, point.scenePosition().x(),
          point.scenePosition().y(), qPrintable(nameOf(event->exclusiveGrabber(point))),
          qPrintable(passive.join(QStringLiteral(", "))));
  }

  // Before Qt has the lift: which list had the drag, and how fast the
  // finger went. The fling starts once Qt is done with the lift.
  void lifted(QTouchEvent* event)
  {
    const Lift lift = take();
    if (event->pointCount() != 1)
      return;
    // The last move may still wait for the next frame (Qt hands one move
    // per frame to the items): it goes out first, as Qt sends it before
    // the lift anyway, so the drag it may start is seen here.
    if (auto* agent = QQuickWindowPrivate::get(m_window)->deliveryAgentPrivate(); agent && agent->delayedTouch)
      agent->deliverDelayedTouchEvent();
    const QEventPoint& point = event->points().first();
    if (m_log)
      describe(event, "lift");
    auto* flickable = qobject_cast<QQuickFlickable*>(event->exclusiveGrabber(point));
    if (!flickable || !flickable->isDragging() || snaps(flickable))
      return;
    const bool vertical = flickable->isDraggingVertically();
    const bool horizontal = flickable->isDraggingHorizontally();
    const QPointF at = point.scenePosition();
    // The finger's speed in the list's own coordinates.
    const QPointF speed = lift.valid
      ? flickable->mapFromScene(at + QPointF(lift.x, lift.y)) - flickable->mapFromScene(at)
      : QPointF();
    const QPointF moved = point.scenePosition() - point.scenePressPosition();
    QPointer<QQuickFlickable> target(flickable);
    const quint64 touch = m_touches;
    QMetaObject::invokeMethod(this, [this, target, touch, lift, speed, vertical, horizontal, moved] {
      // Not under a finger that came down since.
      if (target && touch == m_touches)
        fling(target, lift.valid, horizontal ? speed.x() : 0, vertical ? speed.y() : 0, moved);
    }, Qt::QueuedConnection);
  }

  void fling(QQuickFlickable* flickable, bool valid, qreal vx, qreal vy, QPointF moved)
  {
    if (m_log)
      follow(flickable, moved);
    // A new touch took it meanwhile, or it is past an edge (Qt brings it
    // back; Android's lists do not fling from there either).
    if (flickable->isDragging() || flickable->verticalOvershoot() != 0 || flickable->horizontalOvershoot() != 0)
      return;
    const qreal qtSpeed = std::hypot(flickable->horizontalVelocity(), flickable->verticalVelocity());
    const qreal speed = std::min<qreal>(std::hypot(vx, vy), m_max);
    if (!m_qt && valid && speed < m_min && flickable->isFlicking()) {
      // Too slow for Android to fling; Qt's estimate may still have.
      flickable->cancelFlick();
      if (m_log)
        qInfo("[fling] lift %.0f dp/s (Android %.0f): no fling", qtSpeed, speed);
      return;
    }
    if (m_qt || !valid || speed < m_min) {
      if (m_log)
        qInfo("[fling] lift %.0f dp/s (Android %.0f): %s", qtSpeed, speed,
              m_qt ? "Qt's fling" : "no fling of ours");
      if (m_log && flickable->isFlicking())
        watch(flickable, qtSpeed, flickable->flickDeceleration());
      return;
    }
    const qreal scale = speed / std::hypot(vx, vy);
    const qreal deceleration = speed * speed / (2.0 * splineDistance(speed));
    flickable->setMaximumFlickVelocity(m_max);
    flickable->setFlickDeceleration(deceleration);
    flickable->flick(vx * scale, vy * scale);
    if (m_log) {
      qInfo("[fling] lift: Qt read %.0f dp/s, Android %.0f dp/s; Android goes %.0f dp in %.0f ms", qtSpeed, speed,
            splineDistance(speed), splineDuration(speed));
      watch(flickable, speed, deceleration);
    }
  }

  // How closely the list followed the finger: what it moved against the
  // finger's travel (past the drag threshold, unless it drags in step).
  void follow(QQuickFlickable* flickable, QPointF moved)
  {
    auto* d = QQuickFlickablePrivate::get(flickable);
    const bool vertical = std::abs(moved.y()) >= std::abs(moved.x());
    const auto& axis = vertical ? d->vData : d->hData;
    const qreal finger = vertical ? moved.y() : moved.x();
    const qreal content = axis.move.value() - axis.pressPos;
    const qreal held = flickable->synchronousDrag() ? 0 : axis.dragStartOffset;
    qInfo("[fling] follow: finger %.1f dp, list %.1f dp, held back %.1f dp, off by %.1f dp", finger, content, held,
          content - (finger - held));
  }

  // Where the fling ended, how long it took, against what it set off for.
  void watch(QQuickFlickable* flickable, qreal speed, qreal deceleration)
  {
    const QPointF from(flickable->contentX(), flickable->contentY());
    QElapsedTimer clock;
    clock.start();
    // One report per fling; a list flung again before it stopped reports
    // from the first fling's start.
    connect(flickable, &QQuickFlickable::movementEnded, this, [flickable, from, clock, speed, deceleration] {
      const qreal went = std::hypot(flickable->contentX() - from.x(), flickable->contentY() - from.y());
      qInfo("[fling] went %.0f dp in %lld ms; set off at %.0f dp/s for %.0f dp in %.0f ms", went,
            static_cast<long long>(clock.elapsed()), speed, speed * speed / (2 * deceleration),
            1000 * speed / deceleration);
    }, Qt::SingleShotConnection);
  }

  QQuickWindow* m_window;
  quint64 m_touches = 0;
  bool m_log = false;
  bool m_qt = false;
  bool m_described = false;
  float m_max = 8000;
  float m_min = 50;
  qreal m_typical = 3277;
};

} // namespace

void installFling(QQuickWindow* window)
{
  new Fling(window);
}
