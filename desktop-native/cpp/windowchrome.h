#pragma once

// The custom 32 px title bar on a normal (captioned) Win32 window:
// WM_NCCALCSIZE drops the caption but keeps the frame, so the DWM shadow,
// rounded corners, Aero snap and resize borders stay native. WM_NCHITTEST
// maps the QML bar to HTCAPTION, its keys to HTCLIENT, and the minimise,
// maximise and close keys to HTMINBUTTON, HTMAXBUTTON (Windows 11 snap
// layouts) and HTCLOSE. Windows owns the mouse over those three, so their
// hover and press come back to QML through `hoveredButton` /
// `pressedButton` and their clicks through `buttonClicked`. Being
// non-client, they also work while a modal popup's overlay takes every
// client click, as the desktop app's bar does above its web dialogs.
//
// QML (import Yap.Chrome):
//   WindowChrome.titleBar = bar
//   WindowChrome.minimizeButton = minKey   // likewise maximize, close
//   WindowChrome.setHitTestVisible(key, true)   // keys inside the bar

#include <QtCore/QAbstractNativeEventFilter>
#include <QtCore/QList>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QSize>
#include <QtGui/QColor>
#include <QtQuick/QQuickItem>

class QQuickWindow;

class WindowChrome : public QObject, public QAbstractNativeEventFilter
{
  Q_OBJECT
  Q_PROPERTY(QQuickItem* titleBar READ titleBar WRITE setTitleBar NOTIFY
               titleBarChanged)
  Q_PROPERTY(QQuickItem* minimizeButton READ minimizeButton WRITE
               setMinimizeButton NOTIFY buttonsChanged)
  Q_PROPERTY(QQuickItem* maximizeButton READ maximizeButton WRITE
               setMaximizeButton NOTIFY buttonsChanged)
  Q_PROPERTY(QQuickItem* closeButton READ closeButton WRITE setCloseButton
               NOTIFY buttonsChanged)
  // The caption key under the mouse or held: 8 minimise, 9 maximise,
  // 20 close (the HT codes), 0 for none.
  Q_PROPERTY(int hoveredButton READ hoveredButton NOTIFY hoveredButtonChanged)
  Q_PROPERTY(int pressedButton READ pressedButton NOTIFY pressedButtonChanged)
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
  QQuickItem* minimizeButton() const { return m_minimizeButton; }
  void setMinimizeButton(QQuickItem* item);
  QQuickItem* maximizeButton() const { return m_maximizeButton; }
  void setMaximizeButton(QQuickItem* item);
  QQuickItem* closeButton() const { return m_closeButton; }
  void setCloseButton(QQuickItem* item);
  int hoveredButton() const { return m_hoveredButton; }
  int pressedButton() const { return m_pressedButton; }
  bool dark() const { return m_dark; }
  void setDark(bool dark);
  QColor borderColor() const { return m_borderColor; }
  void setBorderColor(const QColor& color);

  // Items inside the title bar that take clicks themselves.
  Q_INVOKABLE void setHitTestVisible(QQuickItem* item, bool visible = true);
  Q_INVOKABLE void toggleMaximized();
  // Where the last left press in the client landed, in scene coordinates:
  // a modal popup's overlay reports a press on its scrim without one.
  Q_INVOKABLE QPointF lastPress() const { return m_lastPress; }

  bool nativeEventFilter(const QByteArray& eventType, void* message,
                         qintptr* result) override;

signals:
  void titleBarChanged();
  void buttonsChanged();
  void hoveredButtonChanged();
  void pressedButtonChanged();
  // A caption key was clicked (its HT code).
  void buttonClicked(int button);
  void darkChanged();
  void borderColorChanged();

private:
  void setHoveredButton(int button);
  void setPressedButton(int button);
  // The caption key at a scene point (its HT code), or 0.
  int buttonAt(const QPointF& scenePos) const;
  void applyFrameColors();
  bool contains(QQuickItem* item, const QPointF& scenePos) const;
  // The window size that keeps the client's logical size at `newDpi`.
  QSize scaledWindowSize(unsigned newDpi) const;

  QPointer<QQuickWindow> m_window;
  // Set once the native window exists, so the filter never creates it.
  quintptr m_hwnd = 0;
  // The window's DPI before the change being handled (WM_DPICHANGED).
  unsigned m_dpi = 0;
  QPointer<QQuickItem> m_titleBar;
  QPointer<QQuickItem> m_minimizeButton;
  QPointer<QQuickItem> m_maximizeButton;
  QPointer<QQuickItem> m_closeButton;
  QList<QPointer<QQuickItem>> m_hitTestVisible;
  int m_hoveredButton = 0;
  int m_pressedButton = 0;
  QPointF m_lastPress;
  bool m_dark = true;
  QColor m_borderColor = QColor(0x1b, 0x1b, 0x1f);
};
