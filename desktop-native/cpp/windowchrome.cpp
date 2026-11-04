#include "windowchrome.h"

#include <QtQuick/QQuickWindow>

#ifdef Q_OS_WIN
#include <dwmapi.h>
#include <windows.h>
#include <windowsx.h>
#endif

WindowChrome::WindowChrome(QObject* parent)
  : QObject(parent)
{
}

void WindowChrome::attach(QQuickWindow* window)
{
#ifdef Q_OS_WIN
  const HWND hwnd = reinterpret_cast<HWND>(window->winId());
  m_window = window;
  m_hwnd = reinterpret_cast<quintptr>(hwnd);
  applyFrameColors();
  // Re-run WM_NCCALCSIZE now that the filter is in place.
  SetWindowPos(hwnd, nullptr, 0, 0, 0, 0,
               SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER |
                 SWP_NOACTIVATE);
#else
  m_window = window;
#endif
}

void WindowChrome::setTitleBar(QQuickItem* item)
{
  if (m_titleBar == item)
    return;
  m_titleBar = item;
  emit titleBarChanged();
}

void WindowChrome::setMaximizeButton(QQuickItem* item)
{
  if (m_maximizeButton == item)
    return;
  m_maximizeButton = item;
  emit maximizeButtonChanged();
}

void WindowChrome::setHitTestVisible(QQuickItem* item, bool visible)
{
  m_hitTestVisible.removeIf(
    [item](const QPointer<QQuickItem>& p) { return !p || p == item; });
  if (visible && item)
    m_hitTestVisible.append(item);
}

void WindowChrome::setDark(bool dark)
{
  if (m_dark == dark)
    return;
  m_dark = dark;
  applyFrameColors();
  emit darkChanged();
}

void WindowChrome::setBorderColor(const QColor& color)
{
  if (m_borderColor == color)
    return;
  m_borderColor = color;
  applyFrameColors();
  emit borderColorChanged();
}

void WindowChrome::setMaximizeHovered(bool hovered)
{
  if (m_maximizeHovered == hovered)
    return;
  m_maximizeHovered = hovered;
  emit maximizeHoveredChanged();
}

void WindowChrome::setMaximizePressed(bool pressed)
{
  if (m_maximizePressed == pressed)
    return;
  m_maximizePressed = pressed;
  emit maximizePressedChanged();
}

void WindowChrome::toggleMaximized()
{
  if (!m_window)
    return;
#ifdef Q_OS_WIN
  const HWND hwnd = reinterpret_cast<HWND>(m_hwnd);
  ShowWindow(hwnd, IsZoomed(hwnd) ? SW_RESTORE : SW_MAXIMIZE);
#else
  if (m_window->visibility() == QWindow::Maximized)
    m_window->showNormal();
  else
    m_window->showMaximized();
#endif
}

void WindowChrome::applyFrameColors()
{
#ifdef Q_OS_WIN
  if (!m_hwnd)
    return;
  const HWND hwnd = reinterpret_cast<HWND>(m_hwnd);
  const BOOL dark = m_dark;
  DwmSetWindowAttribute(hwnd, 20 /* DWMWA_USE_IMMERSIVE_DARK_MODE */, &dark,
                        sizeof dark);
  const COLORREF border = RGB(m_borderColor.red(), m_borderColor.green(),
                              m_borderColor.blue());
  DwmSetWindowAttribute(hwnd, 34 /* DWMWA_BORDER_COLOR */, &border,
                        sizeof border);
#endif
}

bool WindowChrome::contains(QQuickItem* item, const QPointF& scenePos) const
{
  if (!item || !item->isVisible())
    return false;
  return item->contains(item->mapFromScene(scenePos));
}

