#include "camera.h"

#include "videohub.h"

#include <QtCore/QMetaObject>
#include <QtCore/QTimer>
#include <QtGui/QImage>
#include <QtMultimedia/QCamera>
#include <QtMultimedia/QCameraFormat>
#include <QtMultimedia/QMediaCaptureSession>
#include <QtMultimedia/QMediaDevices>
#include <QtMultimedia/QVideoFrameFormat>
#include <QtMultimedia/QVideoSink>

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <thread>
#include <utility>
#include <vector>

// src/camera.rs: one captured frame into the call (false: not sent).
extern "C" bool yap_camera_frame(int32_t format,
                                 uint32_t width,
                                 uint32_t height,
                                 const uint8_t* p0,
                                 size_t len0,
                                 uint32_t stride0,
                                 const uint8_t* p1,
                                 size_t len1,
                                 uint32_t stride1,
                                 const uint8_t* p2,
                                 size_t len2,
                                 uint32_t stride2);
// A camera background is on (src/camera.rs): frames go to Rust for the
// preview too, and the preview shows the composited ones it hands back.
extern "C" bool yap_camera_effect_active();

namespace {

const QString kLocalKey = QStringLiteral("local-camera");
const QString kDefault = QStringLiteral("default");
const QByteArray kFakeId = QByteArrayLiteral("yap-test-pattern");
constexpr int kStartTimeoutMs = 6000;

// The format codes src/camera.rs reads.
enum Code : int32_t { I420 = 0, NV12 = 1, NV21 = 2, YUY2 = 3, UYVY = 4, BGRA = 5, RGBA = 6 };

int codeFor(QVideoFrameFormat::PixelFormat format)
{
  switch (format) {
    case QVideoFrameFormat::Format_YUV420P:
    case QVideoFrameFormat::Format_YV12:
      return I420;
    case QVideoFrameFormat::Format_NV12:
      return NV12;
    case QVideoFrameFormat::Format_NV21:
      return NV21;
    case QVideoFrameFormat::Format_YUYV:
      return YUY2;
    case QVideoFrameFormat::Format_UYVY:
      return UYVY;
    case QVideoFrameFormat::Format_BGRA8888:
    case QVideoFrameFormat::Format_BGRX8888:
      return BGRA;
    case QVideoFrameFormat::Format_RGBA8888:
    case QVideoFrameFormat::Format_RGBX8888:
      return RGBA;
    default:
      return -1;
  }
}

const char* formatName(QVideoFrameFormat::PixelFormat format)
{
  switch (format) {
    case QVideoFrameFormat::Format_NV12: return "NV12";
    case QVideoFrameFormat::Format_YUYV: return "YUY2";
    case QVideoFrameFormat::Format_UYVY: return "UYVY";
    case QVideoFrameFormat::Format_YUV420P: return "I420";
    case QVideoFrameFormat::Format_Jpeg: return "MJPEG";
    default: return "other";
  }
}

// Maps the frame once and hands its planes to the call. Formats libyuv
// does not take here (MJPEG, ARGB byte orders) go through QImage first.
bool sendToCall(const QVideoFrame& source)
{
  const int code = codeFor(source.pixelFormat());
  const auto w = static_cast<uint32_t>(source.width());
  const auto h = static_cast<uint32_t>(source.height());
  if (code < 0) {
    const QImage image = source.toImage().convertToFormat(QImage::Format_ARGB32);
    if (image.isNull())
      return false;
    return yap_camera_frame(BGRA, static_cast<uint32_t>(image.width()),
                            static_cast<uint32_t>(image.height()), image.constBits(),
                            static_cast<size_t>(image.sizeInBytes()),
                            static_cast<uint32_t>(image.bytesPerLine()), nullptr, 0, 0, nullptr,
                            0, 0);
  }
  QVideoFrame frame = source;
  if (!frame.map(QVideoFrame::ReadOnly))
    return false;
  const uint8_t* planes[3] = {};
  size_t lengths[3] = {};
  uint32_t strides[3] = {};
  for (int i = 0; i < frame.planeCount() && i < 3; ++i) {
    planes[i] = frame.bits(i);
    lengths[i] = static_cast<size_t>(frame.mappedBytes(i));
    strides[i] = static_cast<uint32_t>(frame.bytesPerLine(i));
  }
  if (source.pixelFormat() == QVideoFrameFormat::Format_YV12) {
    std::swap(planes[1], planes[2]);
    std::swap(lengths[1], lengths[2]);
    std::swap(strides[1], strides[2]);
  }
  const bool sent = yap_camera_frame(code, w, h, planes[0], lengths[0], strides[0], planes[1],
                                     lengths[1], strides[1], planes[2], lengths[2], strides[2]);
  frame.unmap();
  return sent;
}

QString normalized(const QString& label)
{
  return label.trimmed().toLower();
}

}  // namespace

