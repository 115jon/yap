#pragma once

// The desktop shell's OS side, as the desktop app does it (`tray.rs`,
// `main_window.rs`, `window_state.rs`, `desktop_settings.rs`, `window.rs`):
// one instance, the tray icon with its unread badge, toasts, the taskbar
// flash, the autostart entry, and the main window's rect. The settings,
// the badge and the toasts themselves come from the Rust `Desktop`
// singleton; QML wires the two.
//
// Its identity is its own everywhere. The installed Yap app owns
// com.115jon.yap, the Run value "Yap", the yap:// scheme and its folders;
// none of them is read or written here. Deep links use our own
// yap-native:// (HKCU\Software\Classes\yap-native only, and only the
// default profile registers it). A named profile (profile.h) suffixes every
// name that identifies the app, so two profiles run side by side.
//
// QML (import Yap.Chrome):
//   DesktopShell.badgeCount / badgeDot / badgeTooltip   (bind from Desktop)
//   DesktopShell.profile                                "" or the profile name
//   DesktopShell.showToast(title, summary, body, channelId, serverId)
//   DesktopShell.toastActivated(channelId, serverId)
//   DesktopShell.setAutostart(on), reveal(), hideToTray(), quit()

#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QRect>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QTimer>

class QLocalServer;
class QQuickWindow;

class DesktopShell : public QObject
{
  Q_OBJECT
  Q_PROPERTY(int badgeCount READ badgeCount WRITE setBadgeCount NOTIFY badgeChanged)
  Q_PROPERTY(bool badgeDot READ badgeDot WRITE setBadgeDot NOTIFY badgeChanged)
  Q_PROPERTY(QString badgeTooltip READ badgeTooltip WRITE setBadgeTooltip NOTIFY badgeChanged)
  // The named profile (YAP_PROFILE / --profile), "" for the default one.
  Q_PROPERTY(QString profile READ profile CONSTANT)

public:
  explicit DesktopShell(QObject* parent = nullptr);
  ~DesktopShell() override;

  // The process's AppUserModelID (taskbar grouping, toasts). Call first
  // thing in main, before any window exists.
  static void setProcessIdentity();
  // True for the first instance (it holds a named mutex from then on).
  // Needs no Qt, so it can run before the log file starts.
  static bool takeInstanceLock();
  // A later instance: asks the first to show its window and hands it this
  // command line's arguments (a yap-native:// link among them). Needs a
  // QCoreApplication; the caller exits after it.
  static void showFirstInstance(const QStringList& args);

  // The Rust `Invites` object: arguments from later instances go to its
  // `openArg`, and `registerUrlScheme` asks it for the registration.
  void setLinkHandler(QObject* links);
  // Writes the per-user yap-native:// registration (HKCU) where it differs
  // from what `Invites.schemeRegistration` asks for. Idempotent.
  void registerUrlScheme();
  // Hands one command-line argument to the link handler.
  void openArg(const QString& arg);

  // The main window and the Rust `Desktop` object: restores the saved rect,
  // creates the tray icon, listens for later instances, and shows the
  // window (or keeps it in the tray on an autostart launch with start
  // minimized).
  void attach(QQuickWindow* window, QObject* desktop);

  int badgeCount() const { return m_badgeCount; }
  void setBadgeCount(int count);
  bool badgeDot() const { return m_badgeDot; }
  void setBadgeDot(bool dot);
  QString badgeTooltip() const { return m_badgeTooltip; }
  QString profile() const;
  void setBadgeTooltip(const QString& tooltip);

  Q_INVOKABLE void showToast(const QString& title, const QString& summary,
                             const QString& body, const QString& channelId,
                             const QString& serverId);
  // Writes or removes the HKCU Run value "YapNative" ("YapNative-<profile>"
  // for a named profile). False on failure.
  Q_INVOKABLE bool setAutostart(bool enabled);
  // Shows, restores and foregrounds the window.
  Q_INVOKABLE void reveal();
  // Close to tray: saves the rect and hides the window.
  Q_INVOKABLE void hideToTray();
  // Saves the rect and quits.
  Q_INVOKABLE void quit();

  // The tray window's messages (called from its window procedure).
  bool handleTrayMessage(unsigned message, quintptr wParam, qintptr lParam);

signals:
  void badgeChanged();
  // A toast was clicked; the window is already revealed.
  void toastActivated(const QString& channelId, const QString& serverId);

private:
  void createTray();
  void removeTray();
  void updateTrayIcon();
  void showTrayMenu();
  void flashTaskbar(bool on);
  void restoreGeometry();
  void scheduleSave();
  void saveNow();
  void onInstanceConnected();
  quintptr windowHandle() const;

  QPointer<QQuickWindow> m_window;
  QPointer<QObject> m_desktop;
  QPointer<QObject> m_links;
  QLocalServer* m_server = nullptr;

  // The hidden window that receives the tray icon's messages.
  quintptr m_trayHwnd = 0;
  quintptr m_icon = 0;
  bool m_trayAdded = false;
  unsigned m_taskbarCreated = 0;
  qint64 m_lastActivation = 0;

  int m_badgeCount = 0;
  bool m_badgeDot = false;
  QString m_badgeTooltip;
  bool m_badgeQueued = false;
  bool m_flashing = false;

  // The last normal (not maximized, not minimized) rect.
  QRect m_normal;
  QTimer m_saveTimer;
};
