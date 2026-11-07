#pragma once

// image://shadow/<blur>/<corner>/<r>/<g>/<b>/<a> : a CSS box shadow
// (tokens.css shadow-2 and shadow-3) as a nine-slice for BorderImage: a
// rounded rectangle of `corner` radius in the colour (0-255 each), blurred
// as CSS blurs (a Gaussian, sigma = blur / 2), with the blur's reach around
// it (ceil(1.5 x blur), three sigma) and one stretchable pixel in the
// middle: the borders are reach + corner on every side.
//
// It replaces a MultiEffect blur over a layer: nothing renders off screen,
// per frame or per open, and the image is made once per style.

#include <QtQuick/QQuickImageProvider>

class ShadowProvider : public QQuickImageProvider
{
public:
  ShadowProvider();

  QImage requestImage(const QString& id, QSize* size,
                      const QSize& requestedSize) override;
};
