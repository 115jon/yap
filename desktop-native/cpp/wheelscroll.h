#pragma once

// Mouse-wheel scrolling the way Chromium does it, for every Flickable
// (ListView, GridView, ScrollView, plain Flickable): a notch moves
// wheelScrollLines x 100/3 px (100 px at Windows' default 3 lines; a page
// setting moves 87.5 % of the view), eased out over 150 ms toward a target
// that further notches extend, and a precise delta (touchpad, free-spinning
// wheel) moves 1:1 at once. At its end a view lets the wheel go on to the
// view around it (scroll chaining), as Qt's own handler does.
//
// Qt's own handler flicks instead: ~72 px a notch with a deceleration
// tail, ticked by QUnifiedTimer's 16 ms timer, which under the basic render
// loop (main.cpp, Qt 6.11 move) steps unevenly against the presents that
// block the GUI thread. Here each frame's position is set from wall-clock
// time when the window's UpdateRequest arrives, before polish, so a
// ListView refills in the same frame it moves.
//
// One application event filter; QML reads `WheelScroll.active(view)` where
// it asks `view.moving` (the wheel's movement is not Flickable's own, so
// `moving` stays false; movementStarted and movementEnded are emitted).

#include <QtCore/QElapsedTimer>
#include <QtCore/QHash>
#include <QtCore/QObject>
#include <QtCore/QPointer>

class QQuickItem;
class QQuickWindow;
class QWheelEvent;

class WheelScroll : public QObject
{
  Q_OBJECT

public:
  explicit WheelScroll(QObject* parent = nullptr);

  // Whether a wheel scroll is moving this Flickable now.
  Q_INVOKABLE bool active(QObject* flickable) const;

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  struct Run
  {
    QPointer<QQuickItem> item;
    bool vertical = true;
    double from = 0;
    double target = 0;
    double last = 0;  // what we set last; anything else means someone else moved it
    qint64 start = 0;
  };

  bool wheel(QQuickItem* item, QWheelEvent* event);
  void step(QQuickWindow* window);
  void finish(QQuickItem* item);
  void watchWindow(QQuickWindow* window);

  QHash<QQuickItem*, Run> m_runs;
  QHash<QQuickWindow*, QMetaObject::Connection> m_windows;
  QElapsedTimer m_clock;
};
