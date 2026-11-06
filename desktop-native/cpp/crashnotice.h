#pragma once

// Crash reports, the shell side. main.cpp installs the minidump handler
// (rust/src/crash.rs) into the profile's `crashes` folder; on the next
// launch this finds a dump newer than the last one shown and QML offers
// "Yap Native closed unexpectedly" with Open folder. Nothing is uploaded.
//
// QML (import Yap.Chrome):
//   CrashNotice.pending      a new dump since the notice was last dismissed
//   CrashNotice.open()       shows the folder in Explorer (YAP_LOG_EXTERNAL_OPEN=1:
//                            only logged), then dismisses
//   CrashNotice.dismiss()    hides the notice until the next crash

#include <QtCore/QObject>
#include <QtCore/QString>

class CrashNotice : public QObject
{
  Q_OBJECT
  Q_PROPERTY(bool pending READ pending NOTIFY pendingChanged)

public:
  explicit CrashNotice(QObject* parent = nullptr);

  // `%LOCALAPPDATA%\YapNative[\profiles\<name>]\crashes`, empty without
  // LOCALAPPDATA.
  static QString folder();
  // Installs the dump handler (call early in main, after the log starts).
  static void install();
  // YAP_TEST_CRASH=1: crashes a few seconds from now (tests only).
  static void scheduleTestCrash();

  bool pending() const { return m_pending; }
  Q_INVOKABLE void open();
  Q_INVOKABLE void dismiss();

signals:
  void pendingChanged();

private:
  bool m_pending = false;
};
