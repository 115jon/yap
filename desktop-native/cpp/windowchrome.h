#pragma once

// The custom 32 px title bar on a normal (captioned) Win32 window:
// WM_NCCALCSIZE drops the caption but keeps the frame, so the DWM shadow,
// rounded corners, Aero snap and resize borders stay native. WM_NCHITTEST
// maps the QML bar to HTCAPTION, its keys to HTCLIENT and the maximise key
// to HTMAXBUTTON, which is what opens Windows 11 snap layouts. Windows then
// owns the mouse over that key, so its hover and press come back to QML
// through `maximizeHovered` / `maximizePressed`.
//
// QML (import Yap.Chrome):
//   WindowChrome.titleBar = bar
//   WindowChrome.maximizeButton = maxKey
//   WindowChrome.setHitTestVisible(key, true)   // keys inside the bar

#include <QtCore/QAbstractNativeEventFilter>
#include <QtCore/QList>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtGui/QColor>
#include <QtQuick/QQuickItem>

class QQuickWindow;

class WindowChrome : public QObject, public QAbstractNativeEventFilter
{
  Q_OBJECT
  Q_PROPERTY(QQuickItem* titleBar READ titleBar WRITE setTitleBar NOTIFY
               titleBarChanged)
  Q_PROPERTY(QQuickItem* maximizeButton READ maximizeButton WRITE
               setMaximizeButton NOTIFY maximizeButtonChanged)
  Q_PROPERTY(
    bool maximizeHovered READ maximizeHovered NOTIFY maximizeHoveredChanged)
  Q_PROPERTY(
    bool maximizePressed READ maximizePressed NOTIFY maximizePressedChanged)
  // Frame colours for DWM (border, system menu): follow the theme.
  Q_PROPERTY(bool dark READ dark WRITE setDark NOTIFY darkChanged)
  Q_PROPERTY(QColor borderColor READ borderColor WRITE setBorderColor NOTIFY
               borderColorChanged)

public:
  explicit WindowChrome(QObject* parent = nullptr);

  // Call once the window exists; restyles its frame.
  void attach(QQuickWindow* window);

  QQuickItem* titleBar() const { return m_titleBar; }
  void setTitleBar(QQuickItem* item);
  QQuickItem* maximizeButton() const { return m_maximizeButton; }
  void setMaximizeButton(QQuickItem* item);
  bool maximizeHovered() const { return m_maximizeHovered; }
  bool maximizePressed() const { return m_maximizePressed; }
  bool dark() const { return m_dark; }
  void setDark(bool dark);
  QColor borderColor() const { return m_borderColor; }
  void setBorderColor(const QColor& color);

  // Items inside the title bar that take clicks themselves.
  Q_INVOKABLE void setHitTestVisible(QQuickItem* item, bool visible = true);
  Q_INVOKABLE void toggleMaximized();

  bool nativeEventFilter(const QByteArray& eventType, void* message,
                         qintptr* result) override;

signals:
  void titleBarChanged();
  void maximizeButtonChanged();
  void maximizeHoveredChanged();
  void maximizePressedChanged();
  void darkChanged();
  void borderColorChanged();

private:
  void setMaximizeHovered(bool hovered);
  void setMaximizePressed(bool pressed);
  void applyFrameColors();
  bool contains(QQuickItem* item, const QPointF& scenePos) const;

  QPointer<QQuickWindow> m_window;
  // Set once the native window exists, so the filter never creates it.
  quintptr m_hwnd = 0;
  QPointer<QQuickItem> m_titleBar;
  QPointer<QQuickItem> m_maximizeButton;
  QList<QPointer<QQuickItem>> m_hitTestVisible;
  bool m_maximizeHovered = false;
  bool m_maximizePressed = false;
  bool m_dark = true;
  QColor m_borderColor = QColor(0x1b, 0x1b, 0x1f);
};
