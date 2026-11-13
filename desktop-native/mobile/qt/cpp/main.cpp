// The Qt Quick mobile prototype's shell: the view layer shared with the
// desktop (QML module Yap.Ui and its C++, from the yap-ui crate), the phone
// shell (com.yap.mobile, this app's crate), and the mobile platform shim
// (Native).

#include <QtCore/QDir>
#include <QtCore/QDirIterator>
#include <QtCore/QElapsedTimer>
#include <QtCore/QFile>
#include <QtCore/QResource>
#include <QtCore/QStandardPaths>
#include <QtCore/QSysInfo>
#ifdef Q_OS_ANDROID
#include <QtCore/QJniObject>
#include <QtCore/qcoreapplication_platform.h>
#endif
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

#include "native.h"
#include "rustnetwork.h"
#include "videohub.h"
#include "videoitem.h"
#include "yap_ui.h"

extern "C" void yap_mobile_init(const char* data_dir, const char* device_name);

// The brand fonts, read in place: the resource is stored uncompressed
// (CMakeLists.txt), so the font database and each FreeType face use the
// bytes mapped with the library. From a path Qt copies every file onto the
// heap, and once more for each face it opens (about 6 MB in all).
static void registerFonts()
{
  QDirIterator it(QStringLiteral(":/fonts"), { QStringLiteral("*.ttf") });
  int count = 0;
  while (it.hasNext()) {
    const QString path = it.next();
    const QResource font(path);
    const int id = font.isValid() && font.compressionAlgorithm() == QResource::NoCompression
      ? QFontDatabase::addApplicationFontFromData(
          QByteArray::fromRawData(reinterpret_cast<const char*>(font.data()), font.size()))
      : QFontDatabase::addApplicationFont(path);
    if (id >= 0)
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
  QString device = QSysInfo::prettyProductName();
#ifdef Q_OS_ANDROID
  device = QJniObject::getStaticObjectField<jstring>("android/os/Build", "MODEL").toString();
#endif
  yap_mobile_init(QFile::encodeName(data).constData(), device.toUtf8().constData());

  registerFonts();

  // The mobile side of the shared QML's seams (Yap.Ui imports
  // Yap.Platform): this app's own files, under the names the shared QML
  // uses. The desktop registers its own.
  for (const char* name :
       { "EmbedCard", "MessageMedia", "NameplateLayer", "VideoSurface", "VoiceStatusLine", "VoiceStreamHoverCard" })
    qmlRegisterType(QUrl(QStringLiteral("qrc:/qt/qml/com/yap/mobile/qml/seams/%1.qml").arg(QLatin1String(name))),
                    "Yap.Platform", 1, 0, name);
  auto* shell = new MobileShell(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "DesktopShell", shell);
  auto* native = new Native(&app);
  qmlRegisterSingletonInstance("Yap.Mobile", 1, 0, "Native", native);
  // Call video (seams/VideoSurface.qml draws with it). The hub is made here,
  // on the GUI thread, before a call's media thread can ask for it.
  VideoHub::instance();
  qmlRegisterType<VideoView>("Yap.Mobile", 1, 0, "VideoView");

  QQmlApplicationEngine engine;
  // Network pictures over the core's rustls client, with a disk cache in
  // the app's cache folder (Qt's own HTTPS would need OpenSSL).
  engine.setNetworkAccessManagerFactory(new RustNetworkFactory(
    QStandardPaths::writableLocation(QStandardPaths::CacheLocation) + QStringLiteral("/images")));
  // AnimatedPicture and ViewportWatch (Yap.Media; GIF, APNG and WebP
  // through yap-anim), the icon and dither providers: the desktop's C++.
  yap_ui_register(engine);
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
#ifdef Q_OS_ANDROID
                                        // Qt's splash (sticky in the manifest)
                                        // fades out over the first frame.
                                        QNativeInterface::QAndroidApplication::hideSplashScreen(200);
#endif
                                      });
                   });
  engine.loadFromModule("com.yap.mobile", "Main");
  return app.exec();
}
