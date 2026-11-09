#pragma once

// Files and media for the chat, the OS side (MessageInput.tsx's file
// input and paste, the attachment downloads and the image viewer's
// "open"):
//
//   Files.pickFiles()          the system's multi-select open dialog
//   Files.testPickColor()      tests: YAP_PICK_COLOR, which the colour
//                              picker (ColorPopover.qml) applies instead of
//                              opening: `#RRGGBB`, "-" (a cancel), or empty
//                              when unset
//   Files.clipboardFiles()     files copied in Explorer, or a copied picture
//                              saved as a PNG (empty when neither); the
//                              core deletes the PNG once it is uploaded
//   Files.downloadPath(name)   a free path in the Downloads folder
//   Files.openPath(name)       a path in YapNative's cache for opening
//   Files.reveal(path)         shows the file in Explorer, selected
//   Files.openMedia(path)      opens a picture or video with its app;
//                              anything else is only revealed
//   Files.clearImageCache()    empties the downloaded-images cache (sign-out)
//   Files.localFiles(urls)     each local file among dragged or dropped URLs
//                              as {name, size} (QFileInfo, no read): the
//                              drop overlay's too-big state before the drop
//   Files.testDropUrls()       tests: YAP_TEST_DROP ("a.png;b.png") as file
//                              URLs, for a drop onto the chat without a
//                              real drag (empty when unset)
//   Files.copyPath(name)       a path in YapNative's cache for Copy image
//   Files.copyImage(path, dry) puts a downloaded picture on the clipboard as
//                              an image (a GIF's or WebP's first frame), the
//                              viewer's Copy image; `dry` (YAP_NO_CLIPBOARD)
//                              only logs it. False when it does not decode.
//   Files.openExternal(url)    an http(s) link in the system browser; with
//                              YAP_LOG_EXTERNAL_OPEN=1 (tests) it is only
//                              logged as "[external] open <host><path>"
//
// Names from messages are untrusted: they are reduced to a plain file
// name, and nothing but pictures and videos is ever opened.

#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QVariantList>

class MediaShell : public QObject
{
  Q_OBJECT

public:
  explicit MediaShell(QObject* parent = nullptr);

  Q_INVOKABLE QStringList pickFiles(const QString& title);
  Q_INVOKABLE QString testPickColor() const;
  Q_INVOKABLE QStringList clipboardFiles();
  Q_INVOKABLE QString downloadPath(const QString& name) const;
  Q_INVOKABLE QString openPath(const QString& name) const;
  Q_INVOKABLE QString copyPath(const QString& name) const;
  Q_INVOKABLE void reveal(const QString& path) const;
  Q_INVOKABLE void openMedia(const QString& path) const;
  Q_INVOKABLE void clearImageCache();
  Q_INVOKABLE void openExternal(const QString& url) const;
  Q_INVOKABLE bool copyImage(const QString& path, bool dry) const;
  Q_INVOKABLE QStringList testDropUrls() const;
  Q_INVOKABLE QVariantList localFiles(const QVariantList& urls) const;

signals:
  void imageCacheCleared();
};
