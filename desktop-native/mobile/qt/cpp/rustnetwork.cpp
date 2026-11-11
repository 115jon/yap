#include "rustnetwork.h"

#include <QtCore/QDateTime>
#include <QtCore/QLocale>
#include <QtCore/QMutex>
#include <QtCore/QTimeZone>
#include <QtNetwork/QNetworkDiskCache>

#include <algorithm>
#include <cstring>

// rust/src/http.rs
extern "C" {
struct YapHttpSink
{
  void* ctx;
  void (*head)(void* ctx, uint16_t status, const char* headers, size_t len, int64_t length,
               bool redirected);
  void (*data)(void* ctx, const uint8_t* bytes, size_t len);
  void (*done)(void* ctx, int32_t code, const char* message, size_t len);
  void (*release)(void* ctx);
};
void* yap_http_get(const char* url, size_t len, const char* headers, size_t hlen, YapHttpSink sink);
void yap_http_cancel(void* handle);
bool yap_http_private(const char* url, size_t len);
uint64_t yap_http_session_generation();
}

namespace {

// http.rs's DONE_* codes.
enum DoneCode { DoneOk = 0, DoneNetwork = 1, DoneTooLarge = 2, DoneBadRequest = 3, DoneTimeout = 4 };

constexpr qint64 kDiskCacheBytes = 256 * 1024 * 1024;
// A copy with no freshness headers and a Last-Modified stays fresh for a
// tenth of its age (RFC 9111 4.2.2), at most this long.
constexpr qint64 kMaxHeuristicSecs = 24 * 3600;

QDateTime httpDate(const QByteArray& value)
{
  // IMF-fixdate, the only form servers send in practice.
  QDateTime at = QLocale::c().toDateTime(QString::fromLatin1(value.trimmed()),
                                         QStringLiteral("ddd, dd MMM yyyy hh:mm:ss 'GMT'"));
  if (at.isValid())
    at.setTimeZone(QTimeZone::UTC);
  return at;
}

using RawHeaders = QNetworkCacheMetaData::RawHeaderList;

RawHeaders parseHeaders(const QByteArray& text)
{
  RawHeaders headers;
  for (const QByteArray& line : text.split('\n')) {
    const qsizetype colon = line.indexOf(':');
    if (colon <= 0)
      continue;
    headers.append({ line.left(colon).trimmed(), line.mid(colon + 1).trimmed() });
  }
  return headers;
}

QByteArray headerValue(const RawHeaders& headers, QByteArrayView name)
{
  for (const auto& [key, value] : headers) {
    if (key.compare(name, Qt::CaseInsensitive) == 0)
      return value;
  }
  return {};
}

// What the disk cache keeps for a 200, or an invalid entry when it must
// not keep it (no-store).
QNetworkCacheMetaData cacheEntry(const QUrl& url, const RawHeaders& headers)
{
  const QByteArray control = headerValue(headers, "cache-control").toLower();
  if (control.contains("no-store"))
    return {};
  const QDateTime now = QDateTime::currentDateTimeUtc();
  QDateTime expires;
  if (control.contains("no-cache")) {
    expires = now; // always revalidate
  } else if (const qsizetype age = control.indexOf("max-age="); age >= 0) {
    qsizetype end = age + 8;
    while (end < control.size() && control[end] >= '0' && control[end] <= '9')
      ++end;
    expires = now.addSecs(control.mid(age + 8, end - age - 8).toLongLong());
  } else if (const QByteArray until = headerValue(headers, "expires"); !until.isEmpty()) {
    // An Expires that does not parse (often "0") means already expired.
    const QDateTime at = httpDate(until);
    expires = at.isValid() ? at : now;
  } else if (const QDateTime modified = httpDate(headerValue(headers, "last-modified"));
             modified.isValid() && modified < now) {
    expires = now.addSecs(qMin(modified.secsTo(now) / 10, kMaxHeuristicSecs));
  } else {
    expires = now;
  }
  QNetworkCacheMetaData meta;
  meta.setUrl(url);
  meta.setRawHeaders(headers);
  meta.setExpirationDate(expires);
  meta.setLastModified(httpDate(headerValue(headers, "last-modified")));
  meta.setSaveToDisk(true);
  meta.setAttributes({ { QNetworkRequest::HttpStatusCodeAttribute, 200 } });
  return meta;
}

// The kept 200's headers updated by a 304's (RFC 9111 4.3.4): the
// validators and freshness it sends replace the stored ones.
RawHeaders merged(RawHeaders kept, const RawHeaders& update)
{
  static const QByteArrayView names[] = { "cache-control", "expires", "etag", "date",
                                          "last-modified", "age" };
  for (const auto& [key, value] : update) {
    const bool relevant = std::any_of(std::begin(names), std::end(names), [&key](QByteArrayView n) {
      return key.compare(n, Qt::CaseInsensitive) == 0;
    });
    if (!relevant)
      continue;
    kept.removeIf([&key](const auto& h) { return h.first.compare(key, Qt::CaseInsensitive) == 0; });
    kept.append({ key, value });
  }
  return kept;
}

bool fresh(const QNetworkCacheMetaData& meta)
{
  return meta.expirationDate().isValid() && meta.expirationDate() > QDateTime::currentDateTimeUtc();
}

QNetworkReply::NetworkError statusError(int status)
{
  switch (status) {
  case 401:
    return QNetworkReply::AuthenticationRequiredError;
  case 403:
    return QNetworkReply::ContentAccessDenied;
  case 404:
  case 410:
    return QNetworkReply::ContentNotFoundError;
  case 500:
    return QNetworkReply::InternalServerError;
  case 503:
    return QNetworkReply::ServiceUnavailableError;
  default:
    return status >= 500 ? QNetworkReply::UnknownServerError
                         : QNetworkReply::ProtocolInvalidOperationError;
  }
}

// Private media never goes to the disk cache, but an animated picture is
// fetched twice in a row: its still (QML's Image, which tells its size) and,
// once its row has stood for a moment, its player (AnimatedPicture). The
// last few private GIF and WebP files stay in memory for a minute, so the
// player (and the viewer, opened soon after) gets the bytes the still had.
// Only under the session that fetched them: a sign-out (or another account)
// never gets them back.
// Shared by every thread's manager (QML's picture reader has its own).
class RecentPrivate
{
public:
  static RecentPrivate& instance()
  {
    static RecentPrivate recent;
    return recent;
  }

