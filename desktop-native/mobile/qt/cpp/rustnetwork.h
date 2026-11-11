#pragma once

// Qt's network loads (QML Image, AnimatedPicture) over the Rust core's
// rustls client (rust/src/http.rs): Qt Network's own HTTPS needs OpenSSL
// on Android, which Qt does not ship. GETs over http(s) become RustReply;
// everything else (qrc:, file:, image providers) stays Qt's.

#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkCacheMetaData>
#include <QtNetwork/QNetworkReply>
#include <QtQml/QQmlNetworkAccessManagerFactory>

#include <memory>

class QNetworkDiskCache;
struct RustReplyBridge;

// A GET answered by the core's client, buffered in memory (pictures are
// small; the Rust side stops at 32 MiB). The disk cache is done here, not
// by Qt: QNetworkDiskCache only plugs into Qt's own HTTP backend.
class RustReply : public QNetworkReply
{
  Q_OBJECT

public:
  RustReply(const QNetworkRequest& request, QNetworkDiskCache* cache, QObject* parent);
  ~RustReply() override;

  void abort() override;
  void close() override;
  bool isSequential() const override { return true; }
  qint64 bytesAvailable() const override;

  // From the bridge, on this object's thread.
  void onHead(int status, const QByteArray& headers, qint64 length, bool redirected);
  void onData(const QByteArray& chunk);
  void onDone(int code, const QString& message);

protected:
  qint64 readData(char* data, qint64 maxSize) override;

private:
  void startNetwork(const QByteArray& extraHeaders);
  void serveCached();
  void store();
  void fail(NetworkError code, const QString& message);
  void finishOk();
  void detach();

  std::shared_ptr<RustReplyBridge> m_bridge;
  void* m_handle = nullptr;
  QNetworkDiskCache* m_cache = nullptr; // null: no cache reads or writes
  bool m_save = false;                  // may store the answer
  QNetworkCacheMetaData m_stale;        // the copy a 304 confirms
  QNetworkCacheMetaData::RawHeaderList m_headers; // the answer's
  QByteArray m_buffer;                            // the whole body
  qsizetype m_read = 0;
  qint64 m_received = 0;
  qint64 m_total = -1;
  int m_status = 0;
  bool m_done = false;
};

class RustNetworkAccessManager : public QNetworkAccessManager
{
public:
  // `cacheDir` empty: no disk cache.
  RustNetworkAccessManager(const QString& cacheDir, QObject* parent);

protected:
  QNetworkReply* createRequest(Operation op, const QNetworkRequest& request,
                               QIODevice* data) override;

private:
  QNetworkDiskCache* m_disk = nullptr;
};

// One manager per thread that loads (QML's picture reader runs its own).
class RustNetworkFactory : public QQmlNetworkAccessManagerFactory
{
public:
  explicit RustNetworkFactory(QString cacheDir) : m_cacheDir(std::move(cacheDir)) {}
  QNetworkAccessManager* create(QObject* parent) override;

private:
  QString m_cacheDir;
};
