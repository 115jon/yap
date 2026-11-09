#include "thumbbar.h"

#include <QtCore/QCoreApplication>
#include <QtGui/QImage>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtGui/QWindow>

#include <cstdio>
#include <iterator>

#include <windows.h>
#include <shobjidl.h>

namespace {

// thumbnail_toolbar.rs BUTTON_ID_*.
constexpr UINT kCamera = 0x4400;
constexpr UINT kMic = 0x4401;
constexpr UINT kDeafen = 0x4402;
constexpr UINT kDisconnect = 0x4403;
constexpr UINT kSeparator = 0x4404;
constexpr UINT kMediaToggle = 0x4405;
constexpr UINT kMediaSkip = 0x4406;

enum Icon
{
  CameraOff,
  CameraOn,
  DeafenOff,
  DeafenOn,
  Disconnect,
  MicOff,
  MicOn,
  Pause,
  Play,
  Skip,
  Separator,
  IconCount
};

// The desktop app's icon files (resources/thumbnail-toolbar), embedded as
// :/thumbbar.
const char* const kIconFiles[] = { "camera-off", "camera-on", "deafen-off",
                                   "deafen-on",  "disconnect", "mic-off",
                                   "mic-on" };

// thumbnail_toolbar.rs draws the media glyphs; the same shapes, in white.
QImage drawnIcon(Icon icon, int size)
{
  const int s = size * 4;
  QImage image(s, s, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  QPainter p(&image);
  p.setRenderHint(QPainter::Antialiasing);
  p.setPen(Qt::NoPen);
  const QColor ink(255, 255, 255, 245);
  auto rect = [&](qreal x, qreal y, qreal w, qreal h, qreal r, QColor c) {
    p.setBrush(c);
    p.drawRoundedRect(QRectF(x * s, y * s, w * s, h * s), r * s, r * s);
  };
  auto triangle = [&](QPointF a, QPointF b, QPointF c) {
    QPainterPath path;
    path.moveTo(a * s);
    path.lineTo(b * s);
    path.lineTo(c * s);
    path.closeSubpath();
    p.fillPath(path, ink);
  };
  switch (icon) {
    case Pause:
      rect(0.28, 0.20, 0.13, 0.60, 0.04, ink);
      rect(0.59, 0.20, 0.13, 0.60, 0.04, ink);
      break;
    case Play:
      triangle({ 0.31, 0.18 }, { 0.31, 0.82 }, { 0.74, 0.50 });
      break;
    case Skip:
      triangle({ 0.18, 0.20 }, { 0.18, 0.80 }, { 0.56, 0.50 });
      rect(0.65, 0.20, 0.10, 0.60, 0.03, ink);
      break;
    case Separator:
      rect(0.46, 0.18, 0.08, 0.64, 0.04, QColor(255, 255, 255, 110));
      break;
    default:
      break;
  }
  p.end();
  return image;
}

void setTip(THUMBBUTTON& button, const wchar_t* tip)
{
  if (!tip)
    return;
  button.dwMask = THUMBBUTTONMASK(button.dwMask | THB_TOOLTIP);
  wcsncpy_s(button.szTip, tip, _TRUNCATE);
}

THUMBBUTTONFLAGS flags(bool hidden, bool disabled)
{
  int value = THBF_ENABLED;
  if (hidden)
    value |= THBF_HIDDEN;
  if (disabled)
    value |= THBF_DISABLED;
  return THUMBBUTTONFLAGS(value);
}

} // namespace

ThumbnailToolbar::ThumbnailToolbar(QObject* parent)
  : QObject(parent)
{
  m_buttonCreated = RegisterWindowMessageW(L"TaskbarButtonCreated");
}

ThumbnailToolbar::~ThumbnailToolbar()
{
  if (QCoreApplication::instance())
    QCoreApplication::instance()->removeNativeEventFilter(this);
  destroyIcons();
}

void ThumbnailToolbar::attach(QWindow* window)
{
  m_window = window;
  m_hwnd = quintptr(window->winId());
  loadIcons();
  QCoreApplication::instance()->installNativeEventFilter(this);
}

QString ThumbnailToolbar::actionFor(unsigned id)
{
  switch (id) {
    case kCamera: return QStringLiteral("toggle-camera");
    case kMic: return QStringLiteral("toggle-mic");
    case kDeafen: return QStringLiteral("toggle-deafen");
    case kDisconnect: return QStringLiteral("disconnect");
    case kMediaToggle: return QStringLiteral("toggle-media-playback");
    case kMediaSkip: return QStringLiteral("skip-media");
    default: return {};
  }
}

void ThumbnailToolbar::loadIcons()
{
  destroyIcons();
  const int size = qMax(16, GetSystemMetrics(SM_CXICON));
  for (int i = 0; i < IconCount; ++i) {
    QImage image =
      i < int(std::size(kIconFiles))
        ? QImage(QStringLiteral(":/thumbbar/%1.png")
                   .arg(QLatin1String(kIconFiles[i])))
        : drawnIcon(Icon(i), size);
    if (image.isNull()) {
      std::fprintf(stderr, "[thumbbar] icon %d missing\n", i);
      continue;
    }
    image = image.scaled(size, size, Qt::IgnoreAspectRatio,
                         Qt::SmoothTransformation);
    m_icons[i] = quintptr(image.toHICON());
  }
}

void ThumbnailToolbar::destroyIcons()
{
  for (quintptr& icon : m_icons) {
    if (icon)
      DestroyIcon(reinterpret_cast<HICON>(icon));
    icon = 0;
  }
}

void ThumbnailToolbar::sync(bool visible, bool hasCamera, bool cameraOn,
                            bool hasMic, bool muted, bool deafened,
                            bool hasMedia, bool mediaPaused)
{
  State next;
  if (visible) {
    next = State{ true,     hasCamera, cameraOn, hasMic,
                  muted,    deafened,  hasMedia, mediaPaused };
  }
  if (next == m_state)
    return;
  m_state = next;
  std::fprintf(stderr,
               "[thumbbar] state visible=%d camera=%d/%d mic=%d muted=%d "
               "deafened=%d media=%d paused=%d\n",
               int(next.visible), int(next.hasCamera), int(next.cameraOn),
               int(next.hasMic), int(next.muted), int(next.deafened),
               int(next.hasMedia), int(next.mediaPaused));
  apply();
}

void ThumbnailToolbar::apply()
{
  // Nothing to add before Explorer has made the button, and nothing to
  // hide while none was ever added.
  if (!m_hwnd || !m_taskbarReady || (!m_added && !m_state.visible))
    return;
  ITaskbarList3* list = nullptr;
  HRESULT hr = CoCreateInstance(CLSID_TaskbarList, nullptr,
                                CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&list));
  if (FAILED(hr) || !list) {
    std::fprintf(stderr, "[thumbbar] no taskbar list: 0x%08lx\n",
                 static_cast<unsigned long>(hr));
    return;
  }
  hr = list->HrInit();

