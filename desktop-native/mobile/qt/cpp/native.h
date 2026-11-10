#pragma once

// `Native` (QML singleton, Yap.Mobile): the platform behaviour Qt Quick
// does not give by itself, each a thin call into the Android shim
// (android/src/dev/yap/qtproto/NativeShim.java). One object so the shared
// QML has one place to ask; an iOS build would back the same properties
// with a Swift/ObjC shim.
//
//   back gesture   OnBackAnimationCallback: progress, cancel, invoke
//   keyboard       the IME inset, frame by frame (WindowInsetsAnimation)
//   haptics        View.performHapticFeedback
//   share          the system share sheet (ACTION_SEND)
//   text size      Configuration.fontScale
//   motion         the animator duration scale (Remove animations)
//   accent         Material You's system accent (Android 12+)
//   fling          ViewConfiguration's fling speed and a matching friction

#include <QtCore/QObject>
#include <QtGui/QColor>

class Native : public QObject
{
  Q_OBJECT
  Q_PROPERTY(bool handlesBack READ handlesBack WRITE setHandlesBack NOTIFY handlesBackChanged)
  Q_PROPERTY(qreal imeHeight READ imeHeight NOTIFY imeHeightChanged)
  Q_PROPERTY(qreal fontScale READ fontScale NOTIFY configurationChanged)
  Q_PROPERTY(bool reducedMotion READ reducedMotion NOTIFY configurationChanged)
  Q_PROPERTY(QColor accent READ accent NOTIFY configurationChanged)
  Q_PROPERTY(qreal maxFlingVelocity READ maxFlingVelocity CONSTANT)
  Q_PROPERTY(qreal flingDeceleration READ flingDeceleration CONSTANT)

public:
  enum Haptic { Tick, LongPress, Confirm, Reject };
  Q_ENUM(Haptic)

  explicit Native(QObject* parent = nullptr);
  ~Native() override;
  static Native* instance();

  bool handlesBack() const { return m_handlesBack; }
  void setHandlesBack(bool on);
  qreal imeHeight() const { return m_imeHeight; }
  qreal fontScale() const { return m_fontScale; }
  bool reducedMotion() const { return m_reducedMotion; }
  QColor accent() const { return m_accent; }
  qreal maxFlingVelocity() const { return m_maxFling; }
  qreal flingDeceleration() const { return m_deceleration; }

  Q_INVOKABLE void haptic(Haptic kind) const;
  Q_INVOKABLE void share(const QString& text) const;
  Q_INVOKABLE void copy(const QString& text) const;
  // Back at the root: leave as the system would (the task goes back).
  Q_INVOKABLE void leave() const;
  // Commits the core's snapshot cache (the app going to the background).
  Q_INVOKABLE void flushCache() const;

  // From the shim, on the GUI thread.
  void onBackProgress(qreal progress);
  void onBackCancelled();
  void onBackInvoked();
  void onIme(qreal dp);
  void readConfiguration();

signals:
  void handlesBackChanged();
  void imeHeightChanged();
  void configurationChanged();
  void backProgressed(qreal progress);
  void backCancelled();
  void backInvoked();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  bool m_handlesBack = false;
  qreal m_imeHeight = 0;
  qreal m_fontScale = 1;
  bool m_reducedMotion = false;
  QColor m_accent;
  qreal m_maxFling = 8000;
  qreal m_deceleration = 1500;
};

// The shared desktop QML's `Yap.Chrome` singletons, mobile side.
class MobileShell : public QObject
{
  Q_OBJECT

public:
  using QObject::QObject;
  // The desktop trims GPU memory after a popup closes; nothing to do here.
  Q_INVOKABLE void releaseGraphicsSoon() const {}
};
