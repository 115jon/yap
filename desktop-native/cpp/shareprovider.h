#pragma once

// image://share/... : the screen share picker's pictures, from the Rust
// capture crate (yap-capture through src/share.rs).
//
//   thumb/<source id>/<nonce>   a still of a monitor or window, at most the
//                               requested width (480 by default)
//   icon/<source id>            a window's app icon, at the requested size
//
// Loading is asynchronous (a Windows Graphics Capture snapshot can take a
// few hundred milliseconds); a source with no picture gives a null image,
// and the picker draws its glyph instead. The nonce only busts the cache.

#include <QtQuick/QQuickImageProvider>

class ShareProvider : public QQuickImageProvider
{
public:
  ShareProvider();

  QImage requestImage(const QString& id, QSize* size,
                      const QSize& requestedSize) override;
};
