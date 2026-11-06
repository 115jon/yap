#include "desktopshell.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QDateTime>
#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QFileInfo>
#include <QtCore/QMetaObject>
#include <QtCore/QStringList>
#include <QtGui/QFont>
#include <QtGui/QGuiApplication>
#include <QtGui/QImage>
#include <QtGui/QPainter>
#include <QtGui/QScreen>
#include <QtNetwork/QLocalServer>
#include <QtNetwork/QLocalSocket>
#include <QtQuick/QQuickWindow>

#include <algorithm>
#include <cstdio>
#include <string>
#include <utility>

#include "profile.h"
#include "toast_win.h"

#include <windows.h>
#include <shellapi.h>
#include <shobjidl.h>

namespace {

// This app's identity. Never the installed app's (com.115jon.yap, "Yap").
// A named profile (profile.h) suffixes every name below with the profile, so
// two profiles never share an instance, a toast identity or a Run value; the
// functions after this block build them.
constexpr wchar_t kAumidBase[] = L"com.115jon.yap.native";
constexpr wchar_t kAumidKeyBase[] =
  L"Software\\Classes\\AppUserModelId\\com.115jon.yap.native";
constexpr wchar_t kMutexBase[] = L"Local\\com.115jon.yap.native.instance";
constexpr char kPipe[] = "com.115jon.yap.native";
constexpr wchar_t kRunKey[] =
  L"Software\\Microsoft\\Windows\\CurrentVersion\\Run";
constexpr wchar_t kRunValueBase[] = L"YapNative";
constexpr wchar_t kTrayClassBase[] = L"YapNativeTrayWindow";
// The app's name in the tray and the tray menu; a profile adds its own.
constexpr wchar_t kAppName[] = L"Yap Native";
// Our deep-link scheme's key (yap_core::links::SCHEME_KEY). Never
// Software\Classes\yap, the installed app's.
constexpr wchar_t kSchemeKey[] = L"Software\\Classes\\yap-native";
// A forwarded argument longer than this is not a link we made: dropped at
// both ends of the pipe hand-off, and by openArg for our own command line.
constexpr int kMaxArgBytes = 4096;

constexpr UINT kTrayMessage = WM_APP + 1;
constexpr UINT kTrayId = 1;
constexpr UINT kMenuShow = 1;
constexpr UINT kMenuQuit = 2;
// tray.rs TRAY_ACTIVATION_DEBOUNCE.
constexpr qint64 kActivationDebounceMs = 500;
// window_state.rs SAVE_DEBOUNCE.
constexpr int kSaveDebounceMs = 500;

// desktop-tray.ts colours.
const QColor kBadgeRed(239, 68, 68);
const QColor kBadgeRing(11, 11, 13);

HANDLE g_instanceMutex = nullptr;

std::wstring aumid()
{
  return kAumidBase + profile::suffix(L'.');
}

std::wstring aumidKey()
{
  return kAumidKeyBase + profile::suffix(L'.');
}

std::wstring mutexName()
{
  return kMutexBase + profile::suffix(L'.');
}

// "YapNative", or "YapNative-<name>".
std::wstring runValue()
{
  return kRunValueBase + profile::suffix(L'-');
}

std::wstring trayClass()
{
  return kTrayClassBase + profile::suffix(L'-');
}

// The mutex is per Windows session (Local\); the pipe name is global, so it
// carries the session id too, or a second signed-in user would find the
// first user's pipe.
QString pipeName()
{
  DWORD session = 0;
  if (!ProcessIdToSessionId(GetCurrentProcessId(), &session))
    session = 0;
  return QString::fromLatin1(kPipe) +
         QString::fromStdWString(profile::suffix(L'.')) + QLatin1Char('.') +
         QString::number(session);
}

std::wstring wide(const QString& text)
{
  return text.toStdWString();
}

// The tray tooltip with the profile in it: "Yap Native (jon) - 3 unread
// notifications". The default profile's tooltip is unchanged.
QString trayTooltip(const QString& tooltip)
{
  if (profile::name().empty())
    return tooltip;
  const QString app = QString::fromWCharArray(kAppName);
  const QString named = QString::fromStdWString(profile::displayName());
  return tooltip.startsWith(app)
           ? named + tooltip.mid(app.size())
           : tooltip + QLatin1String(" (") +
               QString::fromStdWString(profile::name()) + QLatin1Char(')');
}

QString appDir()
{
  return QString::fromStdWString(profile::dataRoot());
}

// The toast icon Windows shows for this AUMID: the app mark, written once
// under our own folder.
QString toastIconPath()
{
  const QString dir = appDir();
  if (dir.isEmpty())
    return {};
  const QString path = dir + QStringLiteral("/toast-icon.png");
  if (!QFile::exists(path)) {
    QDir().mkpath(dir);
    QImage(QStringLiteral(":/app/yap-256.png")).save(path, "PNG");
  }
  return QDir::toNativeSeparators(path);
}

// HKCU\Software\Classes\AppUserModelId\com.115jon.yap.native[.<profile>]:
// the name and icon toasts show, so an unpackaged app can raise them without
// a Start menu shortcut. Only this key is written.
void registerAumid()
{
  HKEY key = nullptr;
  if (RegCreateKeyExW(HKEY_CURRENT_USER, aumidKey().c_str(), 0, nullptr, 0,
                      KEY_SET_VALUE, nullptr, &key, nullptr) != ERROR_SUCCESS)
    return;
  auto set = [key](const wchar_t* name, const std::wstring& value) {
    RegSetValueExW(key, name, 0, REG_SZ,
                   reinterpret_cast<const BYTE*>(value.c_str()),
                   static_cast<DWORD>((value.size() + 1) * sizeof(wchar_t)));
  };
  set(L"DisplayName", profile::displayName());
  const QString icon = toastIconPath();
  if (!icon.isEmpty())
    set(L"IconUri", wide(icon));
  RegCloseKey(key);
}

// The REG_SZ at HKCU `key` `name` (empty: the default value), or a null
// string when it is missing.
QString readRegString(const std::wstring& key, const std::wstring& name)
{
  DWORD size = 0;
  const wchar_t* value = name.empty() ? nullptr : name.c_str();
  if (RegGetValueW(HKEY_CURRENT_USER, key.c_str(), value, RRF_RT_REG_SZ,
                   nullptr, nullptr, &size) != ERROR_SUCCESS)
    return QString();
  std::wstring buffer(size / sizeof(wchar_t) + 1, L'\0');
  if (RegGetValueW(HKEY_CURRENT_USER, key.c_str(), value, RRF_RT_REG_SZ,
                   nullptr, buffer.data(), &size) != ERROR_SUCCESS)
    return QString();
  return QString::fromWCharArray(buffer.c_str());
}

LRESULT CALLBACK trayProc(HWND hwnd, UINT message, WPARAM wParam,
                          LPARAM lParam)
{
  auto* shell = reinterpret_cast<DesktopShell*>(
    GetWindowLongPtrW(hwnd, GWLP_USERDATA));
  if (shell && shell->handleTrayMessage(message, wParam, lParam))
    return 0;
  return DefWindowProcW(hwnd, message, wParam, lParam);
}

// desktop-tray.ts `drawBadgedIcon`: the app icon with a red badge in its
// top-right corner, a dark seam around it, and the count in white.
QImage badgedIcon(int count, bool dot, int size)
{
  constexpr int kCanvas = 64;
  QImage image = QImage(QStringLiteral(":/app/yap-256.png"))
                   .scaled(kCanvas, kCanvas, Qt::IgnoreAspectRatio,
                           Qt::SmoothTransformation)
                   .convertToFormat(QImage::Format_ARGB32_Premultiplied);
  if (count > 0 || dot) {
    const int radius = qMax(8, int(kCanvas * (dot || count > 99 ? 0.28 : 0.3)));
    const QPointF center(kCanvas - radius - 2, radius + 2);
    QPainter p(&image);
    p.setRenderHint(QPainter::Antialiasing);
    p.setPen(Qt::NoPen);
    p.setBrush(kBadgeRing);
    p.drawEllipse(center, radius + 1, radius + 1);
    p.setBrush(kBadgeRed);
    p.drawEllipse(center, radius, radius);
    if (!dot) {
      const QString label =
        count > 99 ? QStringLiteral("99+") : QString::number(count);
      QFont font;
      font.setBold(true);
      font.setPixelSize(label.size() > 1 ? radius - 2 : radius + 4);
      p.setFont(font);
      p.setPen(Qt::white);
      p.drawText(QRectF(center.x() - radius, center.y() - radius, radius * 2,
                        radius * 2),
                 Qt::AlignCenter, label);
    }
  }
  return image.scaled(size, size, Qt::IgnoreAspectRatio,
                      Qt::SmoothTransformation);
}

} // namespace

