#pragma once

// image://dither/... : the web's still cell art (ui/dither.tsx,
// ui/dither-glyph.tsx, ui/yap-bark.tsx) as one pixel per cell. Draw it with
// `Image { smooth: false }` at cells x cell size, so every cell stays square.
//
//   bark/<rest|closed|half|open>/<rgb>      Yappy, 32 x 32 (accent dither)
//   pose/<stuffed|sad|squashed>/<accent>/<warn>/<tear>/<paper>/<fold>/<lines>
//                                           Yappy's too-big poses (the spec's
//                                           A, B, C): stuffed 40 x 34, the
//                                           others 34 x 34
//   badge/<id>/<lit rgb>/<detail rgb>      a badge (lib/badge-art.ts), 16 x
//                                           16: body through the 8x8 Bayer
//                                           light, edge solid, both in `lit`
//                                           (accent ink), details in `detail`
//                                           (ink-1); cached per id and inks
//   voice/<frame rgb>/<light rgb>           the level meter, 27 x 20
//   dots/<coverage 0-100>/<rgb>/<w>/<h>     a 4x4 Bayer tile over w x h
//   ramp/<from>/<to>/<lift>/<rgb>/<w>/<h>   an 8x8 Bayer ramp (percentages):
//                                           DitherRule, DitherRamp

#include <QtCore/QHash>
#include <QtCore/QMutex>
#include <QtQuick/QQuickImageProvider>

class DitherProvider : public QQuickImageProvider
{
public:
  DitherProvider();

  QImage requestImage(const QString& id, QSize* size,
                      const QSize& requestedSize) override;

private:
  // Badges by their full id: eight drawings times the theme's inks, a few
  // hundred bytes each. Requests can come from the loader thread.
  QMutex badgesLock_;
  QHash<QString, QImage> badges_;
};
