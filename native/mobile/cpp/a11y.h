#pragma once

// What a screen reader can reach while a sheet is up (README "Accessibility
// under sheets").
//
// Qt Quick leaves an item out of the accessibility tree only when it is
// invisible (QAccessibleQuickItem::state), and Qt's Android bridge
// (androidjniaccessibility.cpp, childIdListForAccessibleObject) lists every
// child an interface has. So the page under a sheet, which stays drawn
// under the scrim, stays in TalkBack's tree: a swipe walks from the sheet's
// last row into the messages behind it. `Accessible.ignored` does not help:
// it drops the item but hands its children to its parent.
//
// The fix lives here, not in a Qt patch: the Android app builds against
// the stock Qt packages from download.qt.io (fetch-qt.ps1), and
// patches/qt/ feeds only the desktop's own Qt build. QAccessible's public
// factory hook is enough: the app's interfaces, installed over Qt Quick's
// and Qt Quick Templates' (installA11y), are theirs with the children
// filtered, and take the modality from two attached properties:
//
//   A11y.modal: true    while this item is up (a sheet, a panel): items that
//                       are neither inside it nor its ancestors drop out
//   A11y.hidden: true   this item and everything in it drop out (the drawer
//                       under the chat on a phone)
//
// A dropped item reports `invisible` (TalkBack skips a node that is not
// visible to the user) and its parent leaves it out of its children and of
// hit tests. With neither in use the interfaces are Qt Quick's own.
// Opening a modal item moves TalkBack's focus into it.

#include <QtCore/QObject>
#include <QtQml/qqml.h>

class QQuickItem;

class A11yAttached : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool modal READ modal WRITE setModal NOTIFY modalChanged)
  Q_PROPERTY(bool hidden READ hidden WRITE setHidden NOTIFY hiddenChanged)

public:
  explicit A11yAttached(QObject* parent);
  ~A11yAttached() override;

  bool modal() const { return m_modal; }
  void setModal(bool on);
  bool hidden() const { return m_hidden; }
  void setHidden(bool on);

signals:
  void modalChanged();
  void hiddenChanged();

private:
  QQuickItem* m_item;
  bool m_modal = false;
  bool m_hidden = false;
};

class A11y : public QObject {
  Q_OBJECT
  QML_ATTACHED(A11yAttached)

public:
  static A11yAttached* qmlAttachedProperties(QObject* object) { return new A11yAttached(object); }
};

// Registers A11y in Yap.Mobile and the interfaces; before any window exists.
void installA11y();
