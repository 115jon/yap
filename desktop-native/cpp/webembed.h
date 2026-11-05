#pragma once

// A web player in the chat (YouTube, Spotify, TikTok's player, rich
// players): a WebView2 controller in a child window of ours (src/webembed.rs)
// kept over this item.
//
//   WebEmbed { url: ...; active: playing; clipTo: chatList }
//
// Created only while `active`, and only one at a time: activating another
// closes this one (`evicted`). Every frame the window renders, the child
// window follows the item's scene rectangle, cut to the window, every
// clipping ancestor and `clipTo` (the chat viewport). It hides while the
// item is hidden, scrolled away, the window is minimised, or a popup (menu,
// dialog, viewer, tooltip) overlaps it: QML cannot draw over a native
// window. No WebView2 runtime: `failed` (state Failed for good), and QML
// opens the link on the next click. A crashed page just goes back to idle
// (active false). Navigations off the player hosts and new windows come out
// as `externalRequested`, for a user's click only. A new `url` (a pooled row
// reused) drops the player and `active`.

#include <QtCore/QPointer>
#include <QtCore/QString>
#include <QtQuick/QQuickItem>

#include <cstdint>

class WebEmbed : public QQuickItem
{
  Q_OBJECT
  Q_PROPERTY(QString url READ url WRITE setUrl NOTIFY urlChanged)
  Q_PROPERTY(bool active READ active WRITE setActive NOTIFY activeChanged)
  Q_PROPERTY(QQuickItem* clipTo READ clipTo WRITE setClipTo NOTIFY clipToChanged)
  // Set by QML for overlays that are not popups.
  Q_PROPERTY(bool covered READ covered WRITE setCovered NOTIFY coveredChanged)
  Q_PROPERTY(State state READ state NOTIFY stateChanged)

public:
  enum State { Idle, Loading, Ready, Failed };
  Q_ENUM(State)

  explicit WebEmbed(QQuickItem* parent = nullptr);
  ~WebEmbed() override;

  QString url() const { return m_url; }
  void setUrl(const QString& url);
  bool active() const { return m_active; }
  void setActive(bool active);
  QQuickItem* clipTo() const { return m_clipTo; }
  void setClipTo(QQuickItem* item);
  bool covered() const { return m_covered; }
  void setCovered(bool covered);
  State state() const { return m_state; }

  // src/webembed.rs reports here (GUI thread).
  static void onEvent(uint64_t id, int event, const QString& text);

signals:
  void urlChanged();
  void activeChanged();
  void clipToChanged();
  void coveredChanged();
  void stateChanged();
  void failed();
  void evicted();
  void externalRequested(const QString& url);

protected:
  void itemChange(ItemChange change, const ItemChangeData& value) override;
  void geometryChange(const QRectF& next, const QRectF& previous) override;

private:
  void sync();
  void open();
  void close();
  void fail();
  void stop();
  void place();
  bool popupOver(const QRectF& shown) const;
  void setState(State state);

  QString m_url;
  bool m_active = false;
  bool m_covered = false;
  State m_state = Idle;
  QPointer<QQuickItem> m_clipTo;
  QPointer<QQuickWindow> m_window;
  QMetaObject::Connection m_frames;
  uint64_t m_id = 0;
};
