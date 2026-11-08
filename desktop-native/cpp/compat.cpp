#include "compat.h"

#include "mediafoundation.h"
#include "profile.h"
#include "videohub.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QDateTime>
#include <QtCore/QElapsedTimer>
#include <QtCore/QFile>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QSaveFile>
#include <QtCore/QThread>
#include <QtCore/QTimer>
#include <QtQuick/QQuickWindow>
#include <rhi/qrhi.h>

#include <cstdio>
#include <iterator>
#include <string>

#include <windows.h>
#include <d3d11.h>
#include <dxgi.h>
#include <shellapi.h>
#include <wrl/client.h>

// YAP_TEST_DEVICE_LOSS only: QRhiD3D11's own "device lost" flag, set as a
// failed Present sets it.
#include <QtGui/private/qrhid3d11_p.h>

using Microsoft::WRL::ComPtr;

namespace {

constexpr wchar_t kStateFile[] = L"\\gpu-compat.json";
constexpr wchar_t kRelaunchedFrom[] = L"--yap-relaunched-from=";

struct Decision
{
  bool safe = false;
  QString reason;
  bool notice = false;  // show the notice this run
  bool saved = false;   // the safe mode comes from the saved state
};

Decision g_gpu;
// The saved choice for the next start, as Settings shows it (hardware
// acceleration is off while a safe mode is saved), and whether Settings
// made it.
bool g_savedSafeMode = false;
bool g_userChoice = false;
const QString kUserReason = QStringLiteral("Hardware acceleration is off in Settings > Desktop app.");
// The crash guard's count for this run while it has drawn nothing yet: the
// runs in a row (this one included) that ended before a first frame.
int g_unframedStarts = 0;
bool g_prepared = false;

QString statePath()
{
  const std::wstring root = profile::dataRoot();
  return root.empty() ? QString() : QString::fromStdWString(root + kStateFile);
}

QJsonObject loadState()
{
  QFile file(statePath());
  if (statePath().isEmpty() || !file.open(QIODevice::ReadOnly))
    return {};
  return QJsonDocument::fromJson(file.readAll()).object();
}

void saveState(const QJsonObject& state)
{
  if (statePath().isEmpty())
    return;
  QSaveFile file(statePath());
  if (!file.open(QIODevice::WriteOnly))
    return;
  file.write(QJsonDocument(state).toJson(QJsonDocument::Indented));
  file.commit();
}

void updateState(const char* key, const QJsonValue& value)
{
  QJsonObject state = loadState();
  state.insert(QLatin1String(key), value);
  saveState(state);
}

bool hasArg(const wchar_t* wanted)
{
  int argc = 0;
  LPWSTR* argv = CommandLineToArgvW(GetCommandLineW(), &argc);
  if (!argv)
    return false;
  bool found = false;
  for (int i = 1; i < argc && !found; ++i)
    found = std::wcscmp(argv[i], wanted) == 0;
  LocalFree(argv);
  return found;
}

bool envIsOne(const char* name)
{
  return qEnvironmentVariableIntValue(name) == 1;
}

void setDefaultEnv(const char* name, const char* value)
{
  if (!qEnvironmentVariableIsSet(name))
    qputenv(name, value);
}

// The default adapter (the one Qt and D3D11CreateDevice(nullptr) take), as
// a string that changes with the GPU or its driver.
struct Adapter
{
  QString name;
  QString identity;
  bool found = false;
};

Adapter defaultAdapter()
{
  Adapter out;
  ComPtr<IDXGIFactory1> factory;
  ComPtr<IDXGIAdapter1> adapter;
  DXGI_ADAPTER_DESC1 desc{};
  if (FAILED(CreateDXGIFactory1(IID_PPV_ARGS(&factory))) ||
      FAILED(factory->EnumAdapters1(0, &adapter)) || FAILED(adapter->GetDesc1(&desc)))
    return out;
  out.found = true;
  out.name = QString::fromWCharArray(desc.Description);
  LARGE_INTEGER umd{};
  QString driver = QStringLiteral("?");
  if (SUCCEEDED(adapter->CheckInterfaceSupport(__uuidof(IDXGIDevice), &umd)))
    driver = QStringLiteral("%1.%2.%3.%4")
               .arg(HIWORD(umd.HighPart))
               .arg(LOWORD(umd.HighPart))
               .arg(HIWORD(umd.LowPart))
               .arg(LOWORD(umd.LowPart));
  out.identity = QStringLiteral("%1:%2:%3:%4 driver %5")
                   .arg(desc.VendorId, 4, 16, QLatin1Char('0'))
                   .arg(desc.DeviceId, 4, 16, QLatin1Char('0'))
                   .arg(desc.SubSysId, 8, 16, QLatin1Char('0'))
                   .arg(desc.Revision, 2, 16, QLatin1Char('0'))
                   .arg(driver);
  if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE)
    out.identity += QStringLiteral(" software");
  return out;
}

