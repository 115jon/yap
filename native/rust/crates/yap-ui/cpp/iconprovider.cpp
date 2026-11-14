#include "iconprovider.h"

#include <QtCore/QFile>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QMutexLocker>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtSvg/QSvgRenderer>

#include <cstdio>

namespace {
// Icon.tsx: Phosphor's own slash, corner to corner of the live area.
constexpr QPointF kSlashFrom{ 48, 40 };
constexpr QPointF kSlashTo{ 208, 216 };
// Stroke widths in grid units: Bold is 24/256 of the box, Fill 16.
constexpr qreal kSlashBold = 24;
constexpr qreal kSlashFill = 16;
// Clear band cut through the glyph on each side of the slash.
constexpr qreal kSlashGap = 12;
constexpr int kDefaultPx = 18;
}

IconProvider::IconProvider()
  : QQuickImageProvider(QQuickImageProvider::Image)
  , m_cache(512)
{
  QFile file(QStringLiteral(":/icons/manifest.json"));
  if (!file.open(QIODevice::ReadOnly)) {
    std::fprintf(stderr, "[icons] manifest missing\n");
    return;
  }
  const QJsonObject root = QJsonDocument::fromJson(file.readAll()).object();
  const QJsonObject concepts = root.value(QStringLiteral("concepts")).toObject();
  for (auto it = concepts.begin(); it != concepts.end(); ++it) {
    const QJsonObject spec = it.value().toObject();
    m_concepts.insert(it.key(),
                      { spec.value(QStringLiteral("glyph")).toString(),
                        spec.value(QStringLiteral("fill")).toBool(),
                        spec.value(QStringLiteral("slash")).toBool(),
                        spec.value(QStringLiteral("rotate")).toInt() });
  }
  const QJsonObject off = root.value(QStringLiteral("offPair")).toObject();
  for (auto it = off.begin(); it != off.end(); ++it)
    m_offPair.insert(it.key(), it.value().toString());
}

QImage IconProvider::requestImage(const QString& id, QSize* size,
                                  const QSize& requestedSize)
{
  const QStringList parts = id.split(QLatin1Char('/'));
  const QString name = parts.value(0);
  const QString state = parts.value(1, QStringLiteral("idle"));
  const QColor color = QColor::fromString(
    QLatin1Char('#') + parts.value(2, QStringLiteral("000000")));
  const int px = qMax(1, requestedSize.isValid()
                           ? qMax(requestedSize.width(), requestedSize.height())
                           : kDefaultPx);

  const QString key = id + QLatin1Char('@') + QString::number(px);
  QMutexLocker lock(&m_mutex);
  QImage image;
  if (const QImage* hit = m_cache.object(key)) {
    image = *hit;
  } else {
    image = render(name, state, color, px);
    m_cache.insert(key, new QImage(image), 1);
  }
  if (size)
    *size = image.size();
  return image;
}

QImage IconProvider::render(const QString& name, const QString& state,
                            const QColor& color, int px) const
{
  QImage image(px, px, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);

  const QString resolved =
    state == QLatin1String("off") ? m_offPair.value(name, name) : name;
  const auto it = m_concepts.constFind(resolved);
  if (it == m_concepts.constEnd()) {
    std::fprintf(stderr, "[icons] unknown concept \"%s\"\n",
                 qPrintable(resolved));
    return image;
  }
  const Concept& spec = *it;
  // Only concepts that ship a Fill weight take a non-idle state.
  const bool solid = state != QLatin1String("idle") && spec.fill;

  QSvgRenderer svg(QStringLiteral(":/icons/%1-%2.svg")
                     .arg(spec.glyph, solid ? QStringLiteral("fill")
                                            : QStringLiteral("bold")));
  QPainter painter(&image);
  painter.setRenderHint(QPainter::Antialiasing);
  painter.scale(px / 256.0, px / 256.0);
  if (spec.rotate) {
    painter.save();
    painter.translate(128, 128);
    painter.rotate(spec.rotate);
    painter.translate(-128, -128);
    svg.render(&painter, QRectF(0, 0, 256, 256));
    painter.restore();
  } else {
    svg.render(&painter, QRectF(0, 0, 256, 256));
  }

  if (spec.slash) {
    const qreal width = solid ? kSlashFill : kSlashBold;
    QPainterPath slash(kSlashFrom);
    slash.lineTo(kSlashTo);
    painter.setCompositionMode(QPainter::CompositionMode_Clear);
    painter.strokePath(slash, QPen(Qt::black, width + kSlashGap * 2,
                                   Qt::SolidLine, Qt::RoundCap));
    painter.setCompositionMode(QPainter::CompositionMode_SourceOver);
    painter.strokePath(slash,
                       QPen(Qt::black, width, Qt::SolidLine, Qt::RoundCap));
  }

  // currentColor: keep the coverage, take the colour.
  painter.resetTransform();
  painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
  painter.fillRect(image.rect(), color);
  return image;
}