DesktopShell::DesktopShell(QObject* parent)
  : QObject(parent)
{
  m_saveTimer.setSingleShot(true);
  m_saveTimer.setInterval(kSaveDebounceMs);
  connect(&m_saveTimer, &QTimer::timeout, this, &DesktopShell::saveNow);
  connect(qApp, &QCoreApplication::aboutToQuit, this, [this] {
    saveNow();
    removeTray();
    toast::shutdown();
  });
}

DesktopShell::~DesktopShell()
{
  removeTray();
  if (m_trayHwnd)
    DestroyWindow(reinterpret_cast<HWND>(m_trayHwnd));
}

QString DesktopShell::profile() const
{
  return QString::fromStdWString(profile::name());
}

void DesktopShell::setProcessIdentity()
{
  SetCurrentProcessExplicitAppUserModelID(aumid().c_str());
}

bool DesktopShell::takeInstanceLock()
{
  HANDLE mutex = CreateMutexW(nullptr, FALSE, mutexName().c_str());
  // No mutex at all (access denied, say) proves nothing: run.
  if (!mutex || GetLastError() != ERROR_ALREADY_EXISTS) {
    g_instanceMutex = mutex; // held for the life of the process
    return true;
  }
  CloseHandle(mutex);
  return false;
}

void DesktopShell::showFirstInstance(const QStringList& args)
{
  // Let the first instance take the foreground when it reveals itself.
  AllowSetForegroundWindow(ASFW_ANY);
  // One line each: "show", then "arg <argument>" (the first instance
  // decides which are links).
  QByteArray message("show\n");
  for (QString arg : args) {
    arg.remove(QLatin1Char('\r')).remove(QLatin1Char('\n'));
    const QByteArray bytes = arg.toUtf8();
    if (!bytes.isEmpty() && bytes.size() <= kMaxArgBytes)
      message += "arg " + bytes + '\n';
  }
  for (int attempt = 0; attempt < 20; ++attempt) {
    QLocalSocket socket;
    socket.connectToServer(pipeName());
    if (socket.waitForConnected(250)) {
      socket.write(message);
      socket.waitForBytesWritten(500);
      socket.disconnectFromServer();
      std::fprintf(stderr, "[instance] already running; asked it to show\n");
      return;
    }
    Sleep(100); // the first instance may still be starting
  }
  std::fprintf(stderr, "[instance] already running; it did not answer\n");
}

