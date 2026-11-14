#pragma once

// The taskbar thumbnail toolbar (the Tauri app's
// desktop/src-tauri/src/thumbnail_toolbar.rs on branch archive/tauri-desktop,
// and DesktopThumbnailToolbarSync.tsx): camera, mic, deafen and disconnect under
// the window's taskbar preview while in a call, plus play/pause and skip for
// Listen Together when the room has a track. Hidden otherwise. The icons
// show the current state; the tooltips name the next action.
//
// The buttons are added once (ITaskbarList3 cannot remove them) after
// Explorer has made the taskbar button, and hidden or updated from then on.
// A click arrives as WM_COMMAND (THBN_CLICKED) on the main window.
//
// QML (import Yap.Chrome):
//   Thumbbar.sync(visible, hasCamera, cameraOn, hasMic, muted, deafened,
//                 hasMedia, mediaPaused)
//   Thumbbar.action(name)   toggle-camera, toggle-mic, toggle-deafen,
//                           disconnect, toggle-media-playback, skip-media

#include <QtCore/QAbstractNativeEventFilter>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QString>

class QWindow;

class ThumbnailToolbar : public QObject, public QAbstractNativeEventFilter
{
  Q_OBJECT

public:
  explicit ThumbnailToolbar(QObject* parent = nullptr);
  ~ThumbnailToolbar() override;

  // The main window; installs the filter for its messages.
  void attach(QWindow* window);

  // types.ts ThumbnailToolbarState. Applied when it changes.
  Q_INVOKABLE void sync(bool visible, bool hasCamera, bool cameraOn,
                        bool hasMic, bool muted, bool deafened, bool hasMedia,
                        bool mediaPaused);

  bool nativeEventFilter(const QByteArray& eventType, void* message,
                         qintptr* result) override;

  // The action a button id stands for, or "" (also the test hook's map).
  static QString actionFor(unsigned id);

signals:
  void action(const QString& name);

private:
  struct State
  {
    bool visible = false;
    bool hasCamera = false;
    bool cameraOn = false;
    bool hasMic = false;
    bool muted = false;
    bool deafened = false;
    bool hasMedia = false;
    bool mediaPaused = false;
    bool operator==(const State& o) const
    {
      return visible == o.visible && hasCamera == o.hasCamera &&
             cameraOn == o.cameraOn && hasMic == o.hasMic &&
             muted == o.muted && deafened == o.deafened &&
             hasMedia == o.hasMedia && mediaPaused == o.mediaPaused;
    }
  };

  void apply();
  void loadIcons();
  void destroyIcons();

  QPointer<QWindow> m_window;
  quintptr m_hwnd = 0;
  unsigned m_buttonCreated = 0;
  bool m_taskbarReady = false;
  bool m_added = false;
  State m_state;
  // HICONs: camera off/on, deafen off/on, disconnect, mic off/on, pause,
  // play, skip, separator.
  quintptr m_icons[11] = {};
};
