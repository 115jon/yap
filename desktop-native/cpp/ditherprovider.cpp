#include "ditherprovider.h"

#include <QtGui/QColor>

#include <algorithm>
#include <cmath>
#include <functional>

namespace {

using Cell = std::function<bool(int, int)>;

constexpr int kBayer4[4][4] = {
  { 0, 8, 2, 10 },
  { 12, 4, 14, 6 },
  { 3, 11, 1, 9 },
  { 15, 7, 13, 5 },
};

constexpr int kBayer8[8][8] = {
  { 0, 32, 8, 40, 2, 34, 10, 42 },  { 48, 16, 56, 24, 50, 18, 58, 26 },
  { 12, 44, 4, 36, 14, 46, 6, 38 }, { 60, 28, 52, 20, 62, 30, 54, 22 },
  { 3, 35, 11, 43, 1, 33, 9, 41 },  { 51, 19, 59, 27, 49, 17, 57, 25 },
  { 15, 47, 7, 39, 13, 45, 5, 37 }, { 63, 31, 55, 23, 61, 29, 53, 21 },
};

// ui/dither.tsx bayerLit
bool bayerLit(int x, int y, double coverage)
{
  return coverage * 64 > kBayer8[y % 8][x % 8] + 0.5;
}

Cell rect(int x0, int y0, int w, int h)
{
  return [=](int x, int y) {
    return x >= x0 && x < x0 + w && y >= y0 && y < y0 + h;
  };
}

QColor color(const QString& hex)
{
  return QColor::fromString(QLatin1Char('#') + hex);
}

QImage canvas(int w, int h)
{
  QImage image(w, h, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  return image;
}

void paint(QImage& image, const Cell& cell, const QColor& ink)
{
  const QRgb rgb = ink.rgba();
  for (int y = 0; y < image.height(); ++y)
    for (int x = 0; x < image.width(); ++x)
      if (cell(x, y))
        image.setPixel(x, y, qPremultiply(rgb));
}

// ui/yap-bark.tsx: the master mark on a 32-cell grid.
QImage bark(const QString& frame, const QColor& ink)
{
  constexpr int kCells = 32;
  const auto ear = [](int x, int y) { return x >= 2 && y < 10 && x < y + 2; };
  const auto innerEar = [](int x, int y) {
    return x >= 4 && y >= 5 && y < 9 && x < y - 1;
  };
  const Cell skull = rect(2, 10, 19, 14);
  const Cell snout = rect(21, 12, 11, 7);
  const Cell neck = rect(4, 24, 14, 8);
  const Cell eye = rect(13, 13, 4, 4);
  const Cell nose = rect(29, 12, 3, 3);
  const Cell jaw = frame == QLatin1String("closed") ? rect(17, 19, 13, 4)
                   : frame == QLatin1String("half") ? rect(17, 20, 12, 4)
                                                    : rect(17, 22, 11, 4);
  const bool ticks = frame == QLatin1String("open");
  const auto cutout = [&](int x, int y) {
    return innerEar(x, y) || eye(x, y) || nose(x, y);
  };
  const auto light = [](int x, int y) {
    return 0.98 - (x + y) / (2.0 * (kCells - 1)) * 0.3;
  };
  const auto lit = [&](int x, int y) {
    const bool body = ear(x, y) || skull(x, y) || snout(x, y) || neck(x, y);
    return (body || jaw(x, y)) && !cutout(x, y) && bayerLit(x, y, light(x, y));
  };
  constexpr int kTicks[6][2] = { { 29, 10 }, { 30, 9 },  { 31, 8 },
                                 { 29, 28 }, { 30, 29 }, { 31, 30 } };
  QImage image = canvas(kCells, kCells);
  paint(
    image,
    [&](int x, int y) {
      if (ticks)
        for (const auto& t : kTicks)
          if (t[0] == x && t[1] == y)
            return true;
      return lit(x, y);
    },
    ink);
  return image;
}

// ui/dither-glyph.tsx VOICE: seven two-cell bars with peak-hold cells.
QImage voice(const QColor& frameInk, const QColor& lightInk)
{
  constexpr int kBars[7] = { 5, 9, 13, 16, 11, 7, 4 };
  const auto bar = [&](int x) {
    const int i = (x - 1) / 4;
    return x >= 1 && (x - 1) % 4 < 2 && i < 7 ? i : -1;
  };
  QImage image = canvas(27, 20);
  paint(
    image,
    [&](int x, int y) {
      const int i = bar(x);
      const double top = 18 - (i >= 0 ? kBars[i] : 0);
      if (i < 0 || y > 17 || y < top)
        return false;
      return bayerLit(x, y, 0.98 - ((17 - y) / 17.0) * 0.5);
    },
    lightInk);
  paint(
    image,
    [&](int x, int y) {
      if (y == 19)
        return true;
      const int i = bar(x);
      return i >= 0 && y == 17 - kBars[i] - 1;
    },
    frameInk);
  return image;
}

// ui/dither.tsx bayerTileMask, repeated over w x h cells.
QImage dots(int coveragePercent, const QColor& ink, int w, int h)
{
  const int key = static_cast<int>(std::lround(coveragePercent / 100.0 * 16));
  QImage image = canvas(std::max(1, w), std::max(1, h));
  paint(
    image, [&](int x, int y) { return key > kBayer4[y % 4][x % 4]; }, ink);
  return image;
}

} // namespace

DitherProvider::DitherProvider()
  : QQuickImageProvider(QQuickImageProvider::Image)
{
}

QImage DitherProvider::requestImage(const QString& id, QSize* size,
                                    const QSize& /*requestedSize*/)
{
  const QStringList parts = id.split(QLatin1Char('/'));
  const QString kind = parts.value(0);
  QImage image;
  if (kind == QLatin1String("bark"))
    image = bark(parts.value(1), color(parts.value(2)));
  else if (kind == QLatin1String("voice"))
    image = voice(color(parts.value(1)), color(parts.value(2)));
  else if (kind == QLatin1String("dots"))
    image = dots(parts.value(1).toInt(), color(parts.value(2)),
                 parts.value(3).toInt(), parts.value(4).toInt());
  else
    image = canvas(1, 1);
  if (size)
    *size = image.size();
  return image;
}
