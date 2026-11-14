#include "yap_ui.h"

#include <QtCore/QUrl>
#include <QtQml/QQmlEngine>
#include <QtQml/qqml.h>

#include "animatedpicture.h"
#include "ditherprovider.h"
#include "iconprovider.h"

void yap_ui_register(QQmlEngine& engine)
{
  qmlRegisterType<AnimatedPicture>("Yap.Media", 1, 0, "AnimatedPicture");
  qmlRegisterType<ViewportWatch>("Yap.Media", 1, 0, "ViewportWatch");
  engine.addImageProvider(QStringLiteral("icon"), new IconProvider);
  engine.addImageProvider(QStringLiteral("dither"), new DitherProvider);
}

// A path Windows reads as a UNC name (`//host/x`, `\\host\x`, mixed).
static bool isUncLike(const QString& path)
{
  if (path.size() < 2)
    return false;
  const auto slash = [](QChar c) { return c == u'/' || c == u'\\'; };
  return slash(path.at(0)) && slash(path.at(1));
}

bool yap_ui_media_url_allowed(const QUrl& url)
{
  const QString scheme = url.scheme().toLower();
  if (scheme == QLatin1String("http") || scheme == QLatin1String("https"))
    return !url.host().isEmpty();
  if (scheme == QLatin1String("qrc") || scheme == QLatin1String("data") ||
      scheme == QLatin1String("image"))
    return true;
  if (scheme == QLatin1String("file")) {
    if (!url.host().isEmpty())
      return false;
    // `file:////host/x` and `file:///%5C%5Chost/x` have no host, but a path
    // that Windows opens as a UNC name.
    return !isUncLike(url.path()) && !isUncLike(url.toLocalFile());
  }
  return false;
}
