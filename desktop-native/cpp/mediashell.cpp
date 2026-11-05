#include "mediashell.h"

#include <QtCore/QDateTime>
#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QFileInfo>
#include <QtCore/QMimeData>
#include <QtCore/QRegularExpression>
#include <QtCore/QStandardPaths>
#include <QtCore/QUrl>
#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>
#include <QtGui/QImage>
#include <QtGui/QWindow>

#ifdef Q_OS_WIN
#include <windows.h>
#include <shellapi.h>
#include <shlobj.h>
#include <shobjidl.h>
#endif

namespace {

// %LOCALAPPDATA%\YapNative\cache\<sub>, created on demand.
QString cacheDir(const QString& sub)
{
  QString base = qEnvironmentVariable("LOCALAPPDATA");
  if (base.isEmpty())
    base = QDir::tempPath();
  const QString dir = base + QStringLiteral("/YapNative/cache/") + sub;
  QDir().mkpath(dir);
  return dir;
}

// A plain, safe file name: no folders, no characters Windows refuses, no
// device names, never empty.
QString safeName(const QString& name)
{
  QString out = QFileInfo(name).fileName();
  for (QChar& c : out) {
    if (c.unicode() < 32 || QStringLiteral("<>:\"/\\|?*").contains(c))
      c = QLatin1Char('_');
  }
  while (out.endsWith(QLatin1Char('.')) || out.endsWith(QLatin1Char(' ')))
    out.chop(1);
  out = out.trimmed().left(120);
  if (out.isEmpty() || out.startsWith(QLatin1Char('.')))
    out.prepend(QStringLiteral("attachment"));
  // Windows' device names, whatever the extension (COM1-9, LPT1-9 and
  // their superscript forms included).
  static const QRegularExpression reserved(
    QStringLiteral(R"(^(CON|PRN|AUX|NUL|COM[1-9\x{B9}\x{B2}\x{B3}]|LPT[1-9\x{B9}\x{B2}\x{B3}])$)"),
    QRegularExpression::CaseInsensitiveOption);
  if (reserved.match(QFileInfo(out).baseName()).hasMatch())
    out.prepend(QLatin1Char('_'));
  return out;
}

// Deletes files in %LOCALAPPDATA%\YapNative\cache\<sub> older than a day.
void pruneOld(const QString& sub)
{
  const QDateTime cutoff = QDateTime::currentDateTime().addDays(-1);
  const QDir dir(cacheDir(sub));
  for (const QFileInfo& file : dir.entryInfoList(QDir::Files)) {
    if (file.lastModified() < cutoff)
      QFile::remove(file.absoluteFilePath());
  }
}

// `dir/name`, or `dir/name (n).ext` when that is taken.
QString freePath(const QString& dir, const QString& name)
{
  const QFileInfo info(name);
  const QString stem = info.completeBaseName();
  const QString ext = info.suffix().isEmpty() ? QString() : QLatin1Char('.') + info.suffix();
  QString path = dir + QLatin1Char('/') + name;
  for (int n = 1; QFileInfo::exists(path) && n < 1000; ++n)
    path = dir + QLatin1Char('/') + stem + QStringLiteral(" (%1)").arg(n) + ext;
  return path;
}

bool isMediaFile(const QString& path)
{
  static const QStringList kinds = {
    QStringLiteral("png"), QStringLiteral("jpg"), QStringLiteral("jpeg"),
    QStringLiteral("gif"), QStringLiteral("webp"), QStringLiteral("bmp"),
    QStringLiteral("mp4"), QStringLiteral("webm"), QStringLiteral("m4v"),
    QStringLiteral("mov"), QStringLiteral("ogv")
  };
  return kinds.contains(QFileInfo(path).suffix().toLower());
}

#ifdef Q_OS_WIN
HWND ownerWindow()
{
  QWindow* window = QGuiApplication::focusWindow();
  if (!window && !QGuiApplication::topLevelWindows().isEmpty())
    window = QGuiApplication::topLevelWindows().first();
  return window ? reinterpret_cast<HWND>(window->winId()) : nullptr;
}
#endif

} // namespace