// The best feature level a hardware device gets on the default adapter (no
// device is kept), or 0 when there is no hardware D3D11 device at all.
D3D_FEATURE_LEVEL bestFeatureLevel()
{
  const D3D_FEATURE_LEVEL levels[] = { D3D_FEATURE_LEVEL_12_1, D3D_FEATURE_LEVEL_12_0,
                                       D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0,
                                       D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_10_0 };
  D3D_FEATURE_LEVEL got{};
  HRESULT hr = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0, levels,
                                 UINT(std::size(levels)), D3D11_SDK_VERSION, nullptr, &got, nullptr);
  if (hr == E_INVALIDARG) // a runtime that does not know 12_x
    hr = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0, levels + 2,
                           UINT(std::size(levels) - 2), D3D11_SDK_VERSION, nullptr, &got, nullptr);
  return SUCCEEDED(hr) ? got : D3D_FEATURE_LEVEL(0);
}

QString levelName(D3D_FEATURE_LEVEL level)
{
  return QStringLiteral("%1_%2").arg((unsigned(level) >> 12) & 0xf).arg((unsigned(level) >> 8) & 0xf);
}

void applySafeMode()
{
  // WARP for Qt Quick; our own GPU users stay off it too.
  setDefaultEnv("QSG_RHI_PREFER_SOFTWARE_RENDERER", "1");
  setDefaultEnv("YAP_ZERO_COPY", "0");
  setDefaultEnv("YAP_H264_DECODER", "software");
  setDefaultEnv("YAP_VPX_DECODER", "software");
  setDefaultEnv("YAP_SHARE_ENCODER", "software");
}

