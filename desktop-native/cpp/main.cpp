// Entry point: a Qt Quick app whose logic lives in the Rust crate
// (YapClient, registered by the com.yap.native QML module).

#include <QtCore/QElapsedTimer>
#include <QtGui/QGuiApplication>
#include <QtGui/QWindow>
#include <QtQml/QQmlApplicationEngine>
#include <QtQuick/QQuickWindow>
#include <QtQuickControls2/QQuickStyle>

#include <cstdio>

#ifdef Q_OS_WIN
#include <dwmapi.h>
#include <windows.h>

// Native title bar in Yap's colours: dark mode, black caption, no border
// tint. Keeps Windows snap layouts, shadows and resize for free.
static void styleTitleBar(QWindow* window)
{
  const HWND hwnd = reinterpret_cast<HWND>(window->winId());
  const BOOL dark = TRUE;
  DwmSetWindowAttribute(hwnd, 20 /* DWMWA_USE_IMMERSIVE_DARK_MODE */, &dark,
                        sizeof dark);
  const COLORREF caption = RGB(0, 0, 0);
  DwmSetWindowAttribute(hwnd, 35 /* DWMWA_CAPTION_COLOR */, &caption,
                        sizeof caption);
  const COLORREF border = RGB(0x1b, 0x1b, 0x1f);
  DwmSetWindowAttribute(hwnd, 34 /* DWMWA_BORDER_COLOR */, &border,
                        sizeof border);
}
#endif

int main(int argc, char* argv[])
{
  QElapsedTimer startup;
  startup.start();

  QGuiApplication::setApplicationName(QStringLiteral("Yap Native"));
  QGuiApplication::setOrganizationName(QStringLiteral("Yap"));
  QGuiApplication app(argc, argv);
  QQuickStyle::setStyle(QStringLiteral("Basic"));

  QQmlApplicationEngine engine;
  QObject::connect(
    &engine, &QQmlApplicationEngine::objectCreationFailed, &app,
    [] { QCoreApplication::exit(-1); }, Qt::QueuedConnection);
  engine.loadFromModule("com.yap.native", "Main");

  if (engine.rootObjects().isEmpty())
    return -1;

  if (auto* window =
        qobject_cast<QQuickWindow*>(engine.rootObjects().first())) {
#ifdef Q_OS_WIN
    styleTitleBar(window);
#endif
    // First frame on screen: printed for the cold-start measurement.
    QObject::connect(
      window, &QQuickWindow::frameSwapped, window,
      [&startup] {
        std::fprintf(stderr, "[startup] first frame after %lld ms\n",
                     static_cast<long long>(startup.elapsed()));
        std::fflush(stderr);
      },
      Qt::SingleShotConnection);
  }

  return app.exec();
}