  // Only files that animate: a still picture is not asked for twice.
  static bool wanted(const QByteArray& body)
  {
    return body.size() <= kMaxEntry
      && (body.startsWith("GIF87a") || body.startsWith("GIF89a")
          || (body.size() >= 12 && body.startsWith("RIFF") && body.mid(8, 4) == "WEBP"));
  }

  bool find(const QUrl& url, QByteArray* body, RawHeaders* headers)
  {
    QMutexLocker locked(&m_lock);
    expire();
    for (const Entry& entry : std::as_const(m_entries)) {
      if (entry.url == url && entry.session == yap_http_session_generation()) {
        *body = entry.body;
        *headers = entry.headers;
        return true;
      }
    }
    return false;
  }

  // `session`: yap_http_session_generation() when the request started.
  void put(const QUrl& url, const QByteArray& body, const RawHeaders& headers, quint64 session)
  {
    QMutexLocker locked(&m_lock);
    expire();
    m_entries.removeIf([&url](const Entry& e) { return e.url == url; });
    m_entries.append({ url, body, headers, QDateTime::currentMSecsSinceEpoch(), session });
    qsizetype bytes = 0;
    for (const Entry& entry : std::as_const(m_entries))
      bytes += entry.body.size();
    // Oldest first.
    while (bytes > kMaxBytes && !m_entries.isEmpty())
      bytes -= m_entries.takeFirst().body.size();
  }

private:
  static constexpr qsizetype kMaxEntry = 8 * 1024 * 1024;
  static constexpr qsizetype kMaxBytes = 16 * 1024 * 1024;
  static constexpr qint64 kKeepMs = 60 * 1000;

  struct Entry
  {
    QUrl url;
    QByteArray body;
    RawHeaders headers;
    qint64 at = 0;
    quint64 session = 0;
  };

  void expire()
  {
    const qint64 now = QDateTime::currentMSecsSinceEpoch();
    const quint64 session = yap_http_session_generation();
    m_entries.removeIf(
      [now, session](const Entry& e) { return now - e.at > kKeepMs || e.session != session; });
  }