// The YAP_FAKE_CAMERA source: a moving NV12 test pattern (75% colour bars
// scrolling sideways, a bouncing white square) on its own thread, at the
// profile's size and rate. It never opens a device.
class FakeCamera
{
public:
  FakeCamera(CameraCapture* owner, quint64 generation, int width, int height, int fps)
    : m_owner(owner)
    , m_generation(generation)
    , m_width(width & ~1)
    , m_height(height & ~1)
    , m_fps(fps)
    , m_thread([this] { run(); })
  {
  }

  ~FakeCamera()
  {
    m_stop.store(true);
    m_thread.join();
  }

private:
  // YAP_FAKE_CAMERA_IMAGE=<path>: that picture, scaled to cover the
  // profile's size, at its rate (tests of camera backgrounds need a person
  // in frame). False if it would not load.
  bool runImage(const QString& path)
  {
    QImage image(path);
    if (image.isNull())
      return false;
    const int w = m_width;
    const int h = m_height;
    image = image.scaled(w, h, Qt::KeepAspectRatioByExpanding, Qt::SmoothTransformation)
              .convertToFormat(QImage::Format_ARGB32);
    image = image.copy((image.width() - w) / 2, (image.height() - h) / 2, w, h);
    const auto period = std::chrono::microseconds(1000000 / std::max(1, m_fps));
    auto due = std::chrono::steady_clock::now();
    while (!m_stop.load()) {
      QVideoFrame frame(QVideoFrameFormat(QSize(w, h), QVideoFrameFormat::Format_BGRA8888));
      if (frame.map(QVideoFrame::WriteOnly)) {
        // ARGB32 is B, G, R, A in memory on little-endian: BGRA8888.
        for (int row = 0; row < h; ++row)
          std::memcpy(frame.bits(0) + static_cast<size_t>(row) * frame.bytesPerLine(0),
                      image.constScanLine(row), static_cast<size_t>(w) * 4);
        frame.unmap();
        m_owner->deliver(frame, m_generation);
      }
      due += period;
      const auto now = std::chrono::steady_clock::now();
      if (due < now)
        due = now;
      std::this_thread::sleep_until(due);
    }
    return true;
  }

  void run()
  {
    const QString picture = qEnvironmentVariable("YAP_FAKE_CAMERA_IMAGE");
    if (!picture.isEmpty() && runImage(picture))
      return;
    // BT.601 limited-range 75% bars: white, yellow, cyan, green, magenta,
    // red, blue, black.
    static constexpr uint8_t Y[8] = { 180, 162, 131, 112, 84, 65, 35, 16 };
    static constexpr uint8_t U[8] = { 128, 44, 156, 72, 184, 100, 212, 128 };
    static constexpr uint8_t V[8] = { 128, 142, 44, 58, 198, 212, 114, 128 };
    const int w = m_width;
    const int h = m_height;
    // Two periods side by side, so a row is one copy at any scroll.
    std::vector<uint8_t> lumaRow(static_cast<size_t>(w) * 2);
    std::vector<uint8_t> chromaRow(static_cast<size_t>(w) * 2);
    for (int x = 0; x < w * 2; ++x) {
      const int bar = ((x % w) * 8) / w;
      lumaRow[static_cast<size_t>(x)] = Y[bar];
    }
    for (int x = 0; x < w; ++x) {
      const int bar = (((x * 2) % w) * 8) / w;
      chromaRow[static_cast<size_t>(x) * 2] = U[bar];
      chromaRow[static_cast<size_t>(x) * 2 + 1] = V[bar];
    }
    const int box = h / 4;
    const auto period = std::chrono::microseconds(1000000 / std::max(1, m_fps));
    auto due = std::chrono::steady_clock::now();
    for (int n = 0; !m_stop.load(); ++n) {
      QVideoFrame frame(QVideoFrameFormat(QSize(w, h), QVideoFrameFormat::Format_NV12));
      if (frame.map(QVideoFrame::WriteOnly)) {
        const int scroll = ((n * 4) % w) & ~1;
        uint8_t* yPlane = frame.bits(0);
        uint8_t* uvPlane = frame.bits(1);
        const int ys = frame.bytesPerLine(0);
        const int uvs = frame.bytesPerLine(1);
        for (int row = 0; row < h; ++row)
          std::memcpy(yPlane + static_cast<size_t>(row) * ys, lumaRow.data() + scroll,
                      static_cast<size_t>(w));
        for (int row = 0; row < h / 2; ++row)
          std::memcpy(uvPlane + static_cast<size_t>(row) * uvs, chromaRow.data() + scroll,
                      static_cast<size_t>(w));
        // The square bounces corner to corner.
        const int spanX = std::max(1, w - box);
        const int spanY = std::max(1, h - box);
        int bx = (n * 6) % (spanX * 2);
        int by = (n * 4) % (spanY * 2);
        bx = (bx > spanX ? spanX * 2 - bx : bx) & ~1;
        by = (by > spanY ? spanY * 2 - by : by) & ~1;
        for (int row = by; row < by + box; ++row)
          std::memset(yPlane + static_cast<size_t>(row) * ys + bx, 235, static_cast<size_t>(box));
        for (int row = by / 2; row < (by + box) / 2; ++row) {
          uint8_t* uv = uvPlane + static_cast<size_t>(row) * uvs + bx;
          for (int i = 0; i < box; ++i)
            uv[i] = 128;
        }
        frame.unmap();
        m_owner->deliver(frame, m_generation);
      }
      due += period;
      const auto now = std::chrono::steady_clock::now();
      if (due < now)
        due = now;
      std::this_thread::sleep_until(due);
    }
  }

