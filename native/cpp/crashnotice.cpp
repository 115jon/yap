#include "crashnotice.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QDateTime>
#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QFileInfo>
#include <QtCore/QTimer>
#include <QtCore/QUrl>
#include <QtGui/QDesktopServices>

#include <cstdio>
#include <string>

#include "profile.h"

extern "C" bool yap_crash_install(const char16_t* dir, size_t len);
extern "C" void yap_test_crash();

namespace {

// Touched when the notice is dismissed: dumps older than it were shown.
constexpr char kSeenMarker[] = ".notice-seen";

// The newest real dump. A run that did not crash leaves its pre-opened dump
// file empty (rust/src/crash.rs), so an empty one is no crash: it is skipped
// and deleted, except this run's own (named with its pid), still open.
QDateTime newestDump(const QString& dir)
{
  const QString own =
    QStringLiteral("-%1.dmp").arg(QCoreApplication::applicationPid());
  const QFileInfoList dumps =
    QDir(dir).entryInfoList({ QStringLiteral("*.dmp") }, QDir::Files, QDir::Time);
  for (const QFileInfo& dump : dumps) {
    if (dump.size() > 0)
      return dump.lastModified();
    if (!dump.fileName().endsWith(own))
      QFile::remove(dump.absoluteFilePath());
  }
  return {};
}

} // namespace

CrashNotice::CrashNotice(QObject* parent)
  : QObject(parent)
{
  const QString dir = folder();
  if (dir.isEmpty())
    return;
  const QDateTime newest = newestDump(dir);
  const QFileInfo seen(dir + QLatin1Char('/') + QLatin1String(kSeenMarker));
  m_pending = newest.isValid() && (!seen.exists() || newest > seen.lastModified());
  if (m_pending)
    std::fprintf(stderr, "[crash] the last run crashed (dump %s)\n",
                 qPrintable(newest.toString(Qt::ISODate)));
}

QString CrashNotice::folder()
{
  const std::wstring root = profile::dataRoot();
  if (root.empty())
    return {};
  return QString::fromStdWString(root) + QStringLiteral("\\crashes");
}

void CrashNotice::install()
{
  const QString dir = folder();
  if (dir.isEmpty())
    return;
  yap_crash_install(reinterpret_cast<const char16_t*>(dir.utf16()),
                    size_t(dir.size()));
}

void CrashNotice::scheduleTestCrash()
{
  if (qEnvironmentVariableIntValue("YAP_TEST_CRASH") != 1)
    return;
  std::fprintf(stderr, "[crash] YAP_TEST_CRASH=1: crashing in 4 s\n");
  std::fflush(stderr);
  QTimer::singleShot(4000, [] { yap_test_crash(); });
}

void CrashNotice::open()
{
  const QString dir = folder();
  if (qEnvironmentVariableIntValue("YAP_LOG_EXTERNAL_OPEN") != 0) {
    std::fprintf(stderr, "[external] open crash folder\n");
    std::fflush(stderr);
  } else if (!dir.isEmpty()) {
    QDesktopServices::openUrl(QUrl::fromLocalFile(dir));
  }
  dismiss();
}

void CrashNotice::dismiss()
{
  const QString dir = folder();
  if (!dir.isEmpty()) {
    QFile marker(dir + QLatin1Char('/') + QLatin1String(kSeenMarker));
    if (marker.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
      marker.write(QDateTime::currentDateTimeUtc().toString(Qt::ISODate).toUtf8());
      marker.close();
    }
  }
  if (m_pending) {
    m_pending = false;
    emit pendingChanged();
  }
}
