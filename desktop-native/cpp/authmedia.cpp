#include "authmedia.h"

#include <QtCore/QByteArray>

// src/media.rs
extern "C" size_t yap_media_auth(const char* url, size_t len, char* out, size_t cap);
extern "C" size_t yap_media_local_url(const char* url, size_t len, char* out, size_t cap);
extern "C" void yap_media_release_local_url(const char* url, size_t len);

bool AuthMedia::needsAuth(const QString& url) const
{
  const QByteArray bytes = url.toUtf8();
  return yap_media_auth(bytes.constData(), size_t(bytes.size()), nullptr, 0) > 0;
}

QString AuthMedia::localUrl(const QString& url) const
{
  const QByteArray bytes = url.toUtf8();
  // A name that does not fit is told by its length: ask again with room.
  QByteArray out(512, Qt::Uninitialized);
  const auto ask = [&] {
    return yap_media_local_url(bytes.constData(), size_t(bytes.size()), out.data(),
                               size_t(out.size()));
  };
  size_t n = ask();
  if (n > size_t(out.size())) {
    out.resize(qsizetype(n));
    n = ask();
  }
  if (n == 0 || n > size_t(out.size()))
    return url;
  return QString::fromUtf8(out.constData(), qsizetype(n));
}

void AuthMedia::release(const QString& url) const
{
  const QByteArray bytes = url.toUtf8();
  yap_media_release_local_url(bytes.constData(), size_t(bytes.size()));
}