MediaShell::MediaShell(QObject* parent)
  : QObject(parent)
{
  // Pasted pictures and "Open original" copies are throwaways.
  pruneOld(QStringLiteral("paste"));
  pruneOld(QStringLiteral("open"));
}

void MediaShell::clearImageCache()
{
  emit imageCacheCleared();
}

QStringList MediaShell::pickFiles(const QString& title)
{
  QStringList paths;
#ifdef Q_OS_WIN
  IFileOpenDialog* dialog = nullptr;
  if (FAILED(CoCreateInstance(__uuidof(FileOpenDialog), nullptr, CLSCTX_INPROC_SERVER,
                              IID_PPV_ARGS(&dialog))))
    return paths;
  DWORD options = 0;
  dialog->GetOptions(&options);
  dialog->SetOptions(options | FOS_ALLOWMULTISELECT | FOS_FORCEFILESYSTEM | FOS_FILEMUSTEXIST);
  if (!title.isEmpty())
    dialog->SetTitle(reinterpret_cast<LPCWSTR>(title.utf16()));
  if (SUCCEEDED(dialog->Show(ownerWindow()))) {
    IShellItemArray* items = nullptr;
    if (SUCCEEDED(dialog->GetResults(&items))) {
      DWORD count = 0;
      items->GetCount(&count);
      for (DWORD i = 0; i < count; ++i) {
        IShellItem* item = nullptr;
        if (FAILED(items->GetItemAt(i, &item)))
          continue;
        PWSTR name = nullptr;
        if (SUCCEEDED(item->GetDisplayName(SIGDN_FILESYSPATH, &name))) {
          paths << QDir::fromNativeSeparators(QString::fromWCharArray(name));
          CoTaskMemFree(name);
        }
        item->Release();
      }
      items->Release();
    }
  }
  dialog->Release();
#else
  Q_UNUSED(title);
#endif
  return paths;
}

QStringList MediaShell::clipboardFiles()
{
  QStringList paths;
  const QMimeData* mime = QGuiApplication::clipboard()->mimeData();
  if (!mime)
    return paths;
  if (mime->hasUrls()) {
    for (const QUrl& url : mime->urls()) {
      if (url.isLocalFile() && QFileInfo(url.toLocalFile()).isFile())
        paths << url.toLocalFile();
    }
    if (!paths.isEmpty())
      return paths;
  }
  if (mime->hasImage()) {
    const QImage image = qvariant_cast<QImage>(mime->imageData());
    if (!image.isNull()) {
      const QString stamp =
        QDateTime::currentDateTime().toString(QStringLiteral("yyyyMMdd-HHmmss-zzz"));
      const QString path =
        cacheDir(QStringLiteral("paste")) + QStringLiteral("/image-%1.png").arg(stamp);
      if (image.save(path, "PNG"))
        paths << path;
    }
  }
  return paths;
}

QString MediaShell::downloadPath(const QString& name) const
{
  QString dir = QStandardPaths::writableLocation(QStandardPaths::DownloadLocation);
  if (dir.isEmpty())
    dir = cacheDir(QStringLiteral("downloads"));
  QDir().mkpath(dir);
  return freePath(dir, safeName(name));
}

QString MediaShell::openPath(const QString& name) const
{
  return freePath(cacheDir(QStringLiteral("open")), safeName(name));
}

void MediaShell::reveal(const QString& path) const
{
#ifdef Q_OS_WIN
  const QString native = QDir::toNativeSeparators(path);
  PIDLIST_ABSOLUTE item = ILCreateFromPathW(reinterpret_cast<LPCWSTR>(native.utf16()));
  if (item) {
    SHOpenFolderAndSelectItems(item, 0, nullptr, 0);
    ILFree(item);
  }
#else
  Q_UNUSED(path);
#endif
}

void MediaShell::openMedia(const QString& path) const
{
  if (!isMediaFile(path) || !QFileInfo(path).isFile()) {
    reveal(path);
    return;
  }
#ifdef Q_OS_WIN
  const QString native = QDir::toNativeSeparators(path);
  ShellExecuteW(ownerWindow(), L"open", reinterpret_cast<LPCWSTR>(native.utf16()), nullptr,
                nullptr, SW_SHOWNORMAL);
#endif
}
