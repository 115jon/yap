#pragma once

// image://icon/<concept>/<idle|active|off>/<rrggbb|aarrggbb>
//
// Draws a Phosphor glyph from the generated assets (assets/icons) the way
// src/components/ui/icon/Icon.tsx does: Bold when idle, Fill when active,
// the concept's off partner in Fill when off, rotation and the slash (with
// its clear band) composited in QPainter, then tinted to the colour. The
// requested size (Image.sourceSize x device pixel ratio) is the raster size.

#include <QtCore/QCache>
#include <QtCore/QHash>
#include <QtCore/QMutex>
#include <QtGui/QImage>
#include <QtQuick/QQuickImageProvider>

class IconProvider : public QQuickImageProvider
{
public:
  IconProvider();

  QImage requestImage(const QString& id, QSize* size,
                      const QSize& requestedSize) override;

private:
  struct Concept
  {
    QString glyph;
    bool fill = false;
    bool slash = false;
    int rotate = 0;
  };

  QImage render(const QString& name, const QString& state, const QColor& color,
                int px) const;

  QHash<QString, Concept> m_concepts;
  QHash<QString, QString> m_offPair;
  QMutex m_mutex;
  QCache<QString, QImage> m_cache;
};