void DesktopShell::attach(QQuickWindow* window, QObject* desktop)
{
  m_window = window;
  m_desktop = desktop;
  registerAumid();

  m_server = new QLocalServer(this);
  m_server->setSocketOptions(QLocalServer::UserAccessOption);
  connect(m_server, &QLocalServer::newConnection, this,
          &DesktopShell::onInstanceConnected);
  if (!m_server->listen(pipeName()))
    std::fprintf(stderr, "[instance] listen failed: %s\n",
                 qPrintable(m_server->errorString()));

  createTray();

  for (auto signal : { &QWindow::xChanged, &QWindow::yChanged,
                       &QWindow::widthChanged, &QWindow::heightChanged })
    connect(window, signal, this, &DesktopShell::scheduleSave);
  connect(window, &QWindow::windowStateChanged, this,
          &DesktopShell::scheduleSave);

  // Repairs the Run value when open on startup is on (desktop
  // `repair_autostart_entry`); never touched otherwise.
  if (desktop && desktop->property("openOnStartup").toBool())
    setAutostart(true);

  restoreGeometry();
  bool toTray = false;
  if (desktop)
    QMetaObject::invokeMethod(desktop, "launchToTray", Qt::DirectConnection,
                              Q_RETURN_ARG(bool, toTray));
  std::fprintf(stderr, "[window] launch to_tray=%d\n", int(toTray));
  if (toTray)
    return;
  if (window->windowStates() & Qt::WindowMaximized)
    window->showMaximized();
  else
    window->show();
}