bool WindowChrome::nativeEventFilter(const QByteArray& eventType,
                                     void* message, qintptr* result)
{
#ifdef Q_OS_WIN
  if (!m_hwnd || !m_window || eventType != "windows_generic_MSG")
    return false;
  MSG* msg = static_cast<MSG*>(message);
  const HWND hwnd = reinterpret_cast<HWND>(m_hwnd);
  if (msg->hwnd != hwnd)
    return false;

  const UINT dpi = GetDpiForWindow(hwnd);
  // The resize border Windows uses: frame plus padded border, per DPI.
  const int frameY = GetSystemMetricsForDpi(SM_CYFRAME, dpi) +
                     GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);

  switch (msg->message) {
    case WM_NCCALCSIZE: {
      if (!msg->wParam)
        return false;
      // Let Windows size the frame, then give the caption to the client.
      auto* params = reinterpret_cast<NCCALCSIZE_PARAMS*>(msg->lParam);
      const LONG top = params->rgrc[0].top;
      const LRESULT r =
        DefWindowProcW(hwnd, msg->message, msg->wParam, msg->lParam);
      if (r != 0) {
        *result = r;
        return true;
      }
      params->rgrc[0].top = top;
      // Maximised, the frame hangs off the monitor: inset it like the sides.
      if (IsZoomed(hwnd))
        params->rgrc[0].top += frameY;
      *result = 0;
      return true;
    }

    case WM_NCHITTEST: {
      const LRESULT def =
        DefWindowProcW(hwnd, msg->message, msg->wParam, msg->lParam);
      if (def != HTCLIENT) {
        *result = def;
        return true;
      }
      POINT pt{ GET_X_LPARAM(msg->lParam), GET_Y_LPARAM(msg->lParam) };
      ScreenToClient(hwnd, &pt);
      RECT client{};
      GetClientRect(hwnd, &client);
      // The top resize band lives inside the client now.
      if (!IsZoomed(hwnd) && pt.y < frameY) {
        const int frameX = GetSystemMetricsForDpi(SM_CXFRAME, dpi) +
                           GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
        *result = pt.x < frameX                   ? HTTOPLEFT
                  : pt.x >= client.right - frameX ? HTTOPRIGHT
                                                  : HTTOP;
        return true;
      }
      const qreal ratio = m_window->devicePixelRatio();
      const QPointF scenePos(pt.x / ratio, pt.y / ratio);
      if (contains(m_maximizeButton, scenePos)) {
        *result = HTMAXBUTTON;
        return true;
      }
      for (const auto& item : std::as_const(m_hitTestVisible)) {
        if (contains(item, scenePos)) {
          *result = HTCLIENT;
          return true;
        }
      }
      *result = contains(m_titleBar, scenePos) ? HTCAPTION : HTCLIENT;
      return true;
    }

    // Over HTMAXBUTTON Windows sends non-client mouse messages; mirror them
    // into QML and click the key ourselves (the default would draw and track
    // a classic caption button).
    case WM_NCMOUSEMOVE: {
      const bool over = msg->wParam == HTMAXBUTTON;
      if (over && !m_maximizeHovered) {
        TRACKMOUSEEVENT track{ sizeof(TRACKMOUSEEVENT),
                               TME_LEAVE | TME_NONCLIENT, hwnd, 0 };
        TrackMouseEvent(&track);
      }
      setMaximizeHovered(over);
      if (!over)
        setMaximizePressed(false);
      return false;
    }
    case WM_NCMOUSELEAVE:
    case WM_MOUSEMOVE:
      setMaximizeHovered(false);
      setMaximizePressed(false);
      return false;
    case WM_NCLBUTTONDOWN:
    case WM_NCLBUTTONDBLCLK:
      if (msg->wParam != HTMAXBUTTON)
        return false;
      setMaximizePressed(true);
      *result = 0;
      return true;
    case WM_NCLBUTTONUP:
      if (msg->wParam != HTMAXBUTTON)
        return false;
      if (m_maximizePressed) {
        setMaximizePressed(false);
        toggleMaximized();
      }
      *result = 0;
      return true;
    default:
      return false;
  }
#else
  Q_UNUSED(eventType);
  Q_UNUSED(message);
  Q_UNUSED(result);
  return false;
#endif
}
