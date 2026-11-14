// VideoPlayer and VideoPosterProvider (videoplayer.h).

#include "videoplayer.h"

#include <QtCore/QPointer>
#include <QtCore/QRunnable>
#include <QtCore/QThreadPool>
#include <QtCore/QUrl>
#include <QtGui/QGuiApplication>
#include <QtGui/QImage>

#include <atomic>
#include <chrono>
#include <future>
#include <iterator>
#include <memory>
#include <utility>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniEnvironment>
#include <QtCore/QJniObject>
#include <QtCore/qcoreapplication_platform.h>
#endif

extern "C" {
// rust/src/mediacap.rs: `url` as a player loads it, to `done` once from a
// runtime thread (len 0: it cannot be).
void yap_media_playable(const char* url, size_t len, void (*done)(void* ctx, const char* url, size_t len),
                        void* ctx);
}

namespace {

#ifdef Q_OS_ANDROID
constexpr char kShim[] = "dev/yap/qtproto/media/MediaShim";
#endif

VideoPlayer* g_instance = nullptr;

// One yap_media_playable call's answer, handed to `done` on whichever
// thread Rust answers from.
struct Pending
{
  std::function<void(const QString&)> done;
};

void playableDone(void* ctx, const char* url, size_t len)
{
  std::unique_ptr<Pending> pending(static_cast<Pending*>(ctx));
  pending->done(len > 0 ? QString::fromUtf8(url, qsizetype(len)) : QString());
}

// Resolves `url` and waits for the answer (a pool thread's poster grab).
QString resolveBlocking(const QString& url)
{
  auto answer = std::make_shared<std::promise<QString>>();
  std::future<QString> result = answer->get_future();
  const QByteArray utf8 = url.toUtf8();
  yap_media_playable(utf8.constData(), size_t(utf8.size()), playableDone,
                     new Pending{ [answer](const QString& playable) { answer->set_value(playable); } });
  // The grant request has its own timeouts; this only bounds a stuck one.
  if (result.wait_for(std::chrono::seconds(30)) != std::future_status::ready)
    return {};
  return result.get();
}

bool isWeb(const QString& url)
{
  return url.startsWith(QLatin1String("https://")) || url.startsWith(QLatin1String("http://"));
}

#ifdef Q_OS_ANDROID
void jniAudioState(JNIEnv*, jclass, jint state)
{
  QMetaObject::invokeMethod(qApp, [state] {
    if (VideoPlayer* player = VideoPlayer::instance())
      player->onAudioState(int(state));
  }, Qt::QueuedConnection);
}

QJniObject androidContext()
{
  return QNativeInterface::QAndroidApplication::context();
}
#endif

} // namespace

VideoPlayer::VideoPlayer(QObject* parent)
  : QObject(parent)
{
  g_instance = this;
#ifdef Q_OS_ANDROID
  const JNINativeMethod methods[] = {
    { "nativeAudioState", "(I)V", reinterpret_cast<void*>(jniAudioState) },
  };
  QJniEnvironment env;
  if (!env.registerNativeMethods(kShim, methods, std::size(methods)))
    qWarning("[media] could not register the shim's callback");
#endif
  // A sound preview stops when the app leaves the front (a video full
  // screen included: it has the sound then).
  connect(qGuiApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState state) {
    if (state != Qt::ApplicationActive && !m_audioSource.isEmpty())
      stopAudio();
  });
}

VideoPlayer::~VideoPlayer()
{
  if (g_instance == this)
    g_instance = nullptr;
}

VideoPlayer* VideoPlayer::instance()
{
  return g_instance;
}

bool VideoPlayer::available() const
{
#ifdef Q_OS_ANDROID
  return true;
#else
  return false;
#endif
}

void VideoPlayer::resolve(const QString& url, std::function<void(const QString&)> done)
{
  if (!isWeb(url)) {
    done({});
    return;
  }
  QPointer<VideoPlayer> self(this);
  const QByteArray utf8 = url.toUtf8();
  yap_media_playable(utf8.constData(), size_t(utf8.size()), playableDone,
                     new Pending{ [self, done = std::move(done)](const QString& playable) {
                       QMetaObject::invokeMethod(qApp, [self, done, playable] {
                         if (self)
                           done(playable);
                       }, Qt::QueuedConnection);
                     } });
}

void VideoPlayer::play(const QString& url, qint64 positionMs, bool loop, const QString& title)
{
#ifdef Q_OS_ANDROID
  resolve(url, [this, url, positionMs, loop, title](const QString& playable) {
    if (playable.isEmpty()) {
      emit failed(url);
      return;
    }
    QJniObject::callStaticMethod<void>(
      kShim, "play", "(Landroid/content/Context;Ljava/lang/String;JZLjava/lang/String;)V",
      androidContext().object(), QJniObject::fromString(playable).object<jstring>(), jlong(positionMs),
      jboolean(loop), QJniObject::fromString(title).object<jstring>());
  });
#else
  Q_UNUSED(positionMs);
  Q_UNUSED(loop);
  Q_UNUSED(title);
  emit failed(url);
#endif
}