void DesktopShell::restoreGeometry()
{
  if (!m_window || !m_desktop)
    return;
  const QList<QScreen*> screens = QGuiApplication::screens();
  QStringList areas;
  for (QScreen* screen : screens) {
    const QRect a = screen->availableGeometry();
    areas << QStringLiteral("%1,%2,%3,%4")
               .arg(a.x())
               .arg(a.y())
               .arg(a.width())
               .arg(a.height());
  }
  const int primary =
    qMax(0, int(screens.indexOf(QGuiApplication::primaryScreen())));
  QString placed;
  QMetaObject::invokeMethod(m_desktop, "windowPlacement", Qt::DirectConnection,
                            Q_RETURN_ARG(QString, placed),
                            Q_ARG(QString, areas.join(QLatin1Char(';'))),
                            Q_ARG(int, primary));
  const QStringList parts = placed.split(QLatin1Char(','));
  if (parts.size() != 5)
    return;
  const QRect rect(parts[0].toInt(), parts[1].toInt(), parts[2].toInt(),
                   parts[3].toInt());
  m_normal = rect;
  m_window->setGeometry(rect);
  if (parts[4] == QLatin1String("1"))
    m_window->setWindowStates(Qt::WindowMaximized);
  std::fprintf(stderr, "[window] restored %d,%d %dx%d maximized=%s\n",
               rect.x(), rect.y(), rect.width(), rect.height(),
               qPrintable(parts[4]));
}

void DesktopShell::scheduleSave()
{
  if (!m_window || !m_window->isVisible())
    return;
  const Qt::WindowStates states = m_window->windowStates();
  if (!(states & (Qt::WindowMaximized | Qt::WindowMinimized |
                  Qt::WindowFullScreen)))
    m_normal = m_window->geometry();
  m_saveTimer.start();
}

void DesktopShell::saveNow()
{
  m_saveTimer.stop();
  if (!m_window || !m_desktop)
    return;
  if (m_window->isVisible() &&
      !(m_window->windowStates() &
        (Qt::WindowMaximized | Qt::WindowMinimized | Qt::WindowFullScreen)))
    m_normal = m_window->geometry();
  if (!m_normal.isValid())
    return;
  const bool maximized = m_window->windowStates() & Qt::WindowMaximized;
  QMetaObject::invokeMethod(m_desktop, "saveWindowState", Qt::DirectConnection,
                            Q_ARG(int, m_normal.x()), Q_ARG(int, m_normal.y()),
                            Q_ARG(int, m_normal.width()),
                            Q_ARG(int, m_normal.height()),
                            Q_ARG(bool, maximized));
}

void DesktopShell::onInstanceConnected()
{
  while (QLocalSocket* socket = m_server->nextPendingConnection()) {
    connect(socket, &QLocalSocket::disconnected, socket,
            &QObject::deleteLater);
    connect(socket, &QLocalSocket::readyRead, this, [this, socket] {
      while (socket->canReadLine()) {
        const QByteArray line = socket->readLine().trimmed();
        if (line == "show") {
          std::fprintf(stderr, "[instance] launched again; revealing\n");
          reveal();
        } else if (line.startsWith("arg ") && line.size() - 4 <= kMaxArgBytes) {
          openArg(QString::fromUtf8(line.mid(4)));
        }
      }
    });
  }
}

void DesktopShell::setLinkHandler(QObject* links)
{
  m_links = links;
}

void DesktopShell::openArg(const QString& arg)
{
  if (!m_links || arg.toUtf8().size() > kMaxArgBytes)
    return;
  bool link = false;
  QMetaObject::invokeMethod(m_links, "openArg", Qt::DirectConnection,
                            Q_RETURN_ARG(bool, link), Q_ARG(QString, arg));
  if (link) {
    std::fprintf(stderr, "[link] opened from the command line\n");
    reveal();
  }
}

