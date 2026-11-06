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
  m_dpi = GetDpiForWindow(hwnd);
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

QSize WindowChrome::scaledWindowSize(unsigned newDpi) const
{
#ifdef Q_OS_WIN
  const HWND hwnd = reinterpret_cast<HWND>(m_hwnd);
  RECT client{};
  GetClientRect(hwnd, &client);
  const UINT oldDpi = m_dpi ? m_dpi : GetDpiForWindow(hwnd);
  const int frameX = GetSystemMetricsForDpi(SM_CXFRAME, newDpi) +
                     GetSystemMetricsForDpi(SM_CXPADDEDBORDER, newDpi);
  const int frameY = GetSystemMetricsForDpi(SM_CYFRAME, newDpi) +
                     GetSystemMetricsForDpi(SM_CXPADDEDBORDER, newDpi);
  return QSize(MulDiv(client.right, int(newDpi), int(oldDpi)) + 2 * frameX,
               MulDiv(client.bottom, int(newDpi), int(oldDpi)) + frameY);
#else
  Q_UNUSED(newDpi);
  return {};
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
    // A monitor with another scale: keep the client's logical size exact.
    // Qt scales the whole frame by the DPI ratio, but Windows rounds the
    // resize border per DPI (8 px at 96, 9 at 120, 11 at 168), so each move
    // between monitors grew or shrank the client by a pixel or two. The
    // size comes from the client and the new DPI's own border; our frame
    // has no top edge (WM_NCCALCSIZE below).
    case WM_GETDPISCALEDSIZE: {
      if (IsZoomed(hwnd) || IsIconic(hwnd) ||
          m_window->visibility() == QWindow::FullScreen)
        return false;
      const QSize size = scaledWindowSize(unsigned(msg->wParam));
      *reinterpret_cast<SIZE*>(msg->lParam) = SIZE{ size.width(), size.height() };
      *result = TRUE;
      return true;
    }
    case WM_DPICHANGED: {
      // Qt applies the suggested rectangle; make its size the exact one
      // too (a programmatic move gets a linearly scaled one).
      const UINT oldDpi = m_dpi ? m_dpi : dpi;
      const UINT newDpi = HIWORD(msg->wParam);
      if (!IsZoomed(hwnd) && !IsIconic(hwnd) &&
          m_window->visibility() != QWindow::FullScreen) {
        auto* rect = reinterpret_cast<RECT*>(msg->lParam);
        const QSize size = scaledWindowSize(newDpi);
        rect->right = rect->left + size.width();
        rect->bottom = rect->top + size.height();
        // A DPI change from Qt's own setGeometry (the saved rect restored
        // onto another monitor) keeps Qt's rect, which it framed with the
        // old DPI's border: the client comes out a few pixels short. Once
        // Qt is done, put the border it assumed back to the new one.
        const RECT suggested = *rect;
        QMetaObject::invokeMethod(
          this,
          [hwnd, suggested, oldDpi, newDpi] {
            RECT now{};
            GetWindowRect(hwnd, &now);
            if (IsZoomed(hwnd) || IsIconic(hwnd) || EqualRect(&now, &suggested) ||
                GetDpiForWindow(hwnd) != newDpi)
              return;
            const auto border = [](UINT d, int metric) {
              return GetSystemMetricsForDpi(metric, d) +
                     GetSystemMetricsForDpi(SM_CXPADDEDBORDER, d);
            };
            const int oldX = border(oldDpi, SM_CXFRAME);
            const int newX = border(newDpi, SM_CXFRAME);
            const int oldY = border(oldDpi, SM_CYFRAME);
            const int newY = border(newDpi, SM_CYFRAME);
            if (oldX == newX && oldY == newY)
              return;
            SetWindowPos(hwnd, nullptr, now.left + oldX - newX, now.top,
                         now.right - now.left - 2 * oldX + 2 * newX,
                         now.bottom - now.top - oldY + newY,
                         SWP_NOZORDER | SWP_NOACTIVATE);
          },
          Qt::QueuedConnection);
      }
      m_dpi = newDpi;
      return false;
    }

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
