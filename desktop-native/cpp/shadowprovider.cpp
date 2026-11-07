#include "shadowprovider.h"

#include <QtGui/QPainter>
#include <QtGui/QPainterPath>

#include <algorithm>
#include <cmath>
#include <vector>

namespace {

// Box sizes whose three passes approximate a Gaussian of `sigma`
// (W. Jarosz, "Fast Image Convolutions").
std::vector<int> boxesFor(double sigma)
{
  const int n = 3;
  const double ideal = std::sqrt(12.0 * sigma * sigma / n + 1);
  int lower = int(std::floor(ideal));
  if (lower % 2 == 0)
    --lower;
  const int upper = lower + 2;
  const double m = (12.0 * sigma * sigma - n * lower * lower - 4.0 * n * lower - 3.0 * n) /
                   (-4.0 * lower - 4.0);
  const int small = int(std::round(m));
  std::vector<int> sizes;
  for (int i = 0; i < n; ++i)
    sizes.push_back(i < small ? lower : upper);
  return sizes;
}

// One box pass of radius `r` along rows (or columns) of a w x h plane.
void boxPass(const std::vector<float>& in, std::vector<float>& out, int w, int h, int r,
             bool rows)
{
  const int lines = rows ? h : w;
  const int length = rows ? w : h;
  const float scale = 1.0f / float(2 * r + 1);
  for (int line = 0; line < lines; ++line) {
    auto at = [&](int i) {
      i = std::clamp(i, 0, length - 1);
      return rows ? in[size_t(line) * w + i] : in[size_t(i) * w + line];
    };
    float sum = 0;
    for (int i = -r; i <= r; ++i)
      sum += at(i);
    for (int i = 0; i < length; ++i) {
      (rows ? out[size_t(line) * w + i] : out[size_t(i) * w + line]) = sum * scale;
      sum += at(i + r + 1) - at(i - r);
    }
  }
}

} // namespace

ShadowProvider::ShadowProvider()
  : QQuickImageProvider(QQuickImageProvider::Image)
{
}

QImage ShadowProvider::requestImage(const QString& id, QSize* size, const QSize&)
{
  const QStringList parts = id.split(QLatin1Char('/'));
  if (parts.size() != 6)
    return {};
  const int blur = std::clamp(parts[0].toInt(), 0, 256);
  const int corner = std::clamp(parts[1].toInt(), 0, 256);
  const QColor color(parts[2].toInt(), parts[3].toInt(), parts[4].toInt(),
                     std::clamp(parts[5].toInt(), 0, 255));
  // The Gaussian's reach: three sigma, where it has all but faded.
  const int reach = int(std::ceil(blur * 1.5));
  const int side = 2 * (reach + corner) + 1;

  // The shape's coverage, then three box passes each way.
  QImage shape(side, side, QImage::Format_Alpha8);
  shape.fill(0);
  {
    QPainter painter(&shape);
    painter.setRenderHint(QPainter::Antialiasing);
    QPainterPath path;
    path.addRoundedRect(QRectF(reach, reach, side - 2 * reach, side - 2 * reach), corner, corner);
    painter.fillPath(path, Qt::black);
  }
  std::vector<float> plane(size_t(side) * side);
  for (int y = 0; y < side; ++y) {
    const uchar* line = shape.constScanLine(y);
    for (int x = 0; x < side; ++x)
      plane[size_t(y) * side + x] = line[x] / 255.0f;
  }
  if (blur > 0) {
    std::vector<float> scratch(plane.size());
    for (int box : boxesFor(blur / 2.0)) {
      const int r = (box - 1) / 2;
      boxPass(plane, scratch, side, side, r, true);
      boxPass(scratch, plane, side, side, r, false);
    }
  }

  QImage image(side, side, QImage::Format_ARGB32_Premultiplied);
  const float alpha = color.alphaF();
  for (int y = 0; y < side; ++y) {
    auto* line = reinterpret_cast<QRgb*>(image.scanLine(y));
    for (int x = 0; x < side; ++x) {
      const float a = std::clamp(plane[size_t(y) * side + x], 0.0f, 1.0f) * alpha;
      line[x] = qRgba(int(color.red() * a + 0.5f), int(color.green() * a + 0.5f),
                      int(color.blue() * a + 0.5f), int(a * 255 + 0.5f));
    }
  }
  if (size)
    *size = image.size();
  return image;
}