void VideoPlayer::playAudio(const QString& url)
{
  const quint64 request = ++m_audioRequest;
  setAudio(url, Loading);
#ifdef Q_OS_ANDROID
  resolve(url, [this, url, request](const QString& playable) {
    if (request != m_audioRequest)
      return;
    if (playable.isEmpty()) {
      setAudio(url, Failed);
      return;
    }
    QJniObject::callStaticMethod<void>(kShim, "playAudio", "(Landroid/content/Context;Ljava/lang/String;)V",
                                       androidContext().object(),
                                       QJniObject::fromString(playable).object<jstring>());
  });
#else
  setAudio(url, Failed);
#endif
}

void VideoPlayer::stopAudio()
{
  ++m_audioRequest;
#ifdef Q_OS_ANDROID
  QJniObject::callStaticMethod<void>(kShim, "stopAudio", "()V");
#endif
  setAudio({}, Idle);
}

void VideoPlayer::onAudioState(int state)
{
  if (m_audioSource.isEmpty())
    return;
  switch (state) {
  case Loading:
    setAudio(m_audioSource, Loading);
    break;
  case Playing:
    setAudio(m_audioSource, Playing);
    break;
  case Failed:
    setAudio(m_audioSource, Failed);
    break;
  default:
    // Ended, or stopped by Android (audio focus, headphones out).
    setAudio({}, Idle);
    break;
  }
}

void VideoPlayer::setAudio(const QString& source, AudioState state)
{
  if (source == m_audioSource && state == m_audioState)
    return;
  m_audioSource = source;
  m_audioState = state;
  emit audioChanged();
}

// ---- Posters ---------------------------------------------------------------

namespace {

// Two grabs at once: each holds a hardware decoder and a connection.
QThreadPool* posterPool()
{
  static QThreadPool* pool = [] {
    auto* p = new QThreadPool;
    p->setMaxThreadCount(2);
    p->setExpiryTimeout(10000);
    return p;
  }();
  return pool;
}

class PosterResponse : public QQuickImageResponse
{
public:
  QQuickTextureFactory* textureFactory() const override
  {
    return QQuickTextureFactory::textureFactoryForImage(m_image);
  }
  QString errorString() const override { return m_error; }
  void cancel() override { m_cancelled.store(true); }

  void run(const QString& url, QSize size)
  {
    if (!m_cancelled.load())
      grab(url, size);
    if (m_image.isNull() && m_error.isEmpty())
      m_error = QStringLiteral("no poster");
    emit finished();
  }

private:
  void grab(const QString& url, QSize size)
  {
#ifdef Q_OS_ANDROID
    const QString playable = isWeb(url) ? resolveBlocking(url) : QString();
    if (playable.isEmpty() || m_cancelled.load())
      return;
    // Without a size: a chat box's worth.
    const int w = size.width() > 0 ? size.width() : 960;
    const int h = size.height() > 0 ? size.height() : 960;
    QJniObject bytes = QJniObject::callStaticObjectMethod(
      kShim, "poster", "(Ljava/lang/String;II)[B", QJniObject::fromString(playable).object<jstring>(), jint(w),
      jint(h));
    if (!bytes.isValid())
      return;
    QJniEnvironment env;
    auto array = bytes.object<jbyteArray>();
    const jsize len = env->GetArrayLength(array);
    QByteArray jpeg(len, Qt::Uninitialized);
    env->GetByteArrayRegion(array, 0, len, reinterpret_cast<jbyte*>(jpeg.data()));
    m_image = QImage::fromData(jpeg, "JPEG");
#else
    Q_UNUSED(url);
    Q_UNUSED(size);
#endif
  }

  QImage m_image;
  QString m_error;
  std::atomic<bool> m_cancelled{ false };
};

} // namespace

VideoPosterProvider::VideoPosterProvider() = default;
VideoPosterProvider::~VideoPosterProvider() = default;

QQuickImageResponse* VideoPosterProvider::requestImageResponse(const QString& id, const QSize& requestedSize)
{
  auto* response = new PosterResponse;
  // The id is the URL, percent-encoded by the QML (encodeURIComponent).
  const QString url = QUrl::fromPercentEncoding(id.toUtf8());
  // The engine deletes the response only after finished(), which run()
  // emits last.
  posterPool()->start([response, url, requestedSize] { response->run(url, requestedSize); });
  return response;
}
