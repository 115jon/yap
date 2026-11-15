#include "a11y.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QList>
#include <QtCore/QPointer>
#include <QtCore/QTimer>
#include <QtGui/QAccessible>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <QtQuick/private/qaccessiblequickitem_p.h>
#include <QtQuick/private/qaccessiblequicktextedit_p.h>
#include <QtQuick/private/qaccessiblequicktextinput_p.h>
#include <QtQuick/private/qaccessiblequickview_p.h>
#include <QtQuick/private/qquickitem_p.h>
#include <QtQuick/private/qquicktextedit_p.h>
#include <QtQuick/private/qquicktextinput_p.h>

#include <utility>

namespace {

QList<QPointer<QQuickItem>>& modals()
{
  static QList<QPointer<QQuickItem>> list;
  return list;
}

QList<QPointer<QQuickItem>>& hiddenItems()
{
  static QList<QPointer<QQuickItem>> list;
  return list;
}

// The modal item on top: the last one set that is on screen.
QQuickItem* topModal()
{
  auto& list = modals();
  list.removeAll(QPointer<QQuickItem>());
  for (qsizetype i = list.size() - 1; i >= 0; --i)
    if (list.at(i)->isVisible())
      return list.at(i);
  return nullptr;
}

bool active()
{
  hiddenItems().removeAll(QPointer<QQuickItem>());
  return !hiddenItems().isEmpty() || topModal();
}

// Out of the tree: outside the modal item and not on its way down from the
// window, or in a hidden item.
bool concealed(const QQuickItem* item)
{
  if (QQuickItem* modal = topModal()) {
    if (item == modal || item->isAncestorOf(modal))
      return false;
    if (!modal->isAncestorOf(item))
      return true;
  }
  for (const auto& hidden : hiddenItems())
    if (hidden && (hidden == item || hidden->isAncestorOf(item)))
      return true;
  return false;
}

bool dropped(const QAccessibleInterface* iface)
{
  const auto* item = qobject_cast<const QQuickItem*>(iface->object());
  return item && concealed(item);
}

// Qt Quick's interfaces, with the concealed items left out.
template <class Base>
class Concealing : public Base {
public:
  using Base::Base;

  QAccessible::State state() const override
  {
    QAccessible::State state = Base::state();
    if (active() && dropped(this))
      state.invisible = true;
    return state;
  }
  int childCount() const override { return active() ? int(kids().size()) : Base::childCount(); }
  QAccessibleInterface* child(int index) const override
  {
    if (!active())
      return Base::child(index);
    const auto list = kids();
    return index >= 0 && index < list.size() ? list.at(index) : nullptr;
  }
  int indexOfChild(const QAccessibleInterface* child) const override
  {
    if (!active())
      return Base::indexOfChild(child);
    return int(kids().indexOf(const_cast<QAccessibleInterface*>(child)));
  }
  QAccessibleInterface* childAt(int x, int y) const override
  {
    if (!active())
      return Base::childAt(x, y);
    if (dropped(this))
      return nullptr;
    QAccessibleInterface* hit = Base::childAt(x, y);
    return hit && dropped(hit) ? nullptr : hit;
  }

private:
  QList<QAccessibleInterface*> kids() const
  {
    QList<QAccessibleInterface*> list;
    if (dropped(this))
      return list;
    const int count = Base::childCount();
    for (int i = 0; i < count; ++i)
      if (QAccessibleInterface* child = Base::child(i); child && !dropped(child))
        list.append(child);
    return list;
  }
};

// A Qt Quick Controls control. Qt Quick Templates' own interfaces for
// these (QAccessibleQuickControl and its subclasses) are not exported, so
// this is the item's, with the one thing QAccessibleQuickControl adds: the
// control's locale (TalkBack's language for its text). The app uses no
// Page, ScrollBar or Popup, whose interfaces add more.
class ConcealingControl final : public Concealing<QAccessibleQuickItem> {
public:
  using Concealing::Concealing;

