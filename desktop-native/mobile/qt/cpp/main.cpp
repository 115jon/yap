// The Qt Quick mobile prototype's shell: the desktop's QML module
// (com.yap.native, built by the Rust crate with the shared desktop files in
// it), the desktop's portable C++ (AnimatedPicture, the icon provider), and
// the mobile platform shim (Native).

#include <QtCore/QDir>
#include <QtCore/QDirIterator>
#include <QtCore/QElapsedTimer>
#include <QtCore/QFile>
#include <QtCore/QStandardPaths>
#include <QtCore/QSysInfo>
#include <QtGui/QFontDatabase>
#include <QtGui/QGuiApplication>
#include <QtQml/QQmlApplicationEngine>
#include <QtQml/qqml.h>
#include <QtQuick/QQuickWindow>

#include <algorithm>
#include <memory>
#include <utility>
#include <cstdio>
#include <vector>

#include "animatedpicture.h"
#include "ditherprovider.h"
#include "iconprovider.h"
#include "native.h"

extern "C" void yap_mobile_init(const char* data_dir, const char* device_name);

static void registerFonts()
{
  QDirIterator it(QStringLiteral(":/fonts"), { QStringLiteral("*.ttf") });
  int count = 0;
  while (it.hasNext()) {
    if (QFontDatabase::addApplicationFont(it.next()) >= 0)
      ++count;
  }
  if (count == 0)
    std::fprintf(stderr, "[fonts] no brand fonts in the binary\n");
}

int main(int argc, char* argv[])
{
  QElapsedTimer startup;
  startup.start();
  QGuiApplication app(argc, argv);
  app.setOrganizationName(QStringLiteral("Yap"));
  app.setApplicationName(QStringLiteral("Yap Qt prototype"));

  // The core's data folder (snapshot cache, settings, the session file) and
  // the device's name for the sign-in, before the core's threads exist.
  const QString data = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
  QDir().mkpath(data);
  yap_mobile_init(QFile::encodeName(data).constData(),
                  QSysInfo::prettyProductName().toUtf8().constData());

  registerFonts();

  // The desktop's own C++ for pictures (GIF, APNG and WebP through
  // yap-anim), under the URI the shared QML imports.
  qmlRegisterType<AnimatedPicture>("Yap.Media", 1, 0, "AnimatedPicture");
  qmlRegisterType<ViewportWatch>("Yap.Media", 1, 0, "ViewportWatch");
  auto* shell = new MobileShell(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "DesktopShell", shell);
  auto* native = new Native(&app);
  qmlRegisterSingletonInstance("Yap.Mobile", 1, 0, "Native", native);

  QQmlApplicationEngine engine;
  engine.addImageProvider(QStringLiteral("icon"), new IconProvider);
  engine.addImageProvider(QStringLiteral("dither"), new DitherProvider);
  QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                   [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
  // The first frame on screen, for the cold-start comparison (README), and
  // frame pacing while anything animates: every two seconds with frames in
  // them, the count, the median and 95th percentile interval, and how many
  // went past one and two vsyncs (the Compose baseline's gfxinfo numbers;
  // Qt renders on its own thread, which gfxinfo does not see).
  QObject::connect(&engine, &QQmlApplicationEngine::objectCreated, &app,
                   [&startup](QObject* object, const QUrl&) {
                     auto* window = qobject_cast<QQuickWindow*>(object);
                     if (!window)
                       return;
                     auto* pacing = new QElapsedTimer;
                     auto* intervals = new std::vector<qint64>;
                     auto* report = new QElapsedTimer;
                     report->start();
                     // On the render thread, where the swaps happen (Direct).
                     QObject::connect(window, &QQuickWindow::frameSwapped, window,
                                      [pacing, intervals, report] {
                                        if (pacing->isValid()) {
                                          const qint64 ns = pacing->nsecsElapsed();
                                          if (ns < 250'000'000)
                                            intervals->push_back(ns);
                                        }
                                        pacing->start();
                                        if (report->elapsed() < 2000)
                                          return;
                                        report->start();
                                        if (intervals->size() < 10) {
                                          intervals->clear();
                                          return;
                                        }
                                        std::sort(intervals->begin(), intervals->end());
                                        const auto at = [intervals](double q) {
                                          return (*intervals)[size_t(q * (intervals->size() - 1))] / 1e6;
                                        };
                                        const auto over = [intervals](double ms) {
                                          return std::count_if(intervals->begin(), intervals->end(),
                                                               [ms](qint64 v) { return v > ms * 1e6; });
                                        };
                                        qInfo("[frames] %zu p50 %.1f ms p95 %.1f ms max %.1f ms >17ms %lld >34ms %lld",
                                              intervals->size(), at(0.5), at(0.95), at(1.0),
                                              static_cast<long long>(over(17.5)),
                                              static_cast<long long>(over(34)));
                                        intervals->clear();
                                      }, Qt::DirectConnection);
                     // Queued to this thread from the render thread: report once.
                     auto first = std::make_shared<bool>(true);
                     QObject::connect(window, &QQuickWindow::frameSwapped, window,
                                      [&startup, first] {
                                        if (!std::exchange(*first, false))
                                          return;
                                        qInfo("[startup] first frame %lld ms",
                                              static_cast<long long>(startup.elapsed()));
                                      });
                   });
  engine.loadFromModule("com.yap.native", "Main");
  return app.exec();
}