  CameraCapture* m_owner;
  quint64 m_generation;
  int m_width;
  int m_height;
  int m_fps;
  std::atomic<bool> m_stop{ false };
  std::thread m_thread;
};

CameraCapture::CameraCapture(QObject* parent) : QObject(parent)
{
  m_fake = qEnvironmentVariable("YAP_FAKE_CAMERA") == QLatin1String("1");
  m_clock.start();
}

// The first read of the list (a call's camera key, Settings, a camera
// turned on) lists the cameras and starts watching for changes. Not at
// startup: listing initialises Media Foundation, which a session without
// video never needs. The first list is taken silently, inside the read.
void CameraCapture::ensureListed() const
{
  if (m_devices)
    return;
  auto* self = const_cast<CameraCapture*>(this);
  self->m_devices = new QMediaDevices(self);
  connect(self->m_devices, &QMediaDevices::videoInputsChanged, self, [self] {
    self->listDevices();
    // A camera was plugged in or out: a blocked open may work now, and a
    // fallback may get its saved camera back.
    self->m_blocked = false;
    self->sync();
  });
  self->listDevices(false);
}

CameraCapture::~CameraCapture()
{
  close();
}

CameraCapture::Profile CameraCapture::profileFor(const QString& quality)
{
  if (quality == QLatin1String("480p30"))
    return { 854, 480, 30 };
  if (quality == QLatin1String("720p60"))
    return { 1280, 720, 60 };
  if (quality == QLatin1String("1080p30"))
    return { 1920, 1080, 30 };
  if (quality == QLatin1String("1080p60"))
    return { 1920, 1080, 60 };
  return { 1280, 720, 30 };
}

void CameraCapture::listDevices(bool notify)
{
  m_cameras = QMediaDevices::videoInputs();
  QStringList ids;
  QStringList names;
  QStringList found;
  for (const QCameraDevice& camera : std::as_const(m_cameras)) {
    found << camera.description();
    if (!m_fake) {
      ids << QString::fromUtf8(camera.id());
      names << camera.description();
    }
  }
  if (m_fake) {
    ids << QString::fromUtf8(kFakeId);
    names << QStringLiteral("Yap test pattern");
  }
  std::fprintf(stderr, "[camera] %lld listed%s: %s\n", static_cast<long long>(found.size()),
               m_fake ? " (fake mode: none will open)" : "",
               qPrintable(found.join(QStringLiteral(", "))));
  if (ids != m_ids || names != m_names) {
    m_ids = ids;
    m_names = names;
    if (notify)
      emit devicesChanged();
  }
}

void CameraCapture::setDeviceId(const QString& id)
{
  const QString value = id.isEmpty() ? kDefault : id;
  if (value == m_deviceId)
    return;
  m_deviceId = value;
  m_blocked = false;
  emit choiceChanged();
  sync();
}

void CameraCapture::setDeviceLabel(const QString& label)
{
  if (label == m_deviceLabel)
    return;
  m_deviceLabel = label;
  emit choiceChanged();
}