void DesktopShell::registerUrlScheme()
{
  // Only the default profile owns the scheme: it opens the default
  // profile's window, never a named one.
  if (!m_links || !profile::name().empty())
    return;
  const QString exe =
    QDir::toNativeSeparators(QCoreApplication::applicationFilePath());
  QString lines;
  QMetaObject::invokeMethod(m_links, "schemeRegistration", Qt::DirectConnection,
                            Q_RETURN_ARG(QString, lines), Q_ARG(QString, exe));
  // Only ever our own key, whatever the list says.
  const QString root = QString::fromWCharArray(kSchemeKey);
  // An isolated run (a `--config` argument) must not take the scheme from
  // another build that is still there: leave a registration whose
  // executable exists and is not this one.
  const QStringList args = QCoreApplication::arguments();
  const bool isolated =
    std::any_of(args.cbegin(), args.cend(), [](const QString& a) {
      return a == QLatin1String("--config") ||
             a.startsWith(QLatin1String("--config="));
    });
  if (isolated) {
    // The open command is `"<exe>" "%1"`: its quoted first token.
    QString other = readRegString(
                      wide(root + QLatin1String("\\shell\\open\\command")), L"")
                      .trimmed();
    if (other.startsWith(QLatin1Char('"')))
      other = other.mid(1, other.indexOf(QLatin1Char('"'), 1) - 1);
    else
      other = other.section(QLatin1Char(' '), 0, 0);
    if (!other.isEmpty() && QFileInfo::exists(other) &&
        QDir::cleanPath(other).compare(QDir::cleanPath(exe),
                                       Qt::CaseInsensitive) != 0) {
      std::fprintf(stderr, "[scheme] yap-native: isolated run, keeping the "
                           "existing registration\n");
      return;
    }
  }
  int written = 0;
  for (const QString& line : lines.split(QLatin1Char('\n'), Qt::SkipEmptyParts)) {
    const QStringList parts = line.split(QLatin1Char('\t'));
    if (parts.size() != 3)
      continue;
    const QString& key = parts[0];
    if (key.compare(root, Qt::CaseInsensitive) != 0 &&
        !key.startsWith(root + QLatin1Char('\\'), Qt::CaseInsensitive))
      continue;
    const std::wstring wkey = wide(key);
    const std::wstring wname = wide(parts[1]);
    const QString current = readRegString(wkey, wname);
    if (!current.isNull() && current == parts[2])
      continue;
    HKEY handle = nullptr;
    if (RegCreateKeyExW(HKEY_CURRENT_USER, wkey.c_str(), 0, nullptr, 0,
                        KEY_SET_VALUE, nullptr, &handle,
                        nullptr) != ERROR_SUCCESS)
      continue;
    const std::wstring data = wide(parts[2]);
    if (RegSetValueExW(handle, wname.empty() ? nullptr : wname.c_str(), 0,
                       REG_SZ, reinterpret_cast<const BYTE*>(data.c_str()),
                       static_cast<DWORD>((data.size() + 1) *
                                          sizeof(wchar_t))) == ERROR_SUCCESS)
      ++written;
    RegCloseKey(handle);
  }
  std::fprintf(stderr, "[scheme] yap-native: %d value(s) written\n", written);
}

quintptr DesktopShell::windowHandle() const
{
  return m_window ? quintptr(m_window->winId()) : 0;
}

void DesktopShell::reveal()
{
  if (!m_window)
    return;
  const HWND hwnd = reinterpret_cast<HWND>(windowHandle());
  if (!m_window->isVisible()) {
    if (m_window->windowStates() & Qt::WindowMaximized)
      m_window->showMaximized();
    else
      m_window->show();
  }
  if (IsIconic(hwnd))
    ShowWindow(hwnd, SW_RESTORE);
  m_window->raise();
  m_window->requestActivate();
  SetForegroundWindow(hwnd);
}

void DesktopShell::hideToTray()
{
  saveNow();
  if (m_window)
    m_window->hide();
  std::fprintf(stderr, "[window] closed to tray\n");
}

void DesktopShell::quit()
{
  saveNow();
  QCoreApplication::quit();
}

bool DesktopShell::setAutostart(bool enabled)
{
  HKEY key = nullptr;
  if (RegOpenKeyExW(HKEY_CURRENT_USER, kRunKey, 0, KEY_SET_VALUE, &key) !=
      ERROR_SUCCESS)
    return false;
  LONG result;
  if (enabled) {
    // desktop `build_windows_startup_command`: the quoted path, --autostart.
    // A named profile restarts as itself.
    const std::wstring command =
      L"\"" +
      wide(QDir::toNativeSeparators(QCoreApplication::applicationFilePath())) +
      L"\" --autostart" +
      (profile::name().empty() ? std::wstring()
                               : L" --profile " + profile::name());
    result = RegSetValueExW(
      key, runValue().c_str(), 0, REG_SZ, reinterpret_cast<const BYTE*>(command.c_str()),
      static_cast<DWORD>((command.size() + 1) * sizeof(wchar_t)));
  } else {
    result = RegDeleteValueW(key, runValue().c_str());
    if (result == ERROR_FILE_NOT_FOUND)
      result = ERROR_SUCCESS;
  }
  RegCloseKey(key);
  std::fprintf(stderr, "[autostart] %s -> %ld\n", enabled ? "on" : "off",
               long(result));
  return result == ERROR_SUCCESS;
}

