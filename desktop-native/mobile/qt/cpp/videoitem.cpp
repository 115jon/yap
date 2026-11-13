// VideoView (videoitem.h): the newest frame of a key on one texture.

#include "videoitem.h"

#include "videohub.h"

#include <QtGui/QMatrix4x4>
#include <QtQuick/QQuickWindow>
#include <QtQuick/QSGSimpleTextureNode>
#include <QtQuick/QSGTransformNode>
#include <QtQuick/private/qsgplaintexture_p.h>

#include <algorithm>

namespace {

// The item's node: a transform (the frame's turn about the item's
// centre) over the textured quad. The texture is the node's; one per view,
// re-filled with each new frame (the same size reuses its GPU texture).
class FrameNode : public QSGTransformNode
{
public:
  FrameNode()
  {
    // A member, not a heap child: the transform must not delete it.
    m_quad.setFlag(QSGNode::OwnedByParent, false);
    m_quad.setOwnsTexture(true);
    m_quad.setFiltering(QSGTexture::Linear);
    appendChildNode(&m_quad);
  }
  ~FrameNode() override { removeChildNode(&m_quad); }

  QSGSimpleTextureNode m_quad;
  QSGPlainTexture* m_texture = nullptr;
  int m_rotation = 0;
};

} // namespace

VideoView::VideoView(QQuickItem* parent)
  : QQuickItem(parent)
{
  setFlag(ItemHasContents, true);
}

VideoView::~VideoView()
{
  VideoHub::instance()->detach(m_attached, this);
}

void VideoView::syncAttached()
{
  const QString key = isVisible() ? m_key : QString();
  if (key == m_attached)
    return;
  VideoHub::instance()->detach(m_attached, this);
  m_attached = key;
  VideoHub::instance()->attach(m_attached, this);
}

void VideoView::itemChange(ItemChange change, const ItemChangeData& data)
{
  if (change == ItemVisibleHasChanged)
    syncAttached();
  QQuickItem::itemChange(change, data);
}

void VideoView::setVideoKey(const QString& key)
{
  if (key == m_key)
    return;
  m_key = key;
  syncAttached();
  m_drawn = 0;
  if (m_hasFrame) {
    m_hasFrame = false;
    emit hasFrameChanged();
  }
  if (!m_frameSize.isEmpty()) {
    m_frameSize = QSize();
    emit frameSizeChanged();
  }
  update();
  emit videoKeyChanged();
}

void VideoView::setCrop(bool crop)
{
  if (crop == m_crop)
    return;
  m_crop = crop;
  update();
  emit cropChanged();
}

void VideoView::frameArrived(QSize shown)
{
  if (!m_hasFrame) {
    m_hasFrame = true;
    emit hasFrameChanged();
  }
  if (shown != m_frameSize) {
    m_frameSize = shown;
    emit frameSizeChanged();
  }
  update();
}

QSGNode* VideoView::updatePaintNode(QSGNode* old, UpdatePaintNodeData*)
{
  auto* node = static_cast<FrameNode*>(old);
  VideoFeed* feed = m_attached.isEmpty() ? nullptr : VideoHub::instance()->wantedFeed(m_attached);
  const VideoFrame frame = feed ? feed->latest() : VideoFrame{};
  if (frame.serial == 0 || frame.image.isNull() || width() <= 0 || height() <= 0) {
    delete node;
    m_drawn = 0;
    return nullptr;
  }
  if (!node) {
    node = new FrameNode;
    node->m_texture = new QSGPlainTexture;
    node->m_texture->setHasAlphaChannel(false);
    node->m_quad.setTexture(node->m_texture);
    m_drawn = 0;
  }
  if (frame.serial != m_drawn) {
    // Uploaded when the material next commits; the plain texture drops its
    // copy of the image then, so the hub can lend the buffer again. Not
    // setTexture again: the node owns the texture, and setTexture deletes
    // the one it has before taking the new one, even when they are the same.
    node->m_texture->setImage(frame.image);
    node->m_texture->setHasAlphaChannel(false);
    node->m_quad.markDirty(QSGNode::DirtyMaterial);
    m_drawn = frame.serial;
  }

  // The frame as shown (turned) against the item: scaled to cover it
  // (crop) or to fit inside it.
  const bool turned = frame.rotation == 90 || frame.rotation == 270;
  const QSizeF image = frame.image.size();
  const QSizeF shown = turned ? image.transposed() : image;
  const qreal w = width();
  const qreal h = height();
  const qreal scale = m_crop ? std::max(w / shown.width(), h / shown.height())
                             : std::min(w / shown.width(), h / shown.height());
  // What of the shown frame is on screen, then the same in the image's own
  // (unturned) axes.
  const QSizeF visible(std::min(shown.width(), w / scale), std::min(shown.height(), h / scale));
  const QSizeF source = turned ? visible.transposed() : visible;
  node->m_quad.setSourceRect(QRectF(QPointF((image.width() - source.width()) / 2, (image.height() - source.height()) / 2), source));
  // The quad in the image's axes, centred on the item, then turned there.
  const QSizeF drawn = source * scale;
  node->m_quad.setRect(QRectF(QPointF((w - drawn.width()) / 2, (h - drawn.height()) / 2), drawn));
  QMatrix4x4 turn;
  if (frame.rotation != 0) {
    turn.translate(float(w / 2), float(h / 2));
    turn.rotate(float(frame.rotation), 0, 0, 1);
    turn.translate(float(-w / 2), float(-h / 2));
  }
  node->setMatrix(turn);
  return node;
}