void CameraCapture::setQuality(const QString& quality)
{
  if (quality == m_quality)
    return;
  m_quality = quality;
  m_blocked = false;
  emit choiceChanged();
  sync();
}

void CameraCapture::setPublishing(bool on)
{
  if (on == publishing())
    return;
  m_publishing.store(on, std::memory_order_relaxed);
  if (on)
    m_blocked = false;
  emit wantChanged();
  sync();
}

void CameraCapture::setPreviewing(bool on)
{
  if (on == m_previewing)
    return;
  m_previewing = on;
  if (on)
    m_blocked = false;
  emit wantChanged();
  sync();
}

void CameraCapture::retry()
{
  m_blocked = false;
  setProblem(QString());
  sync();
}

QCameraDevice CameraCapture::resolve(bool* fellBack)
{
  *fellBack = false;
  if (m_deviceId != kDefault) {
    for (const QCameraDevice& camera : std::as_const(m_cameras)) {
      if (QString::fromUtf8(camera.id()) == m_deviceId)
        return camera;
    }
    // By name: the same camera under a new id. Saved back as a refresh.
    const QString wanted = normalized(m_deviceLabel);
    if (!wanted.isEmpty()) {
      for (const QCameraDevice& camera : std::as_const(m_cameras)) {
        if (normalized(camera.description()) == wanted) {
          emit refreshed(QString::fromUtf8(camera.id()), camera.description());
          return camera;
        }
      }
    }
    // Not listed: the system default, the saved choice left as it is.
    *fellBack = true;
  }
  return QMediaDevices::defaultVideoInput();
}

void CameraCapture::sync()
{
  const bool want = (publishing() || m_previewing) && !m_blocked;
  if (!want) {
    close();
    return;
  }
  ensureListed();
  bool fellBack = false;
  QCameraDevice device;
  QByteArray id = kFakeId;
  if (!m_fake) {
    device = resolve(&fellBack);
    if (device.isNull()) {
      close();
      m_blocked = true;
      setProblem(QStringLiteral("missing"));
      emit failed(m_problem);
      return;
    }
    id = device.id();
  }
  if (m_open && id == m_openId && m_quality == m_openQuality)
    return;
  close();

  const Profile profile = profileFor(m_quality);
  {
    std::lock_guard lock(m_paceMutex);
    m_frameUs = 1000000 / std::max(1, profile.fps);
    m_nextDueUs = 0;
    m_frames = 0;
    m_sent = 0;
    m_statsSinceUs = m_clock.nsecsElapsed() / 1000;
  }
  const quint64 generation = m_generation.fetch_add(1) + 1;
  m_sawFrame.store(false);
  m_open = true;
  m_openId = id;
  m_openQuality = m_quality;
  m_fellBack = fellBack;

  if (m_fake) {
    m_openName = QStringLiteral("Yap test pattern");
    std::fprintf(stderr, "[camera] open fake %dx%d@%d\n", profile.width, profile.height,
                 profile.fps);
    m_fakeSource = std::make_unique<FakeCamera>(this, generation, profile.width, profile.height, profile.fps);
  } else {
    // The closest native format to the profile, uncompressed first: MJPEG
    // costs a decode per frame.
    QCameraFormat best;
    qint64 bestScore = -1;
    for (const QCameraFormat& format : device.videoFormats()) {
      const QSize size = format.resolution();
      const qint64 pixel = codeFor(format.pixelFormat()) >= 0 ? 0
                           : format.pixelFormat() == QVideoFrameFormat::Format_Jpeg ? 2
                                                                                    : 1;
      const qint64 distance = std::abs(size.width() - profile.width) +
                              std::abs(size.height() - profile.height);
      const qint64 slow = format.maxFrameRate() + 0.5f < profile.fps
                            ? static_cast<qint64>(profile.fps - format.maxFrameRate())
                            : 0;
      const qint64 score = pixel * 1000000 + distance * 10 + slow * 200;
      if (bestScore < 0 || score < bestScore) {
        best = format;
        bestScore = score;
      }
    }
    m_openName = device.description();
    m_camera = new QCamera(device, this);
    if (!best.isNull()) {
      m_camera->setCameraFormat(best);
      std::fprintf(stderr, "[camera] open %s%s: %dx%d %s %.0f-%.0f fps\n",
                   qPrintable(m_openName), fellBack ? " (saved camera not listed)" : "",
                   best.resolution().width(), best.resolution().height(),
                   formatName(best.pixelFormat()), best.minFrameRate(), best.maxFrameRate());
    }
    m_session = new QMediaCaptureSession(this);
    m_sink = new QVideoSink(this);
    m_session->setCamera(m_camera);
    m_session->setVideoSink(m_sink);
    // On the backend's thread: deliver() is thread-safe.
    connect(
      m_sink, &QVideoSink::videoFrameChanged, this,
      [this, generation](const QVideoFrame& frame) { deliver(frame, generation); },
      Qt::DirectConnection);
    connect(m_camera, &QCamera::errorOccurred, this,
            [this](QCamera::Error error, const QString& message) {
              if (error != QCamera::NoError)
                onCameraError(message);
            });
    m_camera->start();
  }
  // A camera another app holds often just never delivers.
  QTimer::singleShot(kStartTimeoutMs, this, [this, generation] {
    if (m_open && m_generation.load() == generation && !m_sawFrame.load())
      onCameraError(QStringLiteral("no frames"));
  });
  emit runningChanged();
}

