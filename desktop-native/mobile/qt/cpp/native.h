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
//   share          the system share sheet (ACTION_SEND), with a title
//   text size      Configuration.fontScale
//   motion         the animator duration scale (Remove animations)
//   accent         Material You's system accent (Android 12+)
//   fling          ViewConfiguration's fling speed and a matching friction
//   media          the composer's attach menu: the photo picker
//                  (ACTION_PICK_IMAGES, GET_CONTENT before 13), the files
//                  picker (OPEN_DOCUMENT) and the system camera, each item
//                  copied into the cache for the core; Paste with files or a
//                  picture on the clipboard, and pictures the keyboard
//                  inserts (commitContent, org/qtproject/qt/android/
//                  YapEditText.java); the photo picker for one picture
//                  (an avatar, a banner); and a picture saved into the
//                  gallery (MediaStore, Pictures/Yap)
//   push           FCM's token for the core, a notification's tap, Reply and
//                  Mark as read, the conversation on screen (its pushes stay
//                  quiet) and the notification permission (cpp/push.cpp,
//                  android/src/dev/yap/qtproto/push)
//   links          the web's invite and chat links and yap:// links the
//                  app was opened with, held until the QML takes them
//                  (cpp/links.cpp, android/src/dev/yap/qtproto/LinkShim)
//   voice          the microphone permission, the call's audio routes
//                  (speaker, earpiece, headset, Bluetooth) and what the call
//                  notification shows (cpp/voice.cpp,
//                  android/src/dev/yap/qtproto/voice)
//   calls          a DM call ringing this phone: the ring from a push or
//                  the core, its ringtone or notification, Answer and
//                  Decline outside the app (cpp/calls.cpp,
//                  android/src/dev/yap/qtproto/push/IncomingCalls.java)

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
  // The DM call ringing this phone as the shim knows it, `{callId,
  // channelId, callerId, callerName, callerAvatar}` (empty: none): also a
  // ring the core never saw (the app started from its push).
  Q_PROPERTY(QVariantMap incomingCall READ incomingCall NOTIFY incomingCallChanged)

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
  // `title` heads the share sheet (empty: the system's own).
  Q_INVOKABLE void share(const QString& text, const QString& title = QString()) const;
  Q_INVOKABLE void copy(const QString& text) const;
  // Back at the root: leave as the system would (the task goes back).
  Q_INVOKABLE void leave() const;
  // Commits the core's snapshot cache (the app going to the background).
  Q_INVOKABLE void flushCache() const;
  // The system photo picker; the copies' paths arrive as mediaPicked. An
  // item over `limitBytes` (the account's attachment limit) is not copied.
  Q_INVOKABLE void pickMedia(qreal limitBytes);
  // The files picker (any type, several), the same way.
  Q_INVOKABLE void pickFiles(qreal limitBytes);
  // The system camera; the photo's path arrives as mediaPicked. 0: it
  // opened; 1: no camera app; 2: the camera permission was asked for (try
  // again); 3: another pick is under way, or it could not start.
  Q_INVOKABLE int takePhoto();
  // A photo the camera took after Android killed the app behind it (its
  // path, once; empty when there is none), for the composer's tray.
  Q_INVOKABLE QString takeLeftoverPhoto();
  // Paste: whether the clipboard holds files or a picture, and copying them
  // in (their paths arrive as contentReceived; false: none, a text paste).
  Q_INVOKABLE bool clipboardHasFiles() const;
  Q_INVOKABLE bool pasteFiles(qreal limitBytes);
  // While the composer's field has focus the keyboard may insert pictures
  // (they arrive as contentReceived), held to `limitBytes` as a pick is.
  Q_INVOKABLE void setAcceptContent(bool on, qreal limitBytes);
  // Swaps in the text view that takes the keyboard's pictures (main.cpp,
  // once the window shows).
  void installContent() const;
  // The photo picker for one picture (an avatar, a banner), no permission
  // needed; its copy arrives as mediaPicked too.
  Q_INVOKABLE void pickImage(qreal limitBytes);
  // Save image: where to download a picture named `name` first (a fresh
  // folder under the cache; empty off Android), then the download into the
  // gallery's Pictures/Yap. The outcome arrives as imageSaved.
  Q_INVOKABLE QString saveTarget(const QString& name) const;
  Q_INVOKABLE void saveToGallery(const QString& path, const QString& mime) const;
  // Turns notifications on: the permission dialog while it can still show,
  // else the app's notification settings.
  Q_INVOKABLE void requestNotifications();
  // The conversation a notification's tap asked for, once (`channelId`,
  // `serverId`, and a DM's `userId`; empty: none). `openRequested` says one
  // is waiting.
  Q_INVOKABLE QVariantMap takeOpenRequest();
  // The link the app was last opened with, once (empty: none).
  // `linkRequested` says one is waiting.
  Q_INVOKABLE QString takeLinkRequest();
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

  QVariantMap incomingCall() const { return m_incomingCall; }
  // The core's ring (CALL_RING; the keys of incomingCall): rings once per
  // call id with a push's, the ringtone in front, else the notification.
  Q_INVOKABLE void ringIncoming(const QVariantMap& call) const;
  // The core's ring stopped: `reason` its end (`accepted` when answered).
  Q_INVOKABLE void stopIncoming(const QString& callId, const QString& reason) const;
  // The app's sheet answered or declined: the ringtone and notification
  // stop. `tellServer`: a ring the core did not see, answered over REST.
  Q_INVOKABLE void answerIncoming(const QString& callId, bool tellServer) const;
  Q_INVOKABLE void declineIncoming(const QString& callId, bool tellServer) const;
  // The call answered outside the app (its notification, the lock
  // screen), once; `callAnswered` says one is waiting.
  Q_INVOKABLE QVariantMap takeAnsweredCall();

  // From the shim, on the GUI thread.
  void onBackProgress(qreal progress);
  void onBackCancelled();
  void onBackInvoked();
  void onIme(qreal dp);
  void onEdges(qreal gestureBottom, qreal clippedBottom);
  void readConfiguration();
  void onMediaCopying();
  void onMediaPicked(const QStringList& paths, int failed);
  void onContentReceived(const QStringList& paths, int failed);
  void onImageSaved(const QString& path, int result);
  void onOpenRequest(const QString& channelId, const QString& serverId, const QString& userId);
  void onLinkRequest(const QString& url);
  void onAudioRoutes(const QByteArray& json);
  void onRing(const QByteArray& json);
  void onRingEnded(const QString& callId);
  void onAnswered();

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
  // Files from a paste or the keyboard, copied in; `failed` could not be read.
  void contentReceived(const QStringList& paths, int failed);
  // saveToGallery's outcome for `path`: 0 saved, 1 failed, 2 the storage
  // permission was asked for (API 28; the save can be tried again), 3 it
  // is refused for good, so the app's settings page was opened.
  void imageSaved(const QString& path, int result);
  void notificationsEnabledChanged();
  void activeConversationChanged();
  void openRequested();
  void linkRequested();
  void audioRoutesChanged();
  void microphoneAnswered(bool granted);
  void incomingCallChanged();
  void callAnswered();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  QObject* backLayer();
  // A picker (or the camera) was started and has not come back.
  bool pickBusy() const;
  // Starts the shim's `intentMethod(long limit)` intent for the pickers' result.
  void pickWith(const char* intentMethod, qreal limitBytes);
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
  // cpp/links.cpp.
  void installLinks();
  // cpp/calls.cpp.
  void installCalls();

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
  QString m_linkRequest;
  QVariantMap m_openRequest;
  QVariantList m_audioRoutes;
  bool m_askingMicrophone = false;
  QVariantMap m_incomingCall;
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