// Tray

void DesktopShell::createTray()
{
  const HINSTANCE instance = GetModuleHandleW(nullptr);
  WNDCLASSW wc{};
  wc.lpfnWndProc = trayProc;
  wc.hInstance = instance;
  const std::wstring className = trayClass();
  wc.lpszClassName = className.c_str();
  RegisterClassW(&wc);
  // A hidden top-level window (not message-only: the tray menu needs a
  // window that can take the foreground).
  HWND hwnd = CreateWindowExW(0, className.c_str(), L"", WS_POPUP, 0, 0, 0, 0,
                              nullptr, nullptr, instance, nullptr);
  if (!hwnd)
    return;
  SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(this));
  m_trayHwnd = quintptr(hwnd);
  m_taskbarCreated = RegisterWindowMessageW(L"TaskbarCreated");
  m_badgeTooltip = QString::fromWCharArray(kAppName);
  updateTrayIcon();
}

void DesktopShell::removeTray()
{
  if (!m_trayAdded)
    return;
  NOTIFYICONDATAW data{};
  data.cbSize = sizeof(data);
  data.hWnd = reinterpret_cast<HWND>(m_trayHwnd);
  data.uID = kTrayId;
  Shell_NotifyIconW(NIM_DELETE, &data);
  m_trayAdded = false;
  if (m_icon) {
    DestroyIcon(reinterpret_cast<HICON>(m_icon));
    m_icon = 0;
  }
}

void DesktopShell::updateTrayIcon()
{
  m_badgeQueued = false;
  if (!m_trayHwnd)
    return;
  const int size = GetSystemMetrics(SM_CXSMICON) *
                   qMax(1, int(GetDpiForSystem() / 96.0 + 0.5));
  const QImage image = badgedIcon(m_badgeCount, m_badgeDot, qMax(16, size));
  const HICON icon = image.toHICON();
  // A development aid: the exact tray pixels, since Windows 11 parks new
  // tray icons in the overflow.
  const QString dump = qEnvironmentVariable("YAP_TRAY_DUMP");
  if (!dump.isEmpty())
    badgedIcon(m_badgeCount, m_badgeDot, 64).save(dump);

  NOTIFYICONDATAW data{};
  data.cbSize = sizeof(data);
  data.hWnd = reinterpret_cast<HWND>(m_trayHwnd);
  data.uID = kTrayId;
  data.uFlags = NIF_ICON | NIF_TIP | NIF_MESSAGE | NIF_SHOWTIP;
  data.uCallbackMessage = kTrayMessage;
  data.hIcon = icon;
  const std::wstring tip = wide(trayTooltip(m_badgeTooltip).left(127));
  wcsncpy_s(data.szTip, tip.c_str(), _TRUNCATE);
  if (!m_trayAdded) {
    m_trayAdded = Shell_NotifyIconW(NIM_ADD, &data);
    data.uVersion = NOTIFYICON_VERSION_4;
    Shell_NotifyIconW(NIM_SETVERSION, &data);
  } else {
    Shell_NotifyIconW(NIM_MODIFY, &data);
  }
  if (m_icon)
    DestroyIcon(reinterpret_cast<HICON>(m_icon));
  m_icon = quintptr(icon);
  std::fprintf(stderr, "[tray] badge count=%d dot=%d tip=\"%s\"\n",
               m_badgeCount, int(m_badgeDot),
               qPrintable(trayTooltip(m_badgeTooltip)));
}

