#include "yap_ui.h"

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
