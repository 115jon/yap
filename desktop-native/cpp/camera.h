#pragma once

// Our camera, for the call and the preview (QML singleton `Camera`).
//
//   Camera.deviceIds / deviceNames   the cameras, as Qt Multimedia lists them
//   Camera.deviceId, deviceLabel     the saved choice ("default" or an id)
//   Camera.quality                   "720p30" ... (camera-quality.ts)
//   Camera.publishing, previewing    who wants frames; either opens it
//   Camera.running, problem          frames arrive; why not ("missing",
//                                    "busy", "unknown")
//
// QCamera -> QMediaCaptureSession -> QVideoSink. Each frame goes to the
// local preview (VideoHub key "local-camera", no copy) and, while the call
// publishes, is mapped once and handed to the Rust call (src/camera.rs),
// which converts it to I420 straight into the encoder's buffer.
//
// YAP_FAKE_CAMERA=1 never creates a QCamera: a moving NV12 test pattern at
// the chosen size and rate goes through the same frame path instead.
//
// Device choice follows packages/voice/DEVICES.md: a saved camera is found
// by id, then by name (its id refreshed, emitted as `refreshed`); one that
// is not listed opens the system default without touching the saved
// choice, and is opened again once it is back.

#include <QtCore/QElapsedTimer>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QStringList>
#include <QtMultimedia/QCameraDevice>
#include <QtMultimedia/QVideoFrame>

#include <atomic>
#include <memory>
#include <mutex>

class QCamera;
class QMediaCaptureSession;
class QMediaDevices;
class QVideoSink;
class FakeCamera;

class CameraCapture : public QObject
{
  Q_OBJECT
  Q_PROPERTY(QStringList deviceIds READ deviceIds NOTIFY devicesChanged)
  Q_PROPERTY(QStringList deviceNames READ deviceNames NOTIFY devicesChanged)
  Q_PROPERTY(bool hasCamera READ hasCamera NOTIFY devicesChanged)
  Q_PROPERTY(QString deviceId READ deviceId WRITE setDeviceId NOTIFY choiceChanged)
  Q_PROPERTY(QString deviceLabel READ deviceLabel WRITE setDeviceLabel NOTIFY choiceChanged)
  Q_PROPERTY(QString quality READ quality WRITE setQuality NOTIFY choiceChanged)
  Q_PROPERTY(bool publishing READ publishing WRITE setPublishing NOTIFY wantChanged)
  Q_PROPERTY(bool previewing READ previewing WRITE setPreviewing NOTIFY wantChanged)
  Q_PROPERTY(bool running READ running NOTIFY runningChanged)
  Q_PROPERTY(QString problem READ problem NOTIFY problemChanged)
  Q_PROPERTY(QString openName READ openName NOTIFY runningChanged)
  Q_PROPERTY(bool fake READ fake CONSTANT)

public:
  explicit CameraCapture(QObject* parent = nullptr);
  ~CameraCapture() override;

  QStringList deviceIds() const { return m_ids; }
  QStringList deviceNames() const { return m_names; }
  bool hasCamera() const { return !m_ids.isEmpty(); }
  QString deviceId() const { return m_deviceId; }
  void setDeviceId(const QString& id);
  QString deviceLabel() const { return m_deviceLabel; }
  void setDeviceLabel(const QString& label);
  QString quality() const { return m_quality; }
  void setQuality(const QString& quality);
  bool publishing() const { return m_publishing.load(std::memory_order_relaxed); }
  void setPublishing(bool on);
  bool previewing() const { return m_previewing; }
  void setPreviewing(bool on);
  bool running() const { return m_running; }
  QString problem() const { return m_problem; }
  QString openName() const { return m_openName; }
  bool fake() const { return m_fake; }

  // Opens again after a failure (the dialog's "Try again").
  Q_INVOKABLE void retry();

  // Any thread: one captured frame of open number `generation` (frames of
  // an earlier open are dropped).
  void deliver(const QVideoFrame& frame, quint64 generation);

signals:
  void devicesChanged();
  void choiceChanged();
  void wantChanged();
  void runningChanged();
  void problemChanged();
  // Turning the camera on failed: "missing", "busy" or "unknown".
  void failed(const QString& reason);
  // The saved camera is listed under a new id: save it.
  void refreshed(const QString& id, const QString& label);

private:
  struct Profile
  {
    int width;
    int height;
    int fps;
  };
  static Profile profileFor(const QString& quality);

  void listDevices();
  void sync();
  void open();
  void close();
  void setProblem(const QString& problem);
  void firstFrame();
  QCameraDevice resolve(bool* fellBack);
  void onCameraError(const QString& message);

  bool m_fake = false;
  QMediaDevices* m_devices = nullptr;
  QList<QCameraDevice> m_cameras;
  QStringList m_ids;
  QStringList m_names;

  QString m_deviceId = QStringLiteral("default");
  QString m_deviceLabel;
  QString m_quality = QStringLiteral("720p30");
  std::atomic<bool> m_publishing{ false };
  bool m_previewing = false;

  // What is open: the device id (or the fake's) and the profile.
  bool m_open = false;
  QByteArray m_openId;
  QString m_openQuality;
  bool m_fellBack = false;
  // An open failed: wait for a retry, a new choice or a device change
  // instead of opening again in a loop.
  bool m_blocked = false;
  bool m_running = false;
  QString m_openName;
  QString m_problem;

  QCamera* m_camera = nullptr;
  QMediaCaptureSession* m_session = nullptr;
  QVideoSink* m_sink = nullptr;
  std::unique_ptr<FakeCamera> m_fakeSource;

  // Frame pacing to the profile's rate (the web asks frameRate max).
  std::mutex m_paceMutex;
  QElapsedTimer m_clock;
  qint64 m_nextDueUs = 0;
  qint64 m_frameUs = 33333;
  std::atomic<bool> m_sawFrame{ false };
  // Bumped by every open: tells its frames and its start watchdog apart
  // from an earlier open of the same device.
  std::atomic<quint64> m_generation{ 0 };
  // Counted on the frame thread, logged every 5 s.
  int m_frames = 0;
  int m_sent = 0;
  qint64 m_statsSinceUs = 0;
};
