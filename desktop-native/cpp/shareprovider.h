#pragma once

// image://share/... : the screen share picker's pictures, from the Rust
// capture crate (yap-capture through src/share.rs).
//
//   thumb/<source id>/<nonce>   a still of a monitor or window, at most the
//                               requested width (480 by default)
//   icon/<source id>            a window's app icon, at the requested size
//
// Asynchronous, on the provider's own one-thread pool: a Windows Graphics
// Capture still takes up to a few hundred milliseconds, so the stills are
// taken one after another, in the order the picker asks, and never on
// Qt's shared image reader (avatars, icons and attachments keep loading).
// A source with no picture gives an error, and the picker draws its glyph.
// The nonce only busts the cache.

#include <QtCore/QThreadPool>
#include <QtQuick/QQuickAsyncImageProvider>

class ShareProvider : public QQuickAsyncImageProvider
{
public:
  ShareProvider();
  ~ShareProvider() override;

  QQuickImageResponse* requestImageResponse(const QString& id,
                                            const QSize& requestedSize) override;

private:
  QThreadPool m_pool;
};