  QMutex m_lock;
  QList<Entry> m_entries;
};

} // namespace

// Shared by the reply and the Rust task (http.rs holds one reference
// through the sink's ctx until `release`). The reply clears `reply` when it
// goes away or stops listening, under the lock, so a callback on a tokio
// thread either posts to a live reply or does nothing; a posted call to a
// reply deleted meanwhile is dropped by Qt with the reply.
struct RustReplyBridge
{
  QMutex lock;
  RustReply* reply = nullptr;

  template <typename F>
  static void post(void* ctx, F&& call)
  {
    auto& bridge = *static_cast<std::shared_ptr<RustReplyBridge>*>(ctx);
    QMutexLocker locked(&bridge->lock);
    if (RustReply* reply = bridge->reply)
      QMetaObject::invokeMethod(
        reply, [reply, call = std::forward<F>(call)] { call(reply); }, Qt::QueuedConnection);
  }

  static YapHttpSink sink(const std::shared_ptr<RustReplyBridge>& bridge)
  {
    YapHttpSink sink{};
    sink.ctx = new std::shared_ptr<RustReplyBridge>(bridge);
    sink.head = [](void* ctx, uint16_t status, const char* headers, size_t len, int64_t length,
                   bool redirected) {
      const QByteArray text(headers, qsizetype(len));
      post(ctx, [status, text, length, redirected](RustReply* r) {
        r->onHead(status, text, length, redirected);
      });
    };
    sink.data = [](void* ctx, const uint8_t* bytes, size_t len) {
      const QByteArray chunk(reinterpret_cast<const char*>(bytes), qsizetype(len));
      post(ctx, [chunk](RustReply* r) { r->onData(chunk); });
    };
    sink.done = [](void* ctx, int32_t code, const char* message, size_t len) {
      const QString text = QString::fromUtf8(message, qsizetype(len));
      post(ctx, [code, text](RustReply* r) { r->onDone(code, text); });
    };
    sink.release = [](void* ctx) { delete static_cast<std::shared_ptr<RustReplyBridge>*>(ctx); };
    return sink;
  }
};

RustReply::RustReply(const QNetworkRequest& request, QNetworkDiskCache* cache, QObject* parent)
  : QNetworkReply(parent), m_bridge(std::make_shared<RustReplyBridge>())
{
  m_bridge->reply = this;
  setRequest(request);
  setUrl(request.url());
  setOperation(QNetworkAccessManager::GetOperation);
  open(QIODevice::ReadOnly);

  const QByteArray url = request.url().toEncoded();
  const bool privateMedia = yap_http_private(url.constData(), size_t(url.size()));
  // Private media stays out of the disk cache, as on the desktop: never
  // stored, never read back (and out of the next account's reach).
  m_private = privateMedia;
  m_session = yap_http_session_generation();
  if (!privateMedia)
    m_cache = cache;
  const int loadControl = request.attribute(QNetworkRequest::CacheLoadControlAttribute,
                                            QNetworkRequest::PreferNetwork).toInt();
  if (privateMedia && loadControl != QNetworkRequest::AlwaysNetwork) {
    QByteArray body;
    RawHeaders headers;
    if (RecentPrivate::instance().find(request.url(), &body, &headers)) {
      qInfo("[net] an animated picture fetched moments ago: %lld bytes from memory",
            static_cast<long long>(body.size()));
      QMetaObject::invokeMethod(
        this, [this, body, headers] { serveRecent(body, headers); }, Qt::QueuedConnection);
      return;
    }
  }
  m_save = m_cache && request.attribute(QNetworkRequest::CacheSaveControlAttribute, true).toBool();
  const int load = request.attribute(QNetworkRequest::CacheLoadControlAttribute,
                                     QNetworkRequest::PreferNetwork).toInt();
  QByteArray conditional;
  if (m_cache && load != QNetworkRequest::AlwaysNetwork) {
    const QNetworkCacheMetaData meta = m_cache->metaData(request.url());
    if (meta.isValid()) {
      m_stale = meta;
      // Qt's rule (loadFromCacheIfAllowed): PreferCache and PreferNetwork
      // both serve only a fresh copy; AlwaysCache serves any.
      if (load == QNetworkRequest::AlwaysCache || fresh(meta)) {
        // Signals only once the caller has connected to them.
        QMetaObject::invokeMethod(this, [this] { serveCached(); }, Qt::QueuedConnection);
        return;
      }
      const RawHeaders headers = meta.rawHeaders();
      if (const QByteArray etag = headerValue(headers, "etag"); !etag.isEmpty())
        conditional += "If-None-Match: " + etag + '\n';
      if (const QByteArray modified = headerValue(headers, "last-modified"); !modified.isEmpty())
        conditional += "If-Modified-Since: " + modified + '\n';
    } else if (load == QNetworkRequest::AlwaysCache) {
      QMetaObject::invokeMethod(
        this, [this] { fail(ContentNotFoundError, QStringLiteral("Not in the cache")); },
        Qt::QueuedConnection);
      return;
    }
  }
  startNetwork(conditional);
}

