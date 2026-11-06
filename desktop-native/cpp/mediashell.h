#pragma once

// Files and media for the chat, the OS side (MessageInput.tsx's file
// input and paste, the attachment downloads and the image viewer's
// "open"):
//
//   Files.pickFiles()          the system's multi-select open dialog
//   Files.clipboardFiles()     files copied in Explorer, or a copied picture
//                              saved as a PNG (empty when neither); the
//                              core deletes the PNG once it is uploaded
//   Files.downloadPath(name)   a free path in the Downloads folder
//   Files.openPath(name)       a path in YapNative's cache for opening
//   Files.reveal(path)         shows the file in Explorer, selected
//   Files.openMedia(path)      opens a picture or video with its app;
//                              anything else is only revealed
//   Files.clearImageCache()    empties the downloaded-images cache (sign-out)
//   Files.openExternal(url)    an http(s) link in the system browser; with
//                              YAP_LOG_EXTERNAL_OPEN=1 (tests) it is only
//                              logged as "[external] open <host><path>"
//
// Names from messages are untrusted: they are reduced to a plain file
// name, and nothing but pictures and videos is ever opened.

#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtCore/QStringList>

class MediaShell : public QObject
{
  Q_OBJECT

public:
  explicit MediaShell(QObject* parent = nullptr);

  Q_INVOKABLE QStringList pickFiles(const QString& title);
  Q_INVOKABLE QStringList clipboardFiles();
  Q_INVOKABLE QString downloadPath(const QString& name) const;
  Q_INVOKABLE QString openPath(const QString& name) const;
  Q_INVOKABLE void reveal(const QString& path) const;
  Q_INVOKABLE void openMedia(const QString& path) const;
  Q_INVOKABLE void clearImageCache();
  Q_INVOKABLE void openExternal(const QString& url) const;

signals:
  void imageCacheCleared();
};
