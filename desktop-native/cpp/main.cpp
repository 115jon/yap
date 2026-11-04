// Entry point: a Qt Quick app whose logic lives in the Rust crate
// (YapClient, registered by the com.yap.native QML module).

#include <QtCore/QDirIterator>
#include <QtCore/QElapsedTimer>
#include <QtGui/QFontDatabase>
#include <QtGui/QGuiApplication>
#include <QtGui/QWindow>
#include <QtQml/QQmlApplicationEngine>
#include <QtQuick/QQuickWindow>
#include <QtQuickControls2/QQuickStyle>

#include <cstdio>

#include "iconprovider.h"
#include "windowchrome.h"

#ifdef Q_OS_WIN
#include <windows.h>
#endif

// The brand faces, one static cut per family (assets/fonts, built by
// tools/build-fonts.py). Registered before QML so Theme's families resolve.
static void registerFonts()
{
  QDirIterator it(QStringLiteral(":/fonts"), { QStringLiteral("*.ttf") });
  int count = 0;
  while (it.hasNext()) {
    if (QFontDatabase::addApplicationFont(it.next()) < 0)
      std::fprintf(stderr, "[fonts] failed to load %s\n",
                   qPrintable(it.filePath()));
    else
      ++count;
  }
  if (count == 0)
    std::fprintf(stderr, "[fonts] no brand fonts in the binary\n");
}

static bool reducedMotion()
{
#ifdef Q_OS_WIN
  // The switch Windows' "Animation effects" setting flips (DESIGN.md:480).
  BOOL animate = TRUE;
  SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &animate, 0);
  return !animate;
#else
  return false;
#endif
}

int main(int argc, char* argv[])
{
  QElapsedTimer startup;
  startup.start();

  QGuiApplication::setApplicationName(QStringLiteral("Yap Native"));
  QGuiApplication::setOrganizationName(QStringLiteral("Yap"));
  QGuiApplication app(argc, argv);
  QQuickStyle::setStyle(QStringLiteral("Basic"));
  // Glyph-cache text instead of distance fields: crisper at 11-15 px and
  // closer to Chromium's DirectWrite output. YAP_TEXT=qt for the other.
  if (qEnvironmentVariable("YAP_TEXT") != QLatin1String("qt"))
    QQuickWindow::setTextRenderType(QQuickWindow::NativeTextRendering);
  registerFonts();

  auto* chrome = new WindowChrome(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "WindowChrome", chrome);
  app.installNativeEventFilter(chrome);

  QQmlApplicationEngine engine;
  engine.addImageProvider(QStringLiteral("icon"), new IconProvider);
  QObject::connect(
    &engine, &QQmlApplicationEngine::objectCreationFailed, &app,
    [] { QCoreApplication::exit(-1); }, Qt::QueuedConnection);

  // Theme before the first binding evaluates: YAP_THEME for now, a setting
  // later (assigning Theme.themeId at runtime re-themes everything).
  if (QObject* theme =
        engine.singletonInstance<QObject*>("com.yap.native", "Theme")) {
    const QString id = qEnvironmentVariable("YAP_THEME");
    if (!id.isEmpty())
      theme->setProperty("themeId", id);
    theme->setProperty("reducedMotion", reducedMotion());
  }

  engine.loadFromModule("com.yap.native", "Main");

  if (engine.rootObjects().isEmpty())
    return -1;

  if (auto* window =
        qobject_cast<QQuickWindow*>(engine.rootObjects().first())) {
    // Main.qml leaves the window hidden, so the first paint is already
    // captionless.
    chrome->attach(window);
    window->show();
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