RustReply::~RustReply()
{
  detach();
}

void RustReply::startNetwork(const QByteArray& extraHeaders)
{
  const QByteArray url = this->url().toEncoded();
  m_handle = yap_http_get(url.constData(), size_t(url.size()), extraHeaders.constData(),
                          size_t(extraHeaders.size()), RustReplyBridge::sink(m_bridge));
}

void RustReply::detach()
{
  {
    QMutexLocker locked(&m_bridge->lock);
    m_bridge->reply = nullptr;
  }
  if (m_handle) {
    yap_http_cancel(m_handle);
    m_handle = nullptr;
  }
}

qint64 RustReply::bytesAvailable() const
{
  return (m_buffer.size() - m_read) + QNetworkReply::bytesAvailable();
}

qint64 RustReply::readData(char* data, qint64 maxSize)
{
  const qint64 left = m_buffer.size() - m_read;
  if (left <= 0)
    return m_done ? -1 : 0;
  const qint64 n = qMin(left, maxSize);
  std::memcpy(data, m_buffer.constData() + m_read, size_t(n));
  // The whole body stays: onDone writes it to the disk cache.
  m_read += n;
  return n;
}

void RustReply::abort()
{
  if (m_done)
    return;
  detach();
  fail(OperationCanceledError, QStringLiteral("Operation canceled"));
}

void RustReply::close()
{
  // Closing a running download stops it, as Qt's own replies do.
  if (!m_done)
    abort();
  QNetworkReply::close();
}

void RustReply::onHead(int status, const QByteArray& headers, qint64 length, bool redirected)
{
  if (m_done)
    return; // aborted with this already queued
  m_status = status;
  m_headers = parseHeaders(headers);
  // A redirect's body belongs to another URL: not cached under this one.
  if (redirected) {
    m_save = false;
    m_redirected = true;
  }
  if (status == 304 && m_stale.isValid())
    return; // the cached copy answers, in onDone
  for (const auto& [key, value] : m_headers)
    setRawHeader(key, value);
  setAttribute(QNetworkRequest::HttpStatusCodeAttribute, status);
  m_total = length;
  emit metaDataChanged();
}

void RustReply::onData(const QByteArray& chunk)
{
  if (m_done)
    return;
  m_buffer.append(chunk);
  m_received += chunk.size();
  emit downloadProgress(m_received, m_total);
  emit readyRead();
}

void RustReply::onDone(int code, const QString& message)
{
  if (m_done)
    return;
  if (m_handle) {
    yap_http_cancel(m_handle); // finished: this only frees the handle
    m_handle = nullptr;
  }
  if (code != DoneOk) {
    // Offline (or the server unreachable): the copy on disk, however old,
    // even under `must-revalidate` (avatars send max-age=0): a phone
    // without signal shows the last picture rather than none.
    if (m_stale.isValid() && (code == DoneNetwork || code == DoneTimeout) && m_received == 0) {
      serveCached();
      return;
    }
    switch (code) {
    case DoneTimeout:
      fail(TimeoutError, message);
      break;
    case DoneTooLarge:
      fail(UnknownContentError, QStringLiteral("Response too large"));
      break;
    case DoneBadRequest:
      fail(ProtocolUnknownError, message);
      break;
    default:
      fail(UnknownNetworkError, message);
    }
    return;
  }
  if (m_status == 304 && m_stale.isValid()) {
    // Still current: fresh again for as long as the new answer says.
    serveCached();
    return;
  }
  if (m_status >= 400) {
    if (m_cache && (m_status == 404 || m_status == 410))
      m_cache->remove(url());
    fail(statusError(m_status), QStringLiteral("HTTP %1").arg(m_status));
    return;
  }
  if (m_status == 200 && m_save)
    store();
  if (m_status == 200 && m_private && !m_redirected && RecentPrivate::wanted(m_buffer))
    RecentPrivate::instance().put(url(), m_buffer, m_headers, m_session);
  finishOk();
}