  const State& s = m_state;
  const bool hidden = !s.visible;
  const bool micOff = s.deafened || s.muted || !s.hasMic;
  auto icon = [this](Icon i) { return reinterpret_cast<HICON>(m_icons[i]); };
  THUMBBUTTON buttons[7] = {};
  for (THUMBBUTTON& b : buttons)
    b.dwMask = THUMBBUTTONMASK(THB_FLAGS | THB_ICON);

  buttons[0].iId = kCamera;
  buttons[0].hIcon = icon(s.cameraOn ? CameraOn : CameraOff);
  buttons[0].dwFlags = flags(hidden, !s.hasCamera);
  setTip(buttons[0], !s.hasCamera ? L"No camera detected"
                     : s.cameraOn ? L"Turn Off Camera"
                                  : L"Turn On Camera");

  buttons[1].iId = kMic;
  buttons[1].hIcon = icon(micOff ? MicOff : MicOn);
  buttons[1].dwFlags = flags(hidden, !s.hasMic);
  setTip(buttons[1], !s.hasMic ? L"No microphone detected"
                     : micOff  ? L"Unmute Microphone"
                               : L"Mute Microphone");

  buttons[2].iId = kDeafen;
  buttons[2].hIcon = icon(s.deafened ? DeafenOn : DeafenOff);
  buttons[2].dwFlags = flags(hidden, false);
  setTip(buttons[2], s.deafened ? L"Undeafen" : L"Deafen");

  buttons[3].iId = kDisconnect;
  buttons[3].hIcon = icon(Disconnect);
  buttons[3].dwFlags = flags(hidden, false);
  setTip(buttons[3], L"Disconnect");

  buttons[4].iId = kSeparator;
  buttons[4].hIcon = icon(Separator);
  buttons[4].dwFlags = hidden || !s.hasMedia
                         ? THBF_HIDDEN
                         : THUMBBUTTONFLAGS(THBF_NONINTERACTIVE | THBF_NOBACKGROUND);

  buttons[5].iId = kMediaToggle;
  buttons[5].hIcon = icon(s.mediaPaused ? Play : Pause);
  buttons[5].dwFlags = flags(hidden || !s.hasMedia, false);
  setTip(buttons[5], s.mediaPaused ? L"Resume Shared Playback"
                                   : L"Pause Shared Playback");

  buttons[6].iId = kMediaSkip;
  buttons[6].hIcon = icon(Skip);
  buttons[6].dwFlags = flags(hidden || !s.hasMedia, false);
  setTip(buttons[6], L"Skip Current Track");

  const HWND hwnd = reinterpret_cast<HWND>(m_hwnd);
  if (SUCCEEDED(hr)) {
    if (!m_added) {
      hr = list->ThumbBarAddButtons(hwnd, UINT(std::size(buttons)), buttons);
      m_added = SUCCEEDED(hr);
    } else {
      hr = list->ThumbBarUpdateButtons(hwnd, UINT(std::size(buttons)), buttons);
    }
  }
  list->Release();
  std::fprintf(stderr, "[thumbbar] %s -> 0x%08lx\n",
               m_added ? "applied" : "add failed",
               static_cast<unsigned long>(hr));
}

bool ThumbnailToolbar::nativeEventFilter(const QByteArray& eventType,
                                         void* message, qintptr* result)
{
  if (!m_hwnd || eventType != "windows_generic_MSG")
    return false;
  const MSG* msg = static_cast<const MSG*>(message);
  if (msg->hwnd != reinterpret_cast<HWND>(m_hwnd))
    return false;
  if (m_buttonCreated && msg->message == m_buttonCreated) {
    // A new taskbar button (first show, or Explorer restarted): its
    // toolbar is empty again.
    m_taskbarReady = true;
    m_added = false;
    std::fprintf(stderr, "[thumbbar] taskbar button created\n");
    apply();
    return false;
  }
  if (msg->message == WM_COMMAND && HIWORD(msg->wParam) == THBN_CLICKED) {
    const QString name = actionFor(LOWORD(msg->wParam));
    if (name.isEmpty())
      return false;
    std::fprintf(stderr, "[thumbbar] clicked %s\n", qPrintable(name));
    emit action(name);
    // A posted WM_COMMAND reaches the filter from the event dispatcher's
    // message loop, which passes no result.
    if (result)
      *result = 0;
    return true;
  }
  return false;
}
