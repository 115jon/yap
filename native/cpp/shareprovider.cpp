#include "shareprovider.h"

#include <QtCore/QByteArray>
#include <QtCore/QRunnable>
#include <QtCore/QStringList>
#include <QtGui/QImage>

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

// One picture: loaded on the pool, then `finished` (the engine deletes the
// response once it has the texture, after `run` has returned).
class ShareResponse : public QQuickImageResponse, public QRunnable
{
public:
  ShareResponse(const QString& id, const QSize& requestedSize)
    : m_id(id)
    , m_requested(requestedSize)
  {
    setAutoDelete(false);
  }

  QQuickTextureFactory* textureFactory() const override
  {
    return QQuickTextureFactory::textureFactoryForImage(m_image);
  }

  QString errorString() const override { return m_error; }

  void run() override
  {
    const QStringList parts = m_id.split(QLatin1Char('/'));
    if (parts.size() < 2) {
      m_error = QStringLiteral("bad id");
    } else {
      const bool icon = parts[0] == QLatin1String("icon");
      const QByteArray source = parts[1].toUtf8();
      const int wanted = icon ? m_requested.height() : m_requested.width();
      const uint32_t px = static_cast<uint32_t>(wanted > 0 ? wanted : (icon ? 16 : 480));
      // Thumbnails are opaque (window alpha is meaningless); icons keep theirs.
      Target target{ {}, icon ? QImage::Format_ARGB32 : QImage::Format_RGB32 };
      if (yap_share_image(source.constData(), static_cast<size_t>(source.size()), icon, px,
                          &copyInto, &target)) {
        m_image = target.image;
      } else {
        m_error = QStringLiteral("no picture for %1").arg(m_id);
      }
    }
    emit finished();
  }

private:
  QString m_id;
  QSize m_requested;
  QImage m_image;
  QString m_error;
};

} // namespace

ShareProvider::ShareProvider()
{
  m_pool.setMaxThreadCount(1);
}

// Every queued response still runs and emits `finished` (the engine
// deletes a response only after that, cancelled or not).
ShareProvider::~ShareProvider()
{
  m_pool.waitForDone();
}

QQuickImageResponse* ShareProvider::requestImageResponse(const QString& id,
                                                         const QSize& requestedSize)
{
  auto* response = new ShareResponse(id, requestedSize);
  m_pool.start(response);
  return response;
}
