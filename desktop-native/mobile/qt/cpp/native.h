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
//   push           FCM's token for the core, a notification's tap, Reply and
//                  Mark as read, the conversation on screen (its pushes stay
//                  quiet) and the notification permission (cpp/push.cpp,
//                  android/src/dev/yap/qtproto/push)
//   voice          the microphone permission, the call's audio routes
//                  (speaker, earpiece, headset, Bluetooth) and what the call
//                  notification shows (cpp/voice.cpp,
//                  android/src/dev/yap/qtproto/voice)

#include <QtCore/QList>
#include <QtCore/QObject>
#include <QtCore/QStringList>
#include <QtCore/QPointer>
#include <QtCore/QVariantList>
#include <QtCore/QVariantMap>
#include <QtGui/QColor>

class Native : public QObject
{
  Q_OBJECT
  Q_PROPERTY(bool handlesBack READ handlesBack WRITE setHandlesBack NOTIFY handlesBackChanged)
  Q_PROPERTY(qreal imeHeight READ imeHeight NOTIFY imeHeightChanged)
  // The bottom system gesture zone Qt's safe area leaves out (a phone with
  // gesture navigation and no navigation bar inset): bottom controls keep
  // clear of max(SafeArea bottom, this). And how far the window reaches
  // below what shows of it (a freeform window's caption bar pushes it down
  // without shrinking it): the shell's content ends this far above the
  // bottom. Both in dp, 0 when none.
  Q_PROPERTY(qreal gestureInset READ gestureInset NOTIFY edgesChanged)
  Q_PROPERTY(qreal clippedBottom READ clippedBottom NOTIFY edgesChanged)
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
  // Message notifications can show (the permission on API 33+, and the
  // app's switch in the system settings). Read again when the app returns.
  Q_PROPERTY(bool notificationsEnabled READ notificationsEnabled NOTIFY notificationsEnabledChanged)
  // The conversation on screen with the app in front (empty: none): its
  // pushes are not shown, and its notification goes. The first one set
  // asks for the notification permission (API 33+, once).
  Q_PROPERTY(QString activeConversation READ activeConversation WRITE setActiveConversation NOTIFY activeConversationChanged)
  // Where a running call can play and record, `[{id, name, active}]`:
  // Bluetooth and a wired headset when there, the speaker, the earpiece.
  // Empty outside a call.
  Q_PROPERTY(QVariantList audioRoutes READ audioRoutes NOTIFY audioRoutesChanged)

public:
  enum Haptic { Tick, LongPress, Confirm, Reject };
  Q_ENUM(Haptic)

  explicit Native(QObject* parent = nullptr);
  ~Native() override;
  static Native* instance();

  bool handlesBack() const { return m_handlesBack; }
  void setHandlesBack(bool on);
  qreal imeHeight() const { return m_imeHeight; }
  qreal gestureInset() const { return m_gestureInset; }
  qreal clippedBottom() const { return m_clippedBottom; }
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
  bool notificationsEnabled() const { return m_notificationsEnabled; }
  QString activeConversation() const { return m_activeConversation; }
  void setActiveConversation(const QString& channelId);

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
  // Turns notifications on: the permission dialog while it can still show,
  // else the app's notification settings.
  Q_INVOKABLE void requestNotifications();
  // The conversation a notification's tap asked for, once (`channelId`,
  // `serverId`, and a DM's `userId`; empty: none). `openRequested` says one
  // is waiting.
  Q_INVOKABLE QVariantMap takeOpenRequest();
  // Signed out: the account's notifications and conversation shortcuts go.
  Q_INVOKABLE void forgetNotifications() const;

  QVariantList audioRoutes() const { return m_audioRoutes; }
  // Asks for the microphone (the system dialog while it can still show);
  // the answer arrives as microphoneAnswered, at once when already decided.
  Q_INVOKABLE void requestMicrophone();
  // The app's page in the system settings (a microphone refused for good).
  Q_INVOKABLE void openAppSettings() const;
  // Routes the running call (an id from audioRoutes).
  Q_INVOKABLE void setAudioRoute(const QString& id) const;
  // What the call notification shows: the channel, and the mic and deafen
  // state its Mute action starts from.
  Q_INVOKABLE void setCallState(const QString& channel, bool muted, bool deafened) const;
  // The user set the mic or deafen themselves: an interruption (a phone
  // call) that deafened the call no longer undoes it when it ends.
  Q_INVOKABLE void callTouched() const;

  // From the shim, on the GUI thread.
  void onBackProgress(qreal progress);
  void onBackCancelled();
  void onBackInvoked();
  void onIme(qreal dp);
  void onEdges(qreal gestureBottom, qreal clippedBottom);
  void readConfiguration();
  void onMediaCopying();
  void onMediaPicked(const QStringList& paths, int failed);
  void onOpenRequest(const QString& channelId, const QString& serverId, const QString& userId);
  void onAudioRoutes(const QByteArray& json);

signals:
  void handlesBackChanged();
  void lightBarsChanged();
  void imeHeightChanged();
  void edgesChanged();
  void configurationChanged();
  void backProgressed(qreal progress);
  void backCancelled();
  void backInvoked();
  void pickingMediaChanged();
  // Local copies of what was picked (none: cancelled); `failed` items could
  // not be read.
  void mediaPicked(const QStringList& paths, int failed);
  void notificationsEnabledChanged();
  void activeConversationChanged();
  void openRequested();
  void audioRoutesChanged();
  void microphoneAnswered(bool granted);

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
  // cpp/push.cpp.
  void installPush();
  void readNotificationsEnabled();
  // cpp/voice.cpp.
  void installVoice();

  bool m_handlesBack = false;
  // Back reaches the app through the shim's OnBackInvokedCallback (API 33+).
  bool m_backByCallback = false;
  qreal m_imeHeight = 0;
  qreal m_gestureInset = 0;
  qreal m_clippedBottom = 0;
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
  bool m_notificationsEnabled = false;
  QString m_activeConversation;
  QVariantMap m_openRequest;
  QVariantList m_audioRoutes;
  bool m_askingMicrophone = false;
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
