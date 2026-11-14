#pragma once

// The colour picker's eyedropper (Chromium's EyeDropperView, which the
// web's <input type="color"> popup opens): a colour from anywhere on screen.
//
//   Eyedropper.start()    puts a transparent window over every screen, with
//                         a magnifier loupe at the pointer; a left click
//                         picks the pixel under it (`picked("#RRGGBB")`),
//                         Esc, a right click or leaving the app cancels
//                         (`cancelled()`)
//   Eyedropper.cancel()   ends a pick without a colour
//   Eyedropper.active     a pick is under way
//
// The screen is read only while a pick the user started is under way, and
// only a small square at the pointer (QScreen::grabWindow). The overlay is
// excluded from capture (WDA_EXCLUDEFROMCAPTURE), so the loupe never sees
// itself.
//
// YAP_TEST_EYEDROPPER (tests): "#RRGGBB" is picked 150 ms after start, "-"
// cancels; no window is made and the screen is never read.

#include <QtCore/QList>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QString>
#include <QtGui/QColor>
#include <QtGui/QImage>

class QWindow;
class EyedropperOverlay;

class Eyedropper : public QObject
{
  Q_OBJECT
  Q_PROPERTY(bool active READ active NOTIFY activeChanged)

public:
  explicit Eyedropper(QObject* parent = nullptr);
  ~Eyedropper() override;

  bool active() const { return m_active; }

  Q_INVOKABLE void start();
  Q_INVOKABLE void cancel();

  // `#RRGGBB` (upper case) for a colour; its alpha is ignored.
  static QString hex(const QColor& color);

signals:
  void picked(const QString& hex);
  void cancelled();
  void activeChanged();

private:
  friend class EyedropperOverlay;

  void moved(EyedropperOverlay* from);
  void finish(const QString& hex);
  void close();

  QList<EyedropperOverlay*> m_overlays;
  QPointer<QWindow> m_previous;
  bool m_active = false;
};