void DesktopShell::showTrayMenu()
{
  const HWND hwnd = reinterpret_cast<HWND>(m_trayHwnd);
  HMENU menu = CreatePopupMenu();
  // tray.rs: "Show Yap" and "Quit", under this app's own name.
  AppendMenuW(menu, MF_STRING, kMenuShow,
              (L"Show " + profile::displayName()).c_str());
  AppendMenuW(menu, MF_STRING, kMenuQuit, L"Quit");
  POINT at;
  GetCursorPos(&at);
  SetForegroundWindow(hwnd);
  const UINT chosen =
    TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, at.x,
                   at.y, 0, hwnd, nullptr);
  DestroyMenu(menu);
  PostMessageW(hwnd, WM_NULL, 0, 0);
  if (chosen == kMenuShow) {
    const qint64 now = QDateTime::currentMSecsSinceEpoch();
    if (now - m_lastActivation >= kActivationDebounceMs) {
      m_lastActivation = now;
      reveal();
    }
  } else if (chosen == kMenuQuit) {
    quit();
  }
}

bool DesktopShell::handleTrayMessage(unsigned message, quintptr, qintptr lParam)
{
  if (message == m_taskbarCreated && m_taskbarCreated != 0) {
    // Explorer restarted: the icon is gone; add it again.
    m_trayAdded = false;
    updateTrayIcon();
    return true;
  }
  if (message != kTrayMessage)
    return false;
  switch (LOWORD(lParam)) {
    case NIN_SELECT:
    case NIN_KEYSELECT:
    case WM_LBUTTONUP: {
      const qint64 now = QDateTime::currentMSecsSinceEpoch();
      if (now - m_lastActivation >= kActivationDebounceMs) {
        m_lastActivation = now;
        reveal();
      }
      return true;
    }
    case WM_CONTEXTMENU:
    case WM_RBUTTONUP:
      showTrayMenu();
      return true;
    default:
      return false;
  }
}

// Badge

void DesktopShell::setBadgeCount(int count)
{
  if (count == m_badgeCount)
    return;
  m_badgeCount = count;
  emit badgeChanged();
  if (!std::exchange(m_badgeQueued, true))
    QMetaObject::invokeMethod(this, &DesktopShell::updateTrayIcon,
                              Qt::QueuedConnection);
}

void DesktopShell::setBadgeDot(bool dot)
{
  if (dot == m_badgeDot)
    return;
  m_badgeDot = dot;
  emit badgeChanged();
  if (!std::exchange(m_badgeQueued, true))
    QMetaObject::invokeMethod(this, &DesktopShell::updateTrayIcon,
                              Qt::QueuedConnection);
}

void DesktopShell::setBadgeTooltip(const QString& tooltip)
{
  if (tooltip == m_badgeTooltip || tooltip.isEmpty())
    return;
  m_badgeTooltip = tooltip;
  emit badgeChanged();
  if (!std::exchange(m_badgeQueued, true))
    QMetaObject::invokeMethod(this, &DesktopShell::updateTrayIcon,
                              Qt::QueuedConnection);
  // desktop-tray.ts: a cleared badge stops the taskbar flash.
  if (m_badgeCount == 0 && !m_badgeDot)
    flashTaskbar(false);
}

// Toasts

void DesktopShell::showToast(const QString& title, const QString& summary,
                             const QString& body, const QString& channelId,
                             const QString& serverId)
{
  // No tag: each message is its own toast, as on the desktop app.
  toast::Content content{ wide(title), wide(summary), wide(body), {} };
  QPointer<DesktopShell> self(this);
  const QString channel = channelId;
  const QString server = serverId;
  const bool shown = toast::show(aumid(), content, [self, channel, server] {
    // A WinRT thread: hop to the GUI thread.
    QMetaObject::invokeMethod(
      qApp,
      [self, channel, server] {
        if (!self)
          return;
        self->reveal();
        emit self->toastActivated(channel, server);
      },
      Qt::QueuedConnection);
  });
  std::fprintf(stderr, "[toast] %s\n", shown ? "shown" : "refused");
  // desktop-toasts.ts: every toast also flashes the taskbar button.
  flashTaskbar(true);
}

void DesktopShell::flashTaskbar(bool on)
{
  if (on == m_flashing && !on)
    return;
  const HWND hwnd = reinterpret_cast<HWND>(windowHandle());
  if (!hwnd)
    return;
  FLASHWINFO flash{};
  flash.cbSize = sizeof(flash);
  flash.hwnd = hwnd;
  flash.dwFlags = on ? (FLASHW_TRAY | FLASHW_TIMERNOFG) : FLASHW_STOP;
  FlashWindowEx(&flash);
  m_flashing = on;
}
