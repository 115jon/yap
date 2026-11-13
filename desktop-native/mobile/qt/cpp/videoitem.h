#pragma once

// A remote camera or share on screen (seams/VideoSurface.qml wraps it):
// while visible, the newest frame of `videoKey` from the video hub
// (videohub.h; hidden, it lets go of the key), uploaded
// as one RGBA texture when the scene graph syncs, turned by the frame's
// rotation and scaled to fill the item (cropped) or to fit in it.

#include <QtQuick/QQuickItem>
#include <QtQml/qqmlregistration.h>

class VideoView : public QQuickItem
{
  Q_OBJECT
  // The key shown (`cam-video-<pid>`, `screen-video-<pid>`); empty: none.
  Q_PROPERTY(QString videoKey READ videoKey WRITE setVideoKey NOTIFY videoKeyChanged)
  // Fill the item and crop the overflow (cameras), or fit inside it
  // (shares).
  Q_PROPERTY(bool crop READ crop WRITE setCrop NOTIFY cropChanged)
  // A frame of the current key has arrived.
  Q_PROPERTY(bool hasFrame READ hasFrame NOTIFY hasFrameChanged)
  // The frame's size as shown (turned), 0 before the first.
  Q_PROPERTY(QSize frameSize READ frameSize NOTIFY frameSizeChanged)

public:
  explicit VideoView(QQuickItem* parent = nullptr);
  ~VideoView() override;

  QString videoKey() const { return m_key; }
  void setVideoKey(const QString& key);
  bool crop() const { return m_crop; }
  void setCrop(bool crop);
  bool hasFrame() const { return m_hasFrame; }
  QSize frameSize() const { return m_frameSize; }

  // GUI thread, from the hub: a new frame of `videoKey` is in.
  void frameArrived(QSize shown);

signals:
  void videoKeyChanged();
  void cropChanged();
  void hasFrameChanged();
  void frameSizeChanged();

protected:
  QSGNode* updatePaintNode(QSGNode* old, UpdatePaintNodeData*) override;
  void itemChange(ItemChange change, const ItemChangeData& data) override;

private:
  // Shows the key while visible, nothing while hidden.
  void syncAttached();

  QString m_key;
  QString m_attached;
  bool m_crop = true;
  bool m_hasFrame = false;
  QSize m_frameSize;
  // The serial of the frame on the texture (render thread).
  quint64 m_drawn = 0;
};