// Saves safe mode as the next start's choice. `user`: Settings turned
// hardware acceleration off, which no driver change undoes.
void saveSafeMode(const QString& reason, bool user = false)
{
  QJsonObject state = loadState();
  state.insert(QLatin1String("safeMode"), true);
  state.insert(QLatin1String("user"), user);
  state.insert(QLatin1String("reason"), reason);
  state.insert(QLatin1String("adapter"), defaultAdapter().identity);
  state.insert(QLatin1String("since"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
  state.insert(QLatin1String("noticeSeen"), false);
  state.insert(QLatin1String("startsWithoutFrame"), 0);
  saveState(state);
}

// The command line again, without our own restart switch.
std::wstring relaunchCommandLine(DWORD pid)
{
  int argc = 0;
  LPWSTR* argv = CommandLineToArgvW(GetCommandLineW(), &argc);
  std::wstring line;
  auto append = [&line](const std::wstring& arg) {
    if (!line.empty())
      line += L' ';
    if (!arg.empty() && arg.find_first_of(L" \t\"") == std::wstring::npos) {
      line += arg;
      return;
    }
    // CommandLineToArgvW's quoting: backslashes double before a quote.
    line += L'"';
    size_t slashes = 0;
    for (const wchar_t c : arg) {
      if (c == L'\\') {
        ++slashes;
      } else if (c == L'"') {
        line.append(slashes * 2 + 1, L'\\');
        line += L'"';
        slashes = 0;
        continue;
      } else {
        slashes = 0;
      }
      line += c;
    }
    line.append(slashes, L'\\');
    line += L'"';
  };
  wchar_t exe[MAX_PATH * 2];
  const DWORD n = GetModuleFileNameW(nullptr, exe, DWORD(std::size(exe)));
  append(std::wstring(exe, n));
  // Only switches go on: not --autostart (the new process shows its window,
  // never starts in the tray: the tray start needs it) and no link or other
  // plain argument (a yap-native:// link was handled once already).
  // --profile keeps its value.
  for (int i = 1; argv && i < argc; ++i) {
    const std::wstring arg = argv[i];
    if (arg == L"--profile") {
      append(arg);
      if (i + 1 < argc)
        append(argv[++i]);
      continue;
    }
    if (arg.rfind(L"--", 0) != 0 || arg == L"--autostart" || arg.rfind(kRelaunchedFrom, 0) == 0 ||
        arg.find(L"://") != std::wstring::npos)
      continue;
    append(arg);
  }
  if (argv)
    LocalFree(argv);
  append(kRelaunchedFrom + std::to_wstring(pid));
  return line;
}

// YAP_TEST_DEVICE_LOSS: makes Qt's QRhi report a lost device, as a Present
// that returned DXGI_ERROR_DEVICE_REMOVED does. QRhi holds nothing but its
// implementation pointer (no virtuals), and QRhiD3D11's members are public.
bool markRhiLost(QQuickWindow* window)
{
  QRhi* rhi = window ? window->rhi() : nullptr;
  if (!rhi || rhi->backend() != QRhi::D3D11)
    return false;
  static_assert(sizeof(QRhi) == sizeof(void*), "QRhi is expected to hold only its d pointer");
  auto* impl = *reinterpret_cast<QRhiImplementation**>(rhi);
  static_cast<QRhiD3D11*>(impl)->deviceLost = true;
  return true;
}

} // namespace

Compat::Compat(QObject* parent) : QObject(parent)
{
  m_gpuNotice = g_gpu.notice;
  if (!mediafoundation::available() && !loadState().value(QLatin1String("mediaNoticeSeen")).toBool())
    m_mediaNotice = true;
}

void Compat::awaitPredecessor()
{
  int argc = 0;
  LPWSTR* argv = CommandLineToArgvW(GetCommandLineW(), &argc);
  DWORD pid = 0;
  for (int i = 1; argv && i < argc; ++i) {
    if (std::wcsncmp(argv[i], kRelaunchedFrom, std::size(kRelaunchedFrom) - 1) == 0)
      pid = DWORD(std::wcstoul(argv[i] + std::size(kRelaunchedFrom) - 1, nullptr, 10));
  }
  if (argv)
    LocalFree(argv);
  if (pid == 0 || pid == GetCurrentProcessId())
    return;
  // It holds the instance lock until it has quit (its call left, its
  // cache committed); a minute at most.
  if (HANDLE process = OpenProcess(SYNCHRONIZE, FALSE, pid)) {
    WaitForSingleObject(process, 60000);
    CloseHandle(process);
  }
}

void Compat::prepare()
{
  g_prepared = true;
  mediafoundation::probe();

  QElapsedTimer probeTime;
  probeTime.start();
  QJsonObject state = loadState();
  const Adapter adapter = defaultAdapter();
  // The feature level needs a hardware device (the driver loads, ~100 ms
  // here): probed once per adapter identity (ids and driver version), then
  // read from the file.
  D3D_FEATURE_LEVEL level{};
  const bool cached = adapter.found &&
                      state.value(QLatin1String("probedAdapter")).toString() == adapter.identity &&
                      state.contains(QLatin1String("featureLevel"));
  if (cached) {
    level = D3D_FEATURE_LEVEL(state.value(QLatin1String("featureLevel")).toInt());
  } else {
    level = bestFeatureLevel();
    state.insert(QLatin1String("probedAdapter"), adapter.identity);
    state.insert(QLatin1String("featureLevel"), int(level));
  }
  if (adapter.found)
    std::fprintf(stderr, "[gpu] adapter %s (%s), feature level %s (%s, %lld ms)\n",
                 qPrintable(adapter.name), qPrintable(adapter.identity),
                 level ? qPrintable(levelName(level)) : "none", cached ? "cached" : "probed",
                 static_cast<long long>(probeTime.elapsed()));
  else
    std::fprintf(stderr, "[gpu] no DXGI adapter\n");

  bool saved = state.value(QLatin1String("safeMode")).toBool();
  // Hardware acceleration turned off in Settings: kept across driver changes.
  const bool userChoice = saved && state.value(QLatin1String("user")).toBool();
  if (saved && hasArg(L"--no-gpu-safe-mode")) {
    std::fprintf(stderr, "[gpu] safe mode cleared (--no-gpu-safe-mode)\n");
    saved = false;
  } else if (saved && !userChoice &&
             state.value(QLatin1String("adapter")).toString() != adapter.identity) {
    std::fprintf(stderr, "[gpu] safe mode cleared: the adapter or its driver changed (was %s)\n",
                 qPrintable(state.value(QLatin1String("adapter")).toString()));
    saved = false;
  }
  if (!saved && state.value(QLatin1String("safeMode")).toBool()) {
    state.insert(QLatin1String("safeMode"), false);
    state.remove(QLatin1String("reason"));
    state.remove(QLatin1String("user"));
  }

  // The crash guard: two runs in a row ended before drawing anything.
  const int unframed = state.value(QLatin1String("startsWithoutFrame")).toInt();
  if (unframed >= 2 && !saved) {
    saved = true;
    state.insert(QLatin1String("safeMode"), true);
    state.insert(QLatin1String("user"), false);
    state.insert(QLatin1String("reason"),
                 QStringLiteral("The last %1 starts closed before the window was drawn.").arg(unframed));
    state.insert(QLatin1String("adapter"), adapter.identity);
    state.insert(QLatin1String("since"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
    state.insert(QLatin1String("noticeSeen"), false);
    std::fprintf(stderr, "[gpu] crash guard: %d starts in a row ended before their first frame\n", unframed);
  }
  g_unframedStarts = (unframed >= 2 ? 0 : unframed) + 1;
  state.insert(QLatin1String("startsWithoutFrame"), g_unframedStarts);

  if (qEnvironmentVariableIntValue("QSG_RHI_PREFER_SOFTWARE_RENDERER") != 0) {
    g_gpu = { true, QStringLiteral("QSG_RHI_PREFER_SOFTWARE_RENDERER is set."), false, false };
  } else if (hasArg(L"--gpu-safe-mode") || envIsOne("YAP_GPU_SAFE_MODE")) {
    g_gpu = { true, QStringLiteral("Safe mode was asked for (--gpu-safe-mode)."), false, false };
  } else if (!level) {
    g_gpu = { true, QStringLiteral("No Direct3D 11 graphics device is available."), true, false };
  } else if (level < D3D_FEATURE_LEVEL_11_0) {
    g_gpu = { true,
              QStringLiteral("The graphics card supports Direct3D feature level %1; Yap Native "
                             "needs 11_0.")
                .arg(levelName(level)),
              true, false };
  } else if (saved) {
    // Turned off in Settings: no notice, the switch shows it.
    g_gpu = { true, state.value(QLatin1String("reason")).toString(),
              !userChoice && !state.value(QLatin1String("noticeSeen")).toBool(), true };
  }
  g_savedSafeMode = saved;
  g_userChoice = saved && userChoice;
  saveState(state);

  if (g_gpu.safe) {
    applySafeMode();
    std::fprintf(stderr, "[gpu] safe mode: drawing with WARP, no GPU decode or encode (%s)\n",
                 qPrintable(g_gpu.reason));
  }
}

void Compat::quitting()
{
  if (g_prepared && g_unframedStarts > 0)
    updateState("startsWithoutFrame", 0);
  g_unframedStarts = 0;
}

void Compat::attach(QQuickWindow* window)
{
  m_window = window;
  if (!window)
    return;
  connect(window, &QQuickWindow::sceneGraphError, this,
          [this](QQuickWindow::SceneGraphError, const QString& message) { sceneGraphError(message); });
  // Direct: in the basic render loop the invalidation runs on this thread,
  // inside the frame that found the device lost, before the next QRhi is made.
  connect(window, &QQuickWindow::sceneGraphInvalidated, this, &Compat::sceneGraphInvalidated,
          Qt::DirectConnection);
  connect(window, &QQuickWindow::frameSwapped, this, &Compat::firstFrame, Qt::SingleShotConnection);

  // A start into the tray draws nothing until it is shown: the marker waits
  // for that show instead of counting this run.
  QTimer::singleShot(0, this, [this] {
    if (!m_window || m_window->isVisible() || g_unframedStarts == 0)
      return;
    updateState("startsWithoutFrame", g_unframedStarts - 1);
    connect(m_window, &QWindow::visibleChanged, this, [](bool visible) {
      if (visible && g_unframedStarts > 0)
        updateState("startsWithoutFrame", g_unframedStarts);
    });
  });

  // YAP_TEST_STARTUP_CRASH=1: the process ends here, after the crash guard
  // counted it and before the window is shown (tests).
  if (envIsOne("YAP_TEST_STARTUP_CRASH")) {
    std::fprintf(stderr, "[gpu] YAP_TEST_STARTUP_CRASH=1: ending before the first frame\n");
    std::fflush(stderr);
    TerminateProcess(GetCurrentProcess(), 0xDEAD);
  }
}

void Compat::firstFrame()
{
  if (g_unframedStarts > 0)
    updateState("startsWithoutFrame", 0);
  g_unframedStarts = 0;
  if (m_mediaNotice)
    updateState("mediaNoticeSeen", true);

  // YAP_TEST_GPU_FAIL=1 (tests): 2 s after the first frame, the scene
  // graph's error handler runs as if the GPU could not start.
  if (envIsOne("YAP_TEST_GPU_FAIL"))
    QTimer::singleShot(2000, this, [this] {
      std::fprintf(stderr, "[gpu] YAP_TEST_GPU_FAIL=1: as if the scene graph failed\n");
      sceneGraphError(QStringLiteral("YAP_TEST_GPU_FAIL"));
    });
  // YAP_TEST_DEVICE_LOSS=<s> (tests): s seconds after the first frame, Qt's
  // QRhi reports the device lost, as after a TDR.
  if (const int secs = qEnvironmentVariableIntValue("YAP_TEST_DEVICE_LOSS"); secs > 0)
    QTimer::singleShot(secs * 1000, this, [this] {
      // Our shared device stays healthy, so the recovery is told; without
      // one, the QRhi's own lost flag is what it sees.
      const bool marked = markRhiLost(m_window);
      m_simulateLoss = marked && VideoHub::sharing();
      std::fprintf(stderr, "[gpu] YAP_TEST_DEVICE_LOSS: %s\n",
                   marked ? "the QRhi now reports its device lost" : "no D3D11 QRhi to lose");
      if (m_window)
        m_window->update();
    });
}

void Compat::sceneGraphError(const QString& message)
{
  std::fprintf(stderr, "[gpu] the scene graph failed: %s\n", qPrintable(message));
  restartInSafeMode(QStringLiteral("The graphics card could not start drawing the window."));
}

bool Compat::countLoss(long reason, bool simulated)
{
  ++m_losses;
  std::fprintf(stderr, "[gpu] device lost (%s0x%08lx), loss %d this run\n",
               simulated ? "simulated, " : "", static_cast<unsigned long>(reason), m_losses);
  return m_losses >= 3;
}

void Compat::sceneGraphInvalidated()
{
  // Our shared device removed, or Qt's QRhi lost its device (its own device
  // without zero copy: YAP_ZERO_COPY=0, sharing failed, or WARP).
  long reason = 0;
  const bool simulated = m_simulateLoss.exchange(false);
  const bool removed = VideoHub::deviceRemoved(&reason);
  QRhi* rhi = m_window ? m_window->rhi() : nullptr;
  if (!simulated && !removed && !(rhi && rhi->isDeviceLost()))
    return; // a teardown, not a loss
  if (!m_window || QThread::currentThread() != m_window->thread()) {
    // The threaded render loop (QSG_RENDER_LOOP=threaded): counted and
    // handled on the GUI thread; not replaced in place.
    QMetaObject::invokeMethod(this, [this, reason, simulated] {
      countLoss(reason, simulated);
      restartInSafeMode(QStringLiteral("The graphics device was lost."));
    }, Qt::QueuedConnection);
    return;
  }
  if (countLoss(reason, simulated)) {
    QMetaObject::invokeMethod(this, [this] {
      restartInSafeMode(QStringLiteral("The graphics device was lost three times."));
    }, Qt::QueuedConnection);
    return;
  }
  if (!VideoHub::sharing()) {
    std::fprintf(stderr, "[gpu] Qt makes a new device for the next frame\n");
  } else if (!VideoHub::replaceDevice(m_window)) {
    QMetaObject::invokeMethod(this, [this] {
      restartInSafeMode(QStringLiteral("A lost graphics device could not be replaced."));
    }, Qt::QueuedConnection);
    return;
  }
  // Nothing may be asking for a frame: the next one makes the QRhi again.
  QTimer::singleShot(0, m_window, [window = m_window] {
    if (window)
      window->update();
  });
}

void Compat::restartInSafeMode(const QString& reason)
{
  // Once: a second error or loss before the event loop ends changes nothing.
  static bool restarting = false;
  if (restarting)
    return;
  restarting = true;
  if (gpuSafeMode()) {
    // Saved with its reason, so the next start says why (and stays safe).
    QString why = reason;
    if (why.endsWith(QLatin1Char('.')))
      why.chop(1);
    saveSafeMode(QStringLiteral("The window could not be drawn even without the graphics card (%1).")
                   .arg(why),
                 g_userChoice);
    g_unframedStarts = 0;
    std::fprintf(stderr, "[gpu] %s Already in safe mode: quitting.\n", qPrintable(reason));
    std::fflush(stderr);
    QCoreApplication::exit(3);
    return;
  }
  saveSafeMode(reason);
  g_unframedStarts = 0;
  // The test switches would only fail the new process again.
  SetEnvironmentVariableW(L"YAP_TEST_GPU_FAIL", nullptr);
  SetEnvironmentVariableW(L"YAP_TEST_DEVICE_LOSS", nullptr);
  std::wstring line = relaunchCommandLine(GetCurrentProcessId());
  wchar_t exe[MAX_PATH * 2];
  const DWORD n = GetModuleFileNameW(nullptr, exe, DWORD(std::size(exe)));
  STARTUPINFOW startup{};
  startup.cb = sizeof startup;
  PROCESS_INFORMATION info{};
  if (n > 0 && CreateProcessW(exe, line.data(), nullptr, nullptr, FALSE, 0, nullptr, nullptr,
                              &startup, &info)) {
    std::fprintf(stderr, "[gpu] %s Restarting in safe mode as pid %lu.\n", qPrintable(reason),
                 static_cast<unsigned long>(info.dwProcessId));
    CloseHandle(info.hThread);
    CloseHandle(info.hProcess);
  } else {
    std::fprintf(stderr, "[gpu] %s Safe mode is saved for the next start (restart failed: %lu).\n",
                 qPrintable(reason), GetLastError());
  }
  std::fflush(stderr);
  QCoreApplication::exit(3);
}

bool Compat::mediaPlayback() const
{
  return mediafoundation::available();
}

bool Compat::gpuSafeMode() const
{
  return g_gpu.safe;
}

QString Compat::gpuReason() const
{
  return g_gpu.reason;
}

bool Compat::gpuSaved() const
{
  return g_gpu.saved;
}

void Compat::dismissMedia()
{
  updateState("mediaNoticeSeen", true);
  if (m_mediaNotice) {
    m_mediaNotice = false;
    emit mediaNoticeChanged();
  }
}

void Compat::dismissGpu()
{
  if (g_gpu.saved)
    updateState("noticeSeen", true);
  if (m_gpuNotice) {
    m_gpuNotice = false;
    emit gpuNoticeChanged();
  }
}

void Compat::useGpuNextTime()
{
  setHardwareAcceleration(true);
  if (m_gpuNotice) {
    m_gpuNotice = false;
    emit gpuNoticeChanged();
  }
}

bool Compat::hardwareAcceleration() const
{
  return !g_savedSafeMode;
}

bool Compat::accelerationPending() const
{
  // The next start draws differently from this one (a safe mode this run
  // that the choice cannot lift, a missing feature level, is not pending).
  return g_savedSafeMode ? !g_gpu.safe : (g_gpu.safe && g_gpu.saved);
}

void Compat::setHardwareAcceleration(bool on)
{
  if (on == !g_savedSafeMode)
    return;
  if (on) {
    QJsonObject state = loadState();
    state.insert(QLatin1String("safeMode"), false);
    state.remove(QLatin1String("reason"));
    state.remove(QLatin1String("user"));
    saveState(state);
    std::fprintf(stderr, "[gpu] hardware acceleration on: the GPU is used from the next start\n");
  } else {
    saveSafeMode(kUserReason, true);
    std::fprintf(stderr, "[gpu] hardware acceleration off: safe mode from the next start\n");
  }
  g_savedSafeMode = !on;
  g_userChoice = !on;
  emit hardwareAccelerationChanged();
}