  QList<QAccessible::Attribute> attributeKeys() const override
  {
    auto keys = Concealing::attributeKeys();
    if (!keys.contains(QAccessible::Attribute::Locale))
      keys << QAccessible::Attribute::Locale;
    return keys;
  }
  QVariant attributeValue(QAccessible::Attribute key) const override
  {
    if (key == QAccessible::Attribute::Locale)
      return object()->property("locale");
    return Concealing::attributeValue(key);
  }
};

// The classes Qt Quick's factory (qQuickAccessibleFactory) and Qt Quick
// Templates' answer for. Installed after them, so asked first for each
// class name, from the most derived up.
QAccessibleInterface* factory(const QString& className, QObject* object)
{
  if (className == u"QQuickWindow")
    return new Concealing<QAccessibleQuickWindow>(qobject_cast<QQuickWindow*>(object));
  if (className == u"QQuickTextEdit")
    return new Concealing<QAccessibleQuickTextEdit>(qobject_cast<QQuickTextEdit*>(object));
  if (className == u"QQuickTextInput")
    return new Concealing<QAccessibleQuickTextInput>(qobject_cast<QQuickTextInput*>(object));
  if (className == u"QQuickPage" || className == u"QQuickScrollBar" || className == u"QQuickControl"
      || className == u"QQuickPopupItem")
    return new ConcealingControl(qobject_cast<QQuickItem*>(object));
  if (className == u"QQuickItem") {
    auto* item = qobject_cast<QQuickItem*>(object);
    if (!item || !QQuickItemPrivate::get(item)->isAccessible)
      return nullptr;
    return new Concealing<QAccessibleQuickItem>(item);
  }
  return nullptr;
}

// The first accessible item in `item`, in the order a swipe reads them.
QQuickItem* firstAccessible(QQuickItem* item)
{
  const auto children = item->childItems();
  for (QQuickItem* child : children) {
    if (!child->isVisible())
      continue;
    if (QQuickItemPrivate::get(child)->isAccessible)
      return child;
    if (QQuickItem* inner = firstAccessible(child))
      return inner;
  }
  return nullptr;
}

QPointer<QQuickWindow> g_window;
QPointer<QQuickItem> g_focus;
bool g_pending = false;

// Once per event loop turn after a change: TalkBack reads the whole tree
// again (an ObjectShow on the window invalidates the view's subtree), and
// an item that just went modal takes the accessibility focus.
void refresh(QQuickItem* item, bool opened)
{
  if (item && item->window())
    g_window = item->window();
  if (opened)
    g_focus = item;
  if (std::exchange(g_pending, true))
    return;
  QTimer::singleShot(0, qApp, [] {
    g_pending = false;
    const QPointer<QQuickItem> focus = std::exchange(g_focus, nullptr);
    if (!QAccessible::isActive() || !g_window)
      return;
    QAccessibleEvent shown(g_window.data(), QAccessible::ObjectShow);
    QAccessible::updateAccessibility(&shown);
    if (focus && focus == topModal()) {
      if (QQuickItem* first = firstAccessible(focus)) {
        QAccessibleEvent focused(first, QAccessible::Focus);
        QAccessible::updateAccessibility(&focused);
      }
    }
  });
}

} // namespace

A11yAttached::A11yAttached(QObject* parent)
  : QObject(parent)
  , m_item(qobject_cast<QQuickItem*>(parent))
{
  if (m_item)
    connect(m_item, &QQuickItem::visibleChanged, this, [this] {
      if (m_modal || m_hidden)
        refresh(m_item, m_modal && m_item->isVisible());
    });
}

A11yAttached::~A11yAttached()
{
  if (m_modal || m_hidden)
    refresh(nullptr, false);
}

void A11yAttached::setModal(bool on)
{
  if (m_modal == on)
    return;
  m_modal = on;
  if (m_item) {
    modals().removeAll(QPointer<QQuickItem>(m_item));
    if (on)
      modals().append(QPointer<QQuickItem>(m_item));
    refresh(m_item, on && m_item->isVisible());
  }
  emit modalChanged();
}

void A11yAttached::setHidden(bool on)
{
  if (m_hidden == on)
    return;
  m_hidden = on;
  if (m_item) {
    hiddenItems().removeAll(QPointer<QQuickItem>(m_item));
    if (on)
      hiddenItems().append(QPointer<QQuickItem>(m_item));
    refresh(m_item, false);
  }
  emit hiddenChanged();
}

void installA11y()
{
  qmlRegisterUncreatableType<A11y>("Yap.Mobile", 1, 0, "A11y", QStringLiteral("A11y is an attached property"));
  // Qt Quick installs its factory as the library loads; the last one
  // installed is asked first.
  QAccessible::installFactory(&factory);
}