// Written in one go once the body is complete: a cache insertion is keyed
// by URL, so one left open across event-loop turns could be cancelled or
// replaced by another reply for the same URL.
void RustReply::store()
{
  const QNetworkCacheMetaData meta = cacheEntry(url(), m_headers);
  if (!meta.isValid()) {
    m_cache->remove(url()); // no-store now: drop the old copy too
    return;
  }
  if (QIODevice* device = m_cache->prepare(meta)) {
    device->write(m_buffer);
    m_cache->insert(device);
  }
}

void RustReply::serveCached()
{
  if (m_done)
    return;
  QIODevice* body = m_cache ? m_cache->data(url()) : nullptr;
  if (!body) {
    // The entry went away (pruned) between the lookup and now.
    m_stale = {};
    if (!m_handle && m_status == 0) {
      startNetwork({});
      return;
    }
    fail(UnknownContentError, QStringLiteral("Cache entry vanished"));
    return;
  }
  m_buffer = body->readAll();
  delete body;
  m_read = 0;
  m_received = m_buffer.size();
  m_total = m_received;
  for (const auto& [key, value] : m_stale.rawHeaders())
    setRawHeader(key, value);
  setAttribute(QNetworkRequest::HttpStatusCodeAttribute, 200);
  setAttribute(QNetworkRequest::SourceIsFromCacheAttribute, true);
  if (m_status == 304 && m_save) {
    // Confirmed current: fresh again for as long as the 304 says.
    const QNetworkCacheMetaData meta = cacheEntry(url(), merged(m_stale.rawHeaders(), m_headers));
    if (meta.isValid())
      m_cache->updateMetaData(meta);
    else
      m_cache->remove(url());
  }
  emit metaDataChanged();
  emit downloadProgress(m_received, m_total);
  emit readyRead();
  finishOk();
}

void RustReply::serveRecent(const QByteArray& body, const RawHeaders& headers)
{
  if (m_done)
    return;
  m_buffer = body;
  m_read = 0;
  m_received = m_buffer.size();
  m_total = m_received;
  for (const auto& [key, value] : headers)
    setRawHeader(key, value);
  setAttribute(QNetworkRequest::HttpStatusCodeAttribute, 200);
  setAttribute(QNetworkRequest::SourceIsFromCacheAttribute, true);
  emit metaDataChanged();
  emit downloadProgress(m_received, m_total);
  emit readyRead();
  finishOk();
}

void RustReply::fail(NetworkError code, const QString& message)
{
  if (m_done)
    return;
  m_done = true;
  setError(code, message);
  setFinished(true);
  emit errorOccurred(code);
  emit finished();
}

void RustReply::finishOk()
{
  if (m_done)
    return;
  m_done = true;
  setFinished(true);
  emit finished();
}

RustNetworkAccessManager::RustNetworkAccessManager(const QString& cacheDir, QObject* parent)
  : QNetworkAccessManager(parent)
{
  if (cacheDir.isEmpty())
    return;
  m_disk = new QNetworkDiskCache(this);
  m_disk->setCacheDirectory(cacheDir);
  m_disk->setMaximumCacheSize(kDiskCacheBytes);
}

QNetworkReply* RustNetworkAccessManager::createRequest(Operation op, const QNetworkRequest& request,
                                                       QIODevice* data)
{
  const QString scheme = request.url().scheme();
  if (op == GetOperation && (scheme == QLatin1String("https") || scheme == QLatin1String("http")))
    return new RustReply(request, m_disk, this);
  return QNetworkAccessManager::createRequest(op, request, data);
}

QNetworkAccessManager* RustNetworkFactory::create(QObject* parent)
{
  return new RustNetworkAccessManager(m_cacheDir, parent);
}
