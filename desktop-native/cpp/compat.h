#pragma once

// Compatibility guards for the GPU and Media Foundation (docs/LIBRARIES.md,
// "Compatibility"), and their notices.
//
// GPU safe mode: Qt Quick draws with WARP (Direct3D's software rasterizer,
// QSG_RHI_PREFER_SOFTWARE_RENDERER=1), and nothing of ours touches the GPU
// either (no shared device, the H.264 decoder in software, VP8 shares). It
// is on for a run when:
//   - the GPU's best feature level is below 11_0 (Qt's shaders need SM 5),
//     or there is no hardware D3D11 device: decided again every start;
//   - it was saved: the scene graph could not start on the GPU
//     (sceneGraphError, the app restarts itself in safe mode), a lost
//     device could not be replaced, devices were lost three times in one
//     run, or two starts in a row ended before their first frame (the crash
//     guard). A saved safe mode ends by itself when the adapter or its
//     driver version changes, or with --no-gpu-safe-mode, or with the
//     notice's "Use the GPU next time";
//   - --gpu-safe-mode or YAP_GPU_SAFE_MODE=1 asks for it (this run only).
// The saved state is `gpu-compat.json` in the profile's data folder.
//
// A lost device (TDR, driver update): Qt's render loop drops its QRhi; if
// the window draws on our shared device (VideoHub, zero copy), a new one is
// made and given to the window and the decoders before the next frame.
//
// QML (import Yap.Chrome), Compat:
//   mediaPlayback     Media Foundation is there (mp4, audio previews)
//   mediaNotice       show "Video playback needs the Media Feature Pack"
//   gpuSafeMode       this run draws in software
//   gpuNotice         show the safe-mode notice
//   gpuReason         why, in a sentence
//   gpuSaved          the safe mode was saved (useGpuNextTime() undoes it)
//   hardwareAcceleration  Settings > Desktop app: off saves safe mode for
//                     the next start, on clears a saved one
//   accelerationPending  that choice differs from how this run draws
//   dismissMedia(), dismissGpu(), useGpuNextTime(),
//   setHardwareAcceleration(on)

#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QString>

#include <atomic>

class QQuickWindow;

class Compat : public QObject
{
  Q_OBJECT
  Q_PROPERTY(bool mediaPlayback READ mediaPlayback CONSTANT)
  Q_PROPERTY(bool mediaNotice READ mediaNotice NOTIFY mediaNoticeChanged)
  Q_PROPERTY(bool gpuSafeMode READ gpuSafeMode CONSTANT)
  Q_PROPERTY(bool gpuNotice READ gpuNotice NOTIFY gpuNoticeChanged)
  Q_PROPERTY(QString gpuReason READ gpuReason CONSTANT)
  Q_PROPERTY(bool gpuSaved READ gpuSaved CONSTANT)
  Q_PROPERTY(bool hardwareAcceleration READ hardwareAcceleration NOTIFY hardwareAccelerationChanged)
  Q_PROPERTY(bool accelerationPending READ accelerationPending NOTIFY hardwareAccelerationChanged)

public:
  explicit Compat(QObject* parent = nullptr);

  // First in main, before the instance lock: a restarted process
  // (--yap-relaunched-from=<pid>) waits for the one it replaces to end.
  static void awaitPredecessor();
  // Before QGuiApplication, in the first instance only: probes Media
  // Foundation and the GPU, logs the adapter, runs the crash guard, and
  // sets the environment safe mode needs.
  static void prepare();
  // Before the window is first shown: the scene graph's error and
  // invalidation, the first frame (crash guard), the test switches.
  void attach(QQuickWindow* window);
  // A clean quit (after the event loop), or any other end after prepare()
  // that is not a GPU death (an early return): the crash guard's marker
  // goes.
  static void quitting();

  bool mediaPlayback() const;
  bool mediaNotice() const { return m_mediaNotice; }
  bool gpuSafeMode() const;
  bool gpuNotice() const { return m_gpuNotice; }
  QString gpuReason() const;
  bool gpuSaved() const;
  bool hardwareAcceleration() const;
  bool accelerationPending() const;

  Q_INVOKABLE void dismissMedia();
  Q_INVOKABLE void dismissGpu();
  Q_INVOKABLE void useGpuNextTime();
  Q_INVOKABLE void setHardwareAcceleration(bool on);

signals:
  void mediaNoticeChanged();
  void gpuNoticeChanged();
  void hardwareAccelerationChanged();

private:
  void firstFrame();
  void sceneGraphError(const QString& message);
  void sceneGraphInvalidated();
  // GUI thread: counts a loss; true when it was the third this run.
  bool countLoss(long reason, bool simulated);
  // Saves safe mode with `reason`, starts a new process and quits this one
  // through the event loop (exit code 3). Already in safe mode: logs and
  // quits.
  void restartInSafeMode(const QString& reason);

  QPointer<QQuickWindow> m_window;
  bool m_mediaNotice = false;
  bool m_gpuNotice = false;
  // Set on the GUI thread, read where the scene graph is invalidated.
  std::atomic<bool> m_simulateLoss{ false };
  int m_losses = 0;
};
