#include "shareprovider.h"

#include <QtCore/QByteArray>
#include <QtCore/QStringList>

#include <cstdint>

extern "C" {
// Calls `sink` once with the picture's BGRA rows (stride bytes apart) and
// returns true, or returns false when there is none. Blocks.
using YapShareSink = void (*)(void* ctx, const uint8_t* bgra, uint32_t width,
                              uint32_t height, uint32_t stride);
bool yap_share_image(const char* id, size_t id_len, bool icon, uint32_t size,
                     YapShareSink sink, void* ctx);
}

namespace {

struct Target
{
  QImage image;
  QImage::Format format;
};

void copyInto(void* ctx, const uint8_t* bgra, uint32_t width, uint32_t height,
              uint32_t stride)
{
  auto* target = static_cast<Target*>(ctx);
  // QImage's ARGB32 / RGB32 are B, G, R, A in memory on little-endian.
  const QImage view(bgra, static_cast<int>(width), static_cast<int>(height),
                    static_cast<qsizetype>(stride), target->format);
  target->image = view.copy();
}

} // namespace

ShareProvider::ShareProvider()
  : QQuickImageProvider(QQuickImageProvider::Image,
                        QQuickImageProvider::ForceAsynchronousImageLoading)
{
}

QImage ShareProvider::requestImage(const QString& id, QSize* size,
                                   const QSize& requestedSize)
{
  const QStringList parts = id.split(QLatin1Char('/'));
  if (parts.size() < 2) {
    return {};
  }
  const bool icon = parts[0] == QLatin1String("icon");
  const QByteArray source = parts[1].toUtf8();
  const int wanted = icon ? requestedSize.height() : requestedSize.width();
  const uint32_t px = static_cast<uint32_t>(wanted > 0 ? wanted : (icon ? 16 : 480));
  // Thumbnails are opaque (window alpha is meaningless); icons keep theirs.
  Target target{ {}, icon ? QImage::Format_ARGB32 : QImage::Format_RGB32 };
  if (!yap_share_image(source.constData(), static_cast<size_t>(source.size()), icon, px,
                       &copyInto, &target)) {
    return {};
  }
  if (size) {
    *size = target.image.size();
  }
  return target.image;
}