void CameraCapture::close()
{
  if (!m_open)
    return;
  m_open = false;
  // Frames still on their way from this open are dropped from here on.
  m_generation.fetch_add(1);
  m_fakeSource.reset();
  if (m_sink)
    m_sink->disconnect(this);
  if (m_camera) {
    m_camera->disconnect(this);
    m_camera->stop();
  }
  delete m_session;
  delete m_sink;
  delete m_camera;
  m_session = nullptr;
  m_sink = nullptr;
  m_camera = nullptr;
  m_openId.clear();
  m_openName.clear();
  m_running = false;
  std::fprintf(stderr, "[camera] closed\n");
  emit runningChanged();
}

void CameraCapture::setProblem(const QString& problem)
{
  if (problem == m_problem)
    return;
  m_problem = problem;
  emit problemChanged();
}

void CameraCapture::onCameraError(const QString& message)
{
  if (!m_open)
    return;
  std::fprintf(stderr, "[camera] %s: %s\n", qPrintable(m_openName), qPrintable(message));
  // Unplugged while open: fall back (or come back) through the list.
  const QByteArray openId = m_openId;
  listDevices();
  const bool stillListed = std::any_of(m_cameras.cbegin(), m_cameras.cend(),
                                       [&](const QCameraDevice& c) { return c.id() == openId; });
  close();
  if (!m_fake && !stillListed && !m_cameras.isEmpty()) {
    sync();
    return;
  }
  m_blocked = true;
  setProblem(m_cameras.isEmpty() && !m_fake ? QStringLiteral("missing") : QStringLiteral("busy"));
  emit failed(m_problem);
}

void CameraCapture::firstFrame()
{
  if (!m_open || m_running)
    return;
  m_running = true;
  setProblem(QString());
  emit runningChanged();
}

void CameraCapture::deliver(const QVideoFrame& frame, quint64 generation)
{
  if (!frame.isValid() || generation != m_generation.load())
    return;
  if (!m_sawFrame.exchange(true))
    QMetaObject::invokeMethod(this, &CameraCapture::firstFrame, Qt::QueuedConnection);
  // At most the profile's rate (frameRate: { max }).
  const qint64 now = m_clock.nsecsElapsed() / 1000;
  bool log = false;
  int frames = 0;
  int sent = 0;
  {
    std::lock_guard lock(m_paceMutex);
    ++m_frames;
    // A little slack: a camera at exactly the rate keeps every frame.
    if (now + 2000 < m_nextDueUs)
      return;
    m_nextDueUs = std::max(m_nextDueUs + m_frameUs, now);
  }
  const bool effect = yap_camera_effect_active();
  bool previewWanted = false;
  if (VideoHub* hub = VideoHub::instance()) {
    if (VideoFeed* feed = hub->wantedFeed(kLocalKey)) {
      previewWanted = true;
      if (!effect)
        feed->push(frame);
    }
  }
  const bool toCall = (publishing() || (effect && previewWanted)) && sendToCall(frame) && publishing();
  {
    std::lock_guard lock(m_paceMutex);
    if (toCall)
      ++m_sent;
    if (now - m_statsSinceUs >= 5000000) {
      log = true;
      frames = m_frames;
      sent = m_sent;
      m_frames = 0;
      m_sent = 0;
      m_statsSinceUs = now;
    }
  }
  if (log)
    std::fprintf(stderr, "[camera] %dx%d %s: %.1f fps in, %.1f fps to the call\n", frame.width(),
                 frame.height(), formatName(frame.pixelFormat()), frames / 5.0, sent / 5.0);
}
