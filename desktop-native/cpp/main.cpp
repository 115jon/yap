// Entry point: a Qt Quick app whose logic lives in the Rust crate
// (YapClient, registered by the com.yap.native QML module).

#include <QtCore/QDirIterator>
#include <QtCore/QElapsedTimer>
#include <QtGui/QFontDatabase>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QWindow>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkDiskCache>
#include <QtQml/QQmlApplicationEngine>
#include <QtQml/QQmlNetworkAccessManagerFactory>
#include <QtQuick/QQuickWindow>
#include <QtQuickControls2/QQuickStyle>

#include <cstdio>

#include "desktopshell.h"
#include "ditherprovider.h"
#include "iconprovider.h"
#include "logfile.h"
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

// Every network image QML loads (avatars, icons, banners) goes through a
// disk cache next to the snapshot cache, apart from the installed app's
// folders. Avatars are public (`/api/avatars/*`, no token); their ETag
// revalidates a cached copy.
class CachedNetworkFactory : public QQmlNetworkAccessManagerFactory
{
public:
  QNetworkAccessManager* create(QObject* parent) override
  {
    auto* manager = new QNetworkAccessManager(parent);
    const QString base = qEnvironmentVariable("LOCALAPPDATA");
    if (!base.isEmpty()) {
      auto* cache = new QNetworkDiskCache(manager);
      cache->setCacheDirectory(base + QStringLiteral("/YapNative/cache/images"));
      cache->setMaximumCacheSize(128 * 1024 * 1024);
      manager->setCache(cache);
    }
    return manager;
  }
};

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

// Qt's messages go to stderr (and so into the log file) whatever the
// subsystem; Windows would otherwise send them to the debugger only.
static void qtMessage(QtMsgType type, const QMessageLogContext&,
                      const QString& message)
{
  static const char* const kinds[] = { "debug", "warning", "critical",
                                       "fatal", "info" };
  const int kind = int(type);
  std::fprintf(stderr, "[qt] %s: %s\n",
               kind >= 0 && kind < 5 ? kinds[kind] : "message",
               qPrintable(message));
}

int main(int argc, char* argv[])
{
  QElapsedTimer startup;
  startup.start();
  DesktopShell::setProcessIdentity();
  // Only the first instance owns the log file; a later one only hands over.
  const bool first = DesktopShell::takeInstanceLock();
  if (first)
    logfile::start();
  qInstallMessageHandler(qtMessage);

  // No organization: Qt's own caches (QML, pipeline) then go to
  // %LOCALAPPDATA%\YapNative\cache. With an organization "Yap" they went to
  // %LOCALAPPDATA%\Yap\..., the installed app's folder.
  QGuiApplication::setApplicationName(QStringLiteral("YapNative"));
  QGuiApplication app(argc, argv);
  // A second launch shows the first instance's window and exits.
  if (!first) {
    DesktopShell::showFirstInstance();
    return 0;
  }
  QGuiApplication::setWindowIcon(QIcon(QStringLiteral(":/app/yap-256.png")));
  QQuickStyle::setStyle(QStringLiteral("Basic"));
  // Glyph-cache text instead of distance fields: crisper at 11-15 px and
  // closer to Chromium's DirectWrite output. YAP_TEXT=qt for the other.
  if (qEnvironmentVariable("YAP_TEXT") != QLatin1String("qt"))
    QQuickWindow::setTextRenderType(QQuickWindow::NativeTextRendering);
  registerFonts();

  auto* chrome = new WindowChrome(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "WindowChrome", chrome);
  app.installNativeEventFilter(chrome);
  auto* shell = new DesktopShell(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "DesktopShell", shell);

  QQmlApplicationEngine engine;
  // Not owned by the engine; it outlives it here.
  static CachedNetworkFactory networkFactory;
  engine.setNetworkAccessManagerFactory(&networkFactory);
  engine.addImageProvider(QStringLiteral("icon"), new IconProvider);
  engine.addImageProvider(QStringLiteral("dither"), new DitherProvider);
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
    // captionless. The shell restores the saved rect and shows it (or
    // keeps it in the tray on an autostart launch with start minimized).
    chrome->attach(window);
    shell->attach(window, engine.singletonInstance<QObject*>(
                            "com.yap.native", "Desktop"));
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
