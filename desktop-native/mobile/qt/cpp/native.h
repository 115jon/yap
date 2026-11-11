#pragma once

// `Native` (QML singleton, Yap.Mobile): the platform behaviour Qt Quick
// does not give by itself, each a thin call into the Android shim
// (android/src/dev/yap/qtproto/NativeShim.java). One object so the shared
// QML has one place to ask; an iOS build would back the same properties
// with a Swift/ObjC shim.
//
//   back gesture   OnBackAnimationCallback: progress, cancel, invoke; a
//                  sheet or viewer on top takes it as a back layer
//   system bars    light or dark status and navigation bar icons
//   keyboard       the IME inset, frame by frame (WindowInsetsAnimation)
//   haptics        View.performHapticFeedback
//   share          the system share sheet (ACTION_SEND)
//   text size      Configuration.fontScale
//   motion         the animator duration scale (Remove animations)
//   accent         Material You's system accent (Android 12+)
//   fling          ViewConfiguration's fling speed and a matching friction
//   media          the photo picker (ACTION_PICK_IMAGES, GET_CONTENT before
//                  13), each item copied into the cache for the core

#include <QtCore/QList>
#include <QtCore/QObject>
#include <QtCore/QStringList>
#include <QtCore/QPointer>
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
  // Picked files are being copied in (a long video takes a moment).
  Q_PROPERTY(bool pickingMedia READ pickingMedia NOTIFY pickingMediaChanged)
  // Dark system bar icons, for a light ground (the light themes). While
  // anything holds the bars dark (holdDarkBars: a viewer, a heavy scrim) the
  // icons stay light whatever this says.
  Q_PROPERTY(bool lightBars READ lightBars WRITE setLightBars NOTIFY lightBarsChanged)

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
  bool pickingMedia() const { return m_pickingMedia; }
  bool lightBars() const { return m_lightBars; }
  void setLightBars(bool on);

  // A layer over the page (a sheet, a viewer) takes the back gesture while
  // it is up: the newest layer gets `backProgressed(progress)`,
  // `backCancelled()` and `backInvoked()` called on it (QML functions), the
  // signals below stay quiet, and the gesture is taken even at the root.
  Q_INVOKABLE void pushBackLayer(QObject* layer);
  Q_INVOKABLE void popBackLayer(QObject* layer);
  // Light system bar icons while `holder` lives and holds (a dark ground
  // over a light theme).
  Q_INVOKABLE void holdDarkBars(QObject* holder);
  Q_INVOKABLE void releaseDarkBars(QObject* holder);

  Q_INVOKABLE void haptic(Haptic kind) const;
  Q_INVOKABLE void share(const QString& text) const;
  Q_INVOKABLE void copy(const QString& text) const;
  // Back at the root: leave as the system would (the task goes back).
  Q_INVOKABLE void leave() const;
  // Commits the core's snapshot cache (the app going to the background).
  Q_INVOKABLE void flushCache() const;
  // The system photo picker; the copies' paths arrive as mediaPicked. An
  // item over `limitBytes` (the account's attachment limit) is not copied.
  Q_INVOKABLE void pickMedia(qreal limitBytes);

  // From the shim, on the GUI thread.
  void onBackProgress(qreal progress);
  void onBackCancelled();
  void onBackInvoked();
  void onIme(qreal dp);
  void readConfiguration();
  void onMediaCopying();
  void onMediaPicked(const QStringList& paths, int failed);

signals:
  void handlesBackChanged();
  void lightBarsChanged();
  void imeHeightChanged();
  void configurationChanged();
  void backProgressed(qreal progress);
  void backCancelled();
  void backInvoked();
  void pickingMediaChanged();
  // Local copies of what was picked (none: cancelled); `failed` items could
  // not be read.
  void mediaPicked(const QStringList& paths, int failed);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  QObject* backLayer();
  void applyBack();
  void applyBars();

  QList<QPointer<QObject>> m_backLayers;
  QList<QPointer<QObject>> m_darkHolds;
  bool m_backTaken = false;
  bool m_lightBars = false;
  // What the system bars were last told: -1 nothing yet, else `light`.
  int m_barsLight = -1;
  bool m_handlesBack = false;
  qreal m_imeHeight = 0;
  qreal m_fontScale = 1;
  bool m_reducedMotion = false;
  QColor m_accent;
  qreal m_maxFling = 8000;
  qreal m_deceleration = 1500;
  bool m_pickingMedia = false;
  // A picker was started and its result has not come back. Qt keeps one
  // callback per request code, so a second picker must not start meanwhile.
  bool m_pickerOpen = false;
  // The app went to the background since (the picker did open).
  bool m_leftSincePick = false;
  qint64 m_pickStartedMs = 0;
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
