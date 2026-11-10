// Entry point: a Qt Quick app whose logic lives in the Rust crate
// (YapClient, registered by the com.yap.native QML module).

#include <QtCore/QAbstractNativeEventFilter>
#include <QtCore/QDirIterator>
#include <QtCore/QPointer>
#include <QtCore/QElapsedTimer>
#include <QtCore/QFile>
#include <QtCore/QHash>
#include <QtCore/QMetaProperty>
#include <QtCore/QSet>
#include <QtCore/QTimer>
#include <QtGui/QFont>
#include <QtGui/QFontDatabase>
#include <QtGui/QScreen>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QSurfaceFormat>
#include <QtGui/QEnterEvent>
#include <QtGui/QMouseEvent>
#include <QtGui/QPointingDevice>
#include <QtGui/QWindow>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkDiskCache>
#include <QtNetwork/QNetworkReply>
#include <QtNetwork/QNetworkRequest>
#include <QtQml/QQmlApplicationEngine>
#include <QtQml/QQmlComponent>
#include <QtQml/QQmlContext>
#include <QtQml/QQmlNetworkAccessManagerFactory>
#ifdef QT_QML_DEBUG
// The profiling build (CMake YAP_QML_PROFILER=ON): this header's static
// enabler turns the debug services on.
#include <QtQml/qqmldebug.h>
#endif
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <QtQuickControls2/QQuickStyle>

#include <algorithm>
#include <cstdio>
#include <functional>

#include "animatedpicture.h"
#include "authmedia.h"
#include "camera.h"
#include "compat.h"
#include "composerhighlighter.h"
#include "crashnotice.h"
#include "desktopshell.h"
#include "ditherprovider.h"
#include "eyedropper.h"
#include "heaptrace.h"
#include "iconprovider.h"
#include "logfile.h"
#include "mediashell.h"
#include "profile.h"
#include "sampler.h"
#include "shareprovider.h"
#include "thumbbar.h"
#include "videohub.h"
#include "webembed.h"
#include "webmplayer.h"
#include "wheelscroll.h"
#include "windowchrome.h"

#ifdef Q_OS_WIN
#include <qpa/qwindowsysteminterface.h>
#include <windows.h>
#include <dwrite.h>
#include <string>
#endif

#ifdef Q_OS_WIN
// YAP_TEST_POINTER=1 (test runs only): a pointer that lives inside the
// process. A test tool posts the registered message "YapTestPointer" to
// our window, with the point in client pixels in lParam and in wParam
// 0 move (hover), 1 leave, 2 left press, 3 left release. It becomes the
// same QMouseEvent a real pointer would give the QQuickWindow, so hover,
// tooltips, hover cards and pressed states show on a window that is
// behind others (a posted WM_MOUSEMOVE gives none: Qt resolves the window
// under the screen point). The real cursor, keyboard and focus are never
// touched.
//
// The test keyboard, "YapTestKey": wParam a Qt::Key, lParam the
// Qt::KeyboardModifiers. The press and release go through
// QWindowSystemInterface as a real key's would, so shortcuts and the focus
// item both see them (a posted WM_KEYDOWN carries no Ctrl or Shift: Qt
// reads those from the real keyboard). Key 0 only holds the modifiers for
// the pointer's events that follow (Shift over a row), until the next.
class TestPointer : public QAbstractNativeEventFilter
{
public:
  explicit TestPointer(QQuickWindow* window)
    : m_window(window), m_message(RegisterWindowMessageW(L"YapTestPointer")),
      m_keyMessage(RegisterWindowMessageW(L"YapTestKey"))
  {
  }

  bool nativeEventFilter(const QByteArray&, void* message, qintptr*) override
  {
    auto* msg = static_cast<MSG*>(message);
    if (!m_window || msg->hwnd != reinterpret_cast<HWND>(m_window->winId()))
      return false;
    if (m_keyMessage != 0 && msg->message == m_keyMessage) {
      // wParam 0xFFFF: type the code point in lParam (an emoji, which a
      // posted WM_CHAR pair may not deliver).
      if (msg->wParam == 0xFFFF) {
        const char32_t cp = char32_t(msg->lParam);
        const QString text = QString::fromUcs4(&cp, 1);
        QWindowSystemInterface::handleKeyEvent<QWindowSystemInterface::SynchronousDelivery>(
          m_window, QEvent::KeyPress, Qt::Key_unknown, Qt::NoModifier, text);
        QWindowSystemInterface::handleKeyEvent<QWindowSystemInterface::SynchronousDelivery>(
          m_window, QEvent::KeyRelease, Qt::Key_unknown, Qt::NoModifier, text);
        return true;
      }
      key(int(msg->wParam), Qt::KeyboardModifiers(int(msg->lParam)));
      return true;
    }
    if (m_message == 0 || msg->message != m_message)
      return false;
    const qreal dpr = m_window->devicePixelRatio();
    const QPointF local(qreal(short(LOWORD(msg->lParam))) / dpr,
                        qreal(short(HIWORD(msg->lParam))) / dpr);
    const QPointF global = m_window->mapToGlobal(local);
    switch (msg->wParam) {
    case 1: {
      QEvent leave(QEvent::Leave);
      QCoreApplication::sendEvent(m_window, &leave);
      m_inside = false;
      m_buttons = Qt::NoButton;
      break;
    }
    case 2:
    case 3: {
      enter(local, global);
      const bool press = msg->wParam == 2;
      m_buttons = press ? Qt::LeftButton : Qt::NoButton;
      QMouseEvent ev(press ? QEvent::MouseButtonPress : QEvent::MouseButtonRelease,
                     local, local, global, Qt::LeftButton, m_buttons, m_mods);
      QCoreApplication::sendEvent(m_window, &ev);
      break;
    }
    default: {
      enter(local, global);
      QMouseEvent ev(QEvent::MouseMove, local, local, global, Qt::NoButton, m_buttons,
                     m_mods);
      QCoreApplication::sendEvent(m_window, &ev);
      break;
    }
    }
    return true;
  }

private:
  void key(int code, Qt::KeyboardModifiers mods)
  {
    m_mods = mods;
    if (code == 0)
      return;
    // The text a US layout gives: letters follow Shift, other printable
    // keys their own character; control keys none (Ctrl+L is a shortcut).
    QString text;
    if (!(mods & (Qt::ControlModifier | Qt::AltModifier)) && code >= 0x20 && code < 0x7f) {
      const QChar c(code);
      text = (mods & Qt::ShiftModifier) ? QString(c) : QString(c.toLower());
    }
    QWindowSystemInterface::handleKeyEvent<QWindowSystemInterface::SynchronousDelivery>(
      m_window, QEvent::KeyPress, code, mods, text);
    QWindowSystemInterface::handleKeyEvent<QWindowSystemInterface::SynchronousDelivery>(
      m_window, QEvent::KeyRelease, code, mods, text);
    m_mods = Qt::NoModifier;
  }

  void enter(const QPointF& local, const QPointF& global)
  {
    if (m_inside)
      return;
    m_inside = true;
    QEnterEvent ev(local, local, global);
    QCoreApplication::sendEvent(m_window, &ev);
  }

  QPointer<QQuickWindow> m_window;
  UINT m_message;
  UINT m_keyMessage;
  bool m_inside = false;
  Qt::MouseButtons m_buttons = Qt::NoButton;
  Qt::KeyboardModifiers m_mods = Qt::NoModifier;
};
#endif

// Presses stop at the popup they land in. In Qt 6.6, once an item takes a
// press, the pointer handlers of every item under the point still get it
// (QQuickDeliveryAgent goes on "handlersOnly"), so a TapHandler beneath a
// popup, modal or not, takes a passive grab and taps on the release:
// picking a clip in the vibe picker also tapped "Set a channel status"
// under it, and a picker's tab or category cell opened the channel under
// it. Two parts:
//   - a press on a popup (or a modal popup's dimmer) is delivered here
//     first; then every passive grab it left with a handler outside that
//     popup is cancelled (the handler sees a cancel, as when a grab is
//     stolen), so nothing beneath taps, long-presses or stays pressed;
//   - each popup gets a bottom-most MouseArea over its item that accepts
//     the presses nothing inside took, so items beneath never see them,
//     and a wheel nothing inside took stops there too, rather than
//     scrolling the chat under a dialog.
// Tooltips and drawers (a drawer drags from its own item) name their
// background `yapNoPressSink` and are left alone.
class PopupPressSink : public QObject
{
public:
  PopupPressSink(QQmlEngine* engine, QQuickWindow* window)
    : QObject(window), m_engine(engine), m_window(window), m_component(engine)
  {
    m_component.setData(
      "import QtQuick\n"
      "MouseArea { anchors.fill: parent; z: -100000; acceptedButtons: Qt.AllButtons;"
      " onWheel: wheel => wheel.accepted = true }\n",
      QUrl(QStringLiteral("qrc:/yap/PopupPressSink.qml")));
    QQuickItem* content = window->contentItem();
    connect(content, &QQuickItem::childrenChanged, this, [this, content] { findOverlay(content); });
    findOverlay(content);
    window->installEventFilter(this);
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override
  {
    if (watched != m_window || m_delivering || event->type() != QEvent::MouseButtonPress)
      return false;
    auto* press = static_cast<QMouseEvent*>(event);
    QQuickItem* owner = ownerAt(press->scenePosition());
    if (!owner)
      return false;
    // Deliver it now, then take back what it gave the handlers beneath.
    m_delivering = true;
    QCoreApplication::sendEvent(m_window, press);
    m_delivering = false;
    const QPointingDevice* device = press->pointingDevice();
    for (const QEventPoint& point : press->points()) {
      const QList<QPointer<QObject>> grabbers = press->passiveGrabbers(point);
      for (const QPointer<QObject>& grabber : grabbers) {
        if (!grabber || !grabber->inherits("QQuickPointerHandler"))
          continue;
        auto* parent = grabber->property("parent").value<QQuickItem*>();
        if (parent && (parent == owner || owner->isAncestorOf(parent)))
          continue;
        emit device->grabChanged(grabber, QPointingDevice::CancelGrabPassive, press, point);
        if (grabber)
          press->removePassiveGrabber(point, grabber);
      }
    }
    return true;
  }

private:
  static bool optedOut(QQuickItem* item)
  {
    const QList<QQuickItem*> parts = item->childItems();
    return std::any_of(parts.begin(), parts.end(), [](QQuickItem* part) {
      return part->objectName() == QLatin1String("yapNoPressSink");
    });
  }

  // The topmost open popup item (or modal dimmer) under a scene point.
  QQuickItem* ownerAt(const QPointF& scenePos) const
  {
    if (!m_overlay || !m_overlay->isVisible())
      return nullptr;
    const QList<QQuickItem*> layers = m_overlay->childItems();
    QQuickItem* top = nullptr;
    for (QQuickItem* layer : layers) {
      if (!layer->isVisible() || layer->opacity() <= 0
          || !layer->contains(layer->mapFromScene(scenePos)))
        continue;
      if (!top || layer->z() >= top->z())
        top = layer;
    }
    // An opted-out popup on top (a drawer) takes the press the usual way.
    // Skipping it made its own modal dimmer the owner, and every passive
    // grab inside the drawer was cancelled: a TapHandler in a sheet (the
    // voice chat's title) never tapped.
    if (top && top->inherits("QQuickPopupItem") && optedOut(top))
      return nullptr;
    return top;
  }

  void findOverlay(QQuickItem* content)
  {
    if (m_overlay)
      return;
    for (QQuickItem* child : content->childItems()) {
      if (child->inherits("QQuickOverlay")) {
        m_overlay = child;
        connect(child, &QQuickItem::childrenChanged, this, &PopupPressSink::sinkPopups);
        sinkPopups();
        return;
      }
    }
  }

  void sinkPopups()
  {
    if (!m_overlay)
      return;
    for (QQuickItem* item : m_overlay->childItems()) {
      if (!item->inherits("QQuickPopupItem") || item->property("_yap_sunk").toBool())
        continue;
      item->setProperty("_yap_sunk", true);
      if (optedOut(item))
        continue;
      QObject* made = m_component.beginCreate(m_engine->rootContext());
      auto* sink = qobject_cast<QQuickItem*>(made);
      if (!sink) {
        delete made;
        std::fprintf(stderr, "[popup] press sink: %s\n", qPrintable(m_component.errorString()));
        return;
      }
      sink->setParent(item);
      sink->setParentItem(item);
      m_component.completeCreate();
      if (++m_sunk == 1)
        std::fprintf(stderr, "[popup] presses stop at popups\n");
    }
  }

  int m_sunk = 0;
  bool m_delivering = false;
  QQmlEngine* m_engine;
  QQuickWindow* m_window;
  QQmlComponent m_component;
  QPointer<QQuickItem> m_overlay;
};

// The text hinting (see main's font block): none, as Chromium draws our
// fonts. YAP_TEXT_HINTING=none|vertical|full|default forces one.
static QFont::HintingPreference textHinting()
{
  const QString forced = qEnvironmentVariable("YAP_TEXT_HINTING");
  if (forced == QLatin1String("vertical"))
    return QFont::PreferVerticalHinting;
  if (forced == QLatin1String("full"))
    return QFont::PreferFullHinting;
  if (forced == QLatin1String("default"))
    return QFont::PreferDefaultHinting;
  return QFont::PreferNoHinting;
}

#ifdef Q_OS_WIN
// The contrast and gamma subpixel text is blended with, and whether it is
// subpixel at all, as Chromium picks them (Qt patch 0006 reads them from
// QSG_TEXT_*; docs/LIBRARIES.md). Chromium (Skia) blends with contrast 1.0
// and the sRGB curve unless the ClearType Tuner has saved values for the
// monitor (HKCU\Software\Microsoft\Avalon.Graphics\DISPLAYn), and then
// follows those: DirectWrite's monitor rendering params carry them
// (EnhancedContrastLevel / 100, GammaLevel / 1000). Untuned, DirectWrite
// reports its own defaults (0.5, 1.8), which Chromium does not use: blended
// with them our text measured further from Edge's than with Skia's (text
// pass, docs/UI_PARITY_AUDIT.md). With font smoothing off or set to
// Standard, text is grayscale. The primary monitor's settings, read once
// at launch. An explicit QSG_TEXT_* in the environment wins.
static void setTextContrastAndGamma()
{
  BOOL smoothing = TRUE;
  UINT type = FE_FONTSMOOTHINGCLEARTYPE;
  SystemParametersInfoW(SPI_GETFONTSMOOTHING, 0, &smoothing, 0);
  SystemParametersInfoW(SPI_GETFONTSMOOTHINGTYPE, 0, &type, 0);
  const bool clearType = smoothing && type == FE_FONTSMOOTHINGCLEARTYPE;
  if (!clearType && !qEnvironmentVariableIsSet("QSG_TEXT_GRAYSCALE"))
    qputenv("QSG_TEXT_GRAYSCALE", "1");

  MONITORINFOEXW info{};
  info.cbSize = sizeof info;
  const HMONITOR primary = MonitorFromPoint(POINT{ 0, 0 }, MONITOR_DEFAULTTOPRIMARY);
  if (!GetMonitorInfoW(primary, &info))
    return;
  // "\\.\DISPLAY3" -> "DISPLAY3", the tuner's key.
  const std::wstring device(info.szDevice);
  const std::wstring key = L"Software\\Microsoft\\Avalon.Graphics\\" +
                           device.substr(device.find_last_of(L'\\') + 1);
  DWORD contrastLevel = 0, gammaLevel = 0, size = sizeof(DWORD);
  const bool tunedContrast =
    RegGetValueW(HKEY_CURRENT_USER, key.c_str(), L"EnhancedContrastLevel", RRF_RT_REG_DWORD,
                 nullptr, &contrastLevel, &size) == ERROR_SUCCESS;
  size = sizeof(DWORD);
  const bool tunedGamma = RegGetValueW(HKEY_CURRENT_USER, key.c_str(), L"GammaLevel",
                                       RRF_RT_REG_DWORD, nullptr, &gammaLevel, &size) == ERROR_SUCCESS;
  float contrast = 1.0f, gamma = 0.0f; // Skia's: 0 is the sRGB curve
  if (tunedContrast || tunedGamma) {
    IDWriteFactory* factory = nullptr;
    IDWriteRenderingParams* params = nullptr;
    if (SUCCEEDED(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory),
                                      reinterpret_cast<IUnknown**>(&factory))) &&
        SUCCEEDED(factory->CreateMonitorRenderingParams(primary, &params))) {
      if (tunedContrast)
        contrast = params->GetEnhancedContrast();
      if (tunedGamma)
        gamma = params->GetGamma();
      params->Release();
    }
    if (factory)
      factory->Release();
  }
  if (!qEnvironmentVariableIsSet("QSG_TEXT_CONTRAST"))
    qputenv("QSG_TEXT_CONTRAST", QByteArray::number(contrast));
  if (!qEnvironmentVariableIsSet("QSG_TEXT_GAMMA"))
    qputenv("QSG_TEXT_GAMMA", QByteArray::number(gamma));
  std::fprintf(stderr, "[text] %s, contrast %s, gamma %s (%s)\n",
               clearType ? "subpixel" : "grayscale", qgetenv("QSG_TEXT_CONTRAST").constData(),
               qgetenv("QSG_TEXT_GAMMA").constData(),
               tunedContrast || tunedGamma ? "ClearType Tuner" : "Chromium's defaults");
}
#endif

// The brand faces, one static cut per family (assets/fonts, built by
// tools/build-fonts.py). Registered before QML so Theme's families resolve.
static void registerFonts()
{
  QDirIterator it(QStringLiteral(":/fonts"), { QStringLiteral("*.ttf") });
  int count = 0;
  while (it.hasNext()) {
    if (QFontDatabase::addApplicationFont(it.next()) < 0)
      std::fprintf(stderr, "[fonts] failed to load %s\n",
                   qPrintable(it.filePath()));
    else
      ++count;
  }
  if (count == 0)
    std::fprintf(stderr, "[fonts] no brand fonts in the binary\n");
}

// The session for an image URL, from the Rust core (src/media.rs): the
// token's length when the URL is the API's own attachment or custom emoji
// (0 for anything else, which never gets it). Writes at most `cap` bytes.
extern "C" size_t yap_media_auth(const char* url, size_t len, char* out, size_t cap);

// Attachments and custom emoji images are private (`/api/attachments/*`
// answers 404 without the session): the loader adds the bearer to exactly
// those requests, so URLs and the disk cache never hold the token.
class MediaNetworkAccessManager : public QNetworkAccessManager
{
public:
  using QNetworkAccessManager::QNetworkAccessManager;

protected:
  QNetworkReply* createRequest(Operation op, const QNetworkRequest& request,
                               QIODevice* data) override
  {
    if (op != GetOperation)
      return QNetworkAccessManager::createRequest(op, request, data);
    // HTTP/1.1 only. Qt 6.6's HTTP/2 can leave a reply that never finishes,
    // and QML's image reader runs at most 8 network loads at once
    // (IMAGEREQUEST_MAX_NETWORK_REQUEST_COUNT), so a few stalled replies
    // froze every later image: the emoji picker showed one row, for good.
    QNetworkRequest plain(request);
    plain.setAttribute(QNetworkRequest::Http2AllowedAttribute, false);
    const QByteArray url = plain.url().toEncoded();
    char token[1024];
    const size_t n = yap_media_auth(url.constData(), size_t(url.size()), token, sizeof token);
    if (n == 0 || n > sizeof token)
      return QNetworkAccessManager::createRequest(op, plain, data);
    QNetworkRequest authed(plain);
    // Qt copies the request, headers included, onto a redirect: only the
    // API's own origin may see the bearer, so a redirect elsewhere fails.
    authed.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                        QNetworkRequest::SameOriginRedirectPolicy);
    // Private images stay out of the disk cache (and out of the next
    // account's reach): never stored, never read back.
    authed.setAttribute(QNetworkRequest::CacheSaveControlAttribute, false);
    authed.setAttribute(QNetworkRequest::CacheLoadControlAttribute,
                        QNetworkRequest::AlwaysNetwork);
    authed.setRawHeader("Authorization", QByteArray("Bearer ") + QByteArray(token, int(n)));
    return QNetworkAccessManager::createRequest(op, authed, data);
  }
};

// Every network image QML loads (avatars, icons, banners, attachments,
// emoji, embeds, GIFs) goes through a disk cache next to the snapshot
// cache, apart from the installed app's folders. Avatars are public
// (`/api/avatars/*`, no token); their ETag revalidates a cached copy.
class CachedNetworkFactory : public QQmlNetworkAccessManagerFactory
{
public:
  // Emptied on sign-out (MediaShell::clearImageCache).
  MediaShell* shell = nullptr;

  QNetworkAccessManager* create(QObject* parent) override
  {
    auto* manager = new MediaNetworkAccessManager(parent);
    const QString root = QString::fromStdWString(profile::dataRoot());
    if (!root.isEmpty()) {
      auto* cache = new QNetworkDiskCache(manager);
      cache->setCacheDirectory(root + QStringLiteral("/cache/images"));
      cache->setMaximumCacheSize(512 * 1024 * 1024);
      manager->setCache(cache);
      // The cache lives in the manager's thread: the queued call runs there.
      if (shell)
        QObject::connect(shell, &MediaShell::imageCacheCleared, cache,
                         &QNetworkDiskCache::clear);
    }
    return manager;
  }
};

static bool reducedMotion()
{
  // YAP_REDUCED_MOTION=1 or 0 (tests) wins over the system setting.
  if (qEnvironmentVariableIsSet("YAP_REDUCED_MOTION"))
    return qEnvironmentVariableIntValue("YAP_REDUCED_MOTION") != 0;
#ifdef Q_OS_WIN
  // The switch Windows' "Animation effects" setting flips (DESIGN.md:480).
  BOOL animate = TRUE;
  SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &animate, 0);
  return !animate;
#else
  return false;
#endif
}

#ifdef Q_OS_WIN
// Theme.reducedMotion follows "Animation effects" while the app runs, as
// the web's prefers-reduced-motion does: flipping it broadcasts
// WM_SETTINGCHANGE (SPI_SETCLIENTAREAANIMATION) to our windows, and Qt
// hands each window message to the native event filters.
class MotionWatch : public QAbstractNativeEventFilter
{
public:
  explicit MotionWatch(QObject* theme) : m_theme(theme) {}

  bool nativeEventFilter(const QByteArray&, void* message, qintptr*) override
  {
    const auto* msg = static_cast<const MSG*>(message);
    if (msg->message != WM_SETTINGCHANGE || msg->wParam != SPI_SETCLIENTAREAANIMATION || !m_theme)
      return false;
    const bool reduced = reducedMotion();
    if (m_theme->property("reducedMotion").toBool() != reduced) {
      m_theme->setProperty("reducedMotion", reduced);
      std::fprintf(stderr, "[motion] reduced motion %s\n", reduced ? "on" : "off");
    }
    return false;
  }

private:
  QPointer<QObject> m_theme;
};
#endif

// Qt's messages go to stderr (and so into the log file) whatever the
// subsystem; Windows would otherwise send them to the debugger only.
static void qtMessage(QtMsgType type, const QMessageLogContext&,
                      const QString& message)
{
  static const char* const kinds[] = { "debug", "warning", "critical",
                                       "fatal", "info" };
  const int kind = int(type);
  std::fprintf(stderr, "[qt] %s: %s\n",
               kind >= 0 && kind < 5 ? kinds[kind] : "message",
               qPrintable(message));
}

// YAP_DEBUG_ANIMATIONS=N (diagnostics): N seconds after launch, and every N
// seconds after that, lists the QML animations that are running and where
// they are declared. A running animation ticks every frame, so an idle
// window should list none.
static void dumpRunningAnimations(QQmlApplicationEngine& engine)
{
  int running = 0;
  for (QObject* root : engine.rootObjects()) {
    const auto all = root->findChildren<QObject*>();
    for (QObject* object : all) {
      const QMetaObject* meta = object->metaObject();
      bool animation = false;
      for (const QMetaObject* m = meta; m; m = m->superClass())
        if (qstrcmp(m->className(), "QQuickAbstractAnimation") == 0)
          animation = true;
      if (!animation || !object->property("running").toBool())
        continue;
      ++running;
      QQmlContext* context = qmlContext(object);
      QObject* target = object->property("target").value<QObject*>();
      QObject* owner = object->parent();
      std::fprintf(stderr, "[anim-debug] %s in %s (parent %s, target %s, property %s, loops %d)\n",
                   meta->className(),
                   context ? qPrintable(context->baseUrl().fileName()) : "?",
                   owner ? owner->metaObject()->className() : "-",
                   target ? target->metaObject()->className() : "-",
                   qPrintable(object->property("property").toString()),
                   object->property("loops").toInt());
    }
  }
  int ticking = 0;
  for (QObject* root : engine.rootObjects())
    for (AnimatedPicture* picture : root->findChildren<AnimatedPicture*>())
      if (picture->ticking()) {
        ++ticking;
        std::fprintf(stderr, "[anim-debug] AnimatedPicture ticking: %s %gx%g\n",
                     qPrintable(picture->source().fileName()), picture->width(),
                     picture->height());
      }
  // Qt's own GIF players, and whether each viewport watch sees its item.
  // Through the item tree: a list's delegates are not QObject children.
  std::function<void(QQuickItem*)> walk = [&walk](QQuickItem* item) {
    if (item->inherits("QQuickAnimatedImage"))
      std::fprintf(stderr, "[anim-debug] AnimatedImage playing %d paused %d frame %d/%d status %d shown %d\n",
                   item->property("playing").toBool(), item->property("paused").toBool(),
                   item->property("currentFrame").toInt(), item->property("frameCount").toInt(),
                   item->property("status").toInt(), visibleInWindow(item));
    else if (auto* watch = qobject_cast<ViewportWatch*>(item))
      std::fprintf(stderr, "[anim-debug] ViewportWatch inView %d visible %d %gx%g\n",
                   watch->inView(), watch->isVisible(), watch->width(), watch->height());
    for (QQuickItem* child : item->childItems())
      walk(child);
  };
  for (QObject* root : engine.rootObjects())
    if (auto* window = qobject_cast<QQuickWindow*>(root))
      walk(window->contentItem());
  // Live media players (the GIF picker's clips, embeds, previews): each
  // one is a Media Foundation session or a libvpx decoder. Through the
  // whole item tree, popups' overlay included (players are QObject
  // children of their items).
  int players = 0, playing = 0, webms = 0, audio = 0;
  std::function<void(QQuickItem*)> media = [&](QQuickItem* item) {
    for (QObject* child : item->children()) {
      if (child->inherits("QMediaPlayer") || child->inherits("WebmPlayer")) {
        ++players;
        webms += child->inherits("WebmPlayer");
        playing += child->property("playbackState").toInt() == 1;
        for (QObject* sub : child->children())
          audio += sub->inherits("QAudioOutput");
      } else if (child->inherits("QAudioOutput")) {
        ++audio;
      }
    }
    for (QQuickItem* child : item->childItems())
      media(child);
  };
  for (QObject* root : engine.rootObjects())
    if (auto* window = qobject_cast<QQuickWindow*>(root))
      media(window->contentItem()->parentItem() ? window->contentItem()->parentItem()
                                                : window->contentItem());
  std::fprintf(stderr, "[anim-debug] media: %d players (%d playing, %d webm), %d audio outputs\n",
               players, playing, webms, audio);
  QWindow* focus = QGuiApplication::focusWindow();
  std::fprintf(stderr, "[anim-debug] %d running, %d pictures ticking, app state %d, focus %s\n",
               running, ticking, int(QGuiApplication::applicationState()),
               focus ? qPrintable(focus->title()) : "none");
}

// A QML type's name without the engine's "_QMLTYPE_n" / "_QML_n" suffix.
static QString typeName(const QObject* object)
{
  QString name = QString::fromLatin1(object->metaObject()->className());
  const qsizetype cut = name.indexOf(QLatin1String("_QML"));
  return cut > 0 ? name.left(cut) : name;
}

// YAP_DEBUG_OBJECTS=N (diagnostics): every N seconds, the number of objects
// alive in the window's trees (closed popups' trees included, which hang off
// the popup, not the window), split by the outermost popup that owns them,
// largest first, then the files that declare the most of the rest.
static void dumpObjectCounts(QQmlApplicationEngine& engine)
{
  QSet<QObject*> seen;
  QHash<QString, int> byPopup;
  QHash<QString, int> byFile;
  int total = 0;
  std::function<void(QObject*, const QString&)> visit = [&](QObject* object,
                                                            const QString& popup) {
    if (!object || seen.contains(object))
      return;
    seen.insert(object);
    ++total;
    QString owner = popup;
    const bool isPopup = object->inherits("QQuickPopup");
    if (owner.isEmpty() && isPopup) {
      QQmlContext* context = qmlContext(object);
      owner = typeName(object) + QLatin1String(" (") +
              (context ? context->baseUrl().fileName() : QStringLiteral("?")) +
              QLatin1Char(')');
    }
    if (!owner.isEmpty()) {
      ++byPopup[owner];
    } else {
      QQmlContext* context = qmlContext(object);
      ++byFile[context ? context->baseUrl().fileName() : QStringLiteral("(no context)")];
    }
    for (QObject* child : object->children())
      visit(child, owner);
    if (auto* item = qobject_cast<QQuickItem*>(object))
      for (QQuickItem* child : item->childItems())
        visit(child, owner);
    if (isPopup) {
      // The popup item (contentItem's parent) is not the popup's child.
      if (auto* content = object->property("contentItem").value<QQuickItem*>()) {
        visit(content->parentItem(), owner);
        visit(content, owner);
      }
      visit(object->property("background").value<QObject*>(), owner);
    }
  };
  for (QObject* root : engine.rootObjects()) {
    visit(root, QString());
    if (auto* window = qobject_cast<QQuickWindow*>(root))
      visit(window->contentItem(), QString());
  }
  int inPopups = 0;
  for (int n : std::as_const(byPopup))
    inPopups += n;
  std::fprintf(stderr, "[objects] %d alive, %d in %lld popups\n", total, inPopups,
               static_cast<long long>(byPopup.size()));
  auto top = [](const QHash<QString, int>& counts, const char* kind, int limit) {
    QList<QPair<int, QString>> rows;
    for (auto it = counts.cbegin(); it != counts.cend(); ++it)
      rows.append({ it.value(), it.key() });
    std::sort(rows.begin(), rows.end(), [](const auto& a, const auto& b) { return a.first > b.first; });
    for (int i = 0; i < rows.size() && i < limit; ++i)
      std::fprintf(stderr, "[objects] %s %6d %s\n", kind, rows[i].first,
                   qPrintable(rows[i].second));
  };
  top(byPopup, "popup", 40);
  top(byFile, "file ", 25);
  std::fflush(stderr);
}

// Velopack's startup handling (rust/src/installer.rs): an install, update or
// uninstall hook runs and exits here; any other launch returns at once.
extern "C" void yap_velopack_startup();
// The released install's default profile only (rust/src/installer.rs):
// imports the Tauri app's preferences once and arms its uninstall. A no-op
// for every other build and profile.
extern "C" void yap_replace_tauri_at_launch();
// Main.qml has its window: the armed Tauri uninstall runs 20 s later, so a
// build that crashes on start never removes the working Tauri app.
extern "C" void yap_tauri_app_started();
// The in-app updater (rust/src/updater.rs): with install on quit, hands a
// downloaded update to Velopack's updater. Not while Windows shuts down.
extern "C" void yap_updater_quit(bool session_ending);

int main(int argc, char* argv[])
{
  // YAP_DEBUG_HEAP (diagnostics): allocation stacks from here on.
  heaptrace::install();
  // YAP_DEBUG_SAMPLE (diagnostics): the GUI thread's CPU by stack.
  sampler::install();
  // First, before Qt and the instance lock: a hook must not show a window
  // or hand its arguments to a running instance.
  yap_velopack_startup();
  // A restart into GPU safe mode (cpp/compat.*) waits for the process it
  // replaces to let go of the instance lock.
  Compat::awaitPredecessor();
  QElapsedTimer startup;
  startup.start();
  DesktopShell::setProcessIdentity();
  // Only the first instance owns the log file; a later one only hands over.
  const bool first = DesktopShell::takeInstanceLock();
  if (first) {
    // Resolves the data folder (moving the old one, once) as it starts.
    logfile::start();
    // Before anything reads the settings.
    yap_replace_tauri_at_launch();
    // Minidumps from here on, into the profile's crashes folder.
    CrashNotice::install();
    // Before Qt: Media Foundation's presence, the GPU (adapter, feature
    // level, the crash guard) and, when needed, safe mode's environment.
    Compat::prepare();
  }
  qInstallMessageHandler(qtMessage);

  // No organization: Qt's own caches (QML, pipeline) then go to
  // %LOCALAPPDATA%\<applicationName>\cache. Not "Yap": that folder is the
  // Tauri app's install root, which its uninstaller deletes. So the name is
  // the data folder's (Yap.Data, yap_cache::profile::DATA_DIR); a named
  // profile suffixes it ("Yap.Data-jon"), so its caches are its own too.
  // No display name: Qt would append it to every window title.
  QGuiApplication::setApplicationName(
    QStringLiteral("Yap.Data") +
    QString::fromStdWString(profile::suffix(L'-')));
  // Fractional monitor scales as they are (125%, 150%, 175%), the way
  // Chromium renders them; QT_SCALE_FACTOR_ROUNDING_POLICY still overrides.
  // Per-monitor v2 awareness comes from the manifest (cpp/app.manifest).
  QGuiApplication::setHighDpiScaleFactorRoundingPolicy(
    Qt::HighDpiScaleFactorRoundingPolicy::PassThrough);
  // Animations on wall-clock time. Qt's default driver advances one
  // primary-screen vsync per frame, fixed at startup, so on a faster
  // secondary monitor (144 Hz beside a 60 Hz primary) every transition ran
  // that much faster. YAP_QSG_VSYNC_ANIMATIONS=1 keeps Qt's default.
  if (!qEnvironmentVariableIsSet("QSG_USE_SIMPLE_ANIMATION_DRIVER") &&
      qEnvironmentVariableIntValue("YAP_QSG_VSYNC_ANIMATIONS") == 0)
    qputenv("QSG_USE_SIMPLE_ANIMATION_DRIVER", "1");
  // Qt Quick's basic render loop: frames render on the GUI thread, paced by
  // Qt 6.9's DXGI vblank thread (QDxgiVSyncService), instead of on a render
  // thread of their own. Our frames are small (1-2 ms), so the threaded
  // loop's overlap bought nothing, and its per-frame handoff between the
  // two threads cost more than the frame: focused animated idle (a GIF rail
  // icon and two chat GIFs, 25.7 renders a second) went from 6.4 % to 3.9 %
  // of a core (docs/PERFORMANCE.md, pass 5). QSG_RENDER_LOOP=threaded keeps
  // Qt's default.
  if (!qEnvironmentVariableIsSet("QSG_RENDER_LOOP"))
    qputenv("QSG_RENDER_LOOP", "basic");
  // D3D11's blt-model swapchain (DXGI_SWAP_EFFECT_DISCARD) instead of Qt's
  // flip model. A blt Present lands in the window's redirection surface,
  // which DWM resizes together with the window, so a resize shows the new
  // frame with the new bounds; a flip-model buffer is composed on its own
  // and trailed the edge by two or more steps, over strips no surface
  // covered (black or white). Each resize step also took two vblanks
  // instead of one, and the flip model's frame-latency wait stalled for its
  // full 1 s timeout during a move-and-resize. Idle and scroll CPU are
  // unchanged (docs/PERFORMANCE.md, resize pass). QT_D3D_NO_FLIP=0 keeps
  // the flip model.
  if (!qEnvironmentVariableIsSet("QT_D3D_NO_FLIP"))
    qputenv("QT_D3D_NO_FLIP", "1");
  // The JS engine commits its whole JS and GC stacks up front (4 + 2 MB by
  // default, plus guards), every page charged to the process whether it is
  // touched or not. Our QML never recurses deeply: 1 MB of JS stack is
  // ~130,000 values, and a full GC stack only means an extra drain.
  if (!qEnvironmentVariableIsSet("QV4_JS_MAX_STACK_SIZE"))
    qputenv("QV4_JS_MAX_STACK_SIZE", QByteArray::number(1024 * 1024));
  if (!qEnvironmentVariableIsSet("QV4_GC_MAX_STACK_SIZE"))
    qputenv("QV4_GC_MAX_STACK_SIZE", QByteArray::number(512 * 1024));
  // YAP_DEBUG_NO_VSYNC=1 (measurements): presents never wait for the
  // compositor, so a run on a sleeping display (DWM pacing presents at
  // ~4 Hz) still renders every frame the content asks for.
  if (qEnvironmentVariableIntValue("YAP_DEBUG_NO_VSYNC") != 0) {
    QSurfaceFormat format = QSurfaceFormat::defaultFormat();
    format.setSwapInterval(0);
    QSurfaceFormat::setDefaultFormat(format);
  }
#ifdef Q_OS_WIN
  setTextContrastAndGamma();
#endif
  QGuiApplication app(argc, argv);
  // A second launch shows the first instance's window and exits.
  if (!first) {
    DesktopShell::showFirstInstance(app.arguments().mid(1));
    return 0;
  }
  QGuiApplication::setWindowIcon(QIcon(QStringLiteral(":/app/yap-256.png")));
  QQuickStyle::setStyle(QStringLiteral("Basic"));
  // Glyph-cache text instead of distance fields: crisper at 11-15 px and
  // closer to Chromium's DirectWrite output. YAP_TEXT=qt for the other.
  if (qEnvironmentVariable("YAP_TEXT") != QLatin1String("qt"))
    QQuickWindow::setTextRenderType(QQuickWindow::NativeTextRendering);
  // DirectWrite with no hinting at every scale, as Chromium draws our
  // fonts. Qt's default picks GDI's full hinting while every monitor is at
  // 100% and DirectWrite once one is scaled, so glyph widths (and with them
  // every wrapped line) changed with the monitor setup. No hinting is
  // DirectWrite's natural symmetric mode (antialiased both ways) with
  // subpixel glyph positions, which is what Chromium (Skia) uses for fonts
  // like ours that carry no hints of their own: matched against Edge's
  // output glyph by glyph it fits at 100% and 150% alike. Vertical hinting
  // (natural, antialiased across only) left ~17-20% fewer edge pixels than
  // Chromium at 100%: crisper stems, jagged curves (text pass,
  // docs/UI_PARITY_AUDIT.md).
  // YAP_TEXT_HINTING=none|vertical|full|default forces one.
  {
    QFont font = QGuiApplication::font();
    font.setHintingPreference(textHinting());
    QGuiApplication::setFont(font);
  }
  registerFonts();
  std::fprintf(stderr, "[startup] fonts after %lld ms\n",
               static_cast<long long>(startup.elapsed()));

  auto* chrome = new WindowChrome(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "WindowChrome", chrome);
  app.installNativeEventFilter(chrome);
  auto* shell = new DesktopShell(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "DesktopShell", shell);
  // The taskbar thumbnail's call keys, and the last run's crash notice.
  auto* thumbbar = new ThumbnailToolbar(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "Thumbbar", thumbbar);
  auto* crashes = new CrashNotice(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "CrashNotice", crashes);
  // GPU safe mode and Media Foundation, with their notices.
  auto* compat = new Compat(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "Compat", compat);
  auto* files = new MediaShell(&app);
  static CachedNetworkFactory networkFactory;
  networkFactory.shell = files;
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "Files", files);
  // The colour picker's eyedropper (YAP_TEST_EYEDROPPER: a set colour, no
  // window, no screen read).
  auto* eyedropper = new Eyedropper(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "Eyedropper", eyedropper);
  // Never deleted: media threads may still deliver a frame while the
  // window closes.
  auto* video = new VideoHub;
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "VideoHub", video);
  // Our camera, for the call and the preview (YAP_FAKE_CAMERA=1: the test
  // pattern, no device opened).
  auto* camera = new CameraCapture(&app);
  qmlRegisterSingletonInstance("Yap.Chrome", 1, 0, "Camera", camera);
  // APNG and animated WebP (decorations, effects, X GIFs), and the web
  // players (WebView2).
  qmlRegisterType<AnimatedPicture>("Yap.Media", 1, 0, "AnimatedPicture");
  qmlRegisterType<ViewportWatch>("Yap.Media", 1, 0, "ViewportWatch");
  qmlRegisterType<WebEmbed>("Yap.Media", 1, 0, "WebEmbed");
  qmlRegisterType<ComposerHighlighter>("Yap.Media", 1, 0,
                                       "ComposerHighlighter");
  // WebM (VP8/VP9, alpha too) through libvpx, and private video
  // attachments read with the session (Media Foundation for the rest).
  qmlRegisterType<WebmPlayer>("Yap.Media", 1, 0, "WebmPlayer");
  auto* authMedia = new AuthMedia(&app);
  qmlRegisterSingletonInstance("Yap.Media", 1, 0, "AuthMedia", authMedia);
  // Items QML draws over the chat list that a web player (a native child
  // window) must stay out from under: the unread banner.
  qmlRegisterSingletonInstance("Yap.Media", 1, 0, "PlayerCover", PlayerCover::instance());
  // The mouse wheel as Chromium scrolls (cpp/wheelscroll.h): 100 px a
  // notch eased over 150 ms, for every Flickable. YAP_QT_WHEEL=1 keeps Qt's
  // own wheel flick (the singleton is still there and reads inactive).
  auto* wheelScroll = new WheelScroll(&app);
  if (qEnvironmentVariableIntValue("YAP_QT_WHEEL") == 0)
    app.installEventFilter(wheelScroll);
  qmlRegisterSingletonInstance("Yap.Media", 1, 0, "WheelScroll", wheelScroll);

  QQmlApplicationEngine engine;
  // Not owned by the engine; it outlives it here.
  engine.setNetworkAccessManagerFactory(&networkFactory);
  engine.addImageProvider(QStringLiteral("icon"), new IconProvider);
  engine.addImageProvider(QStringLiteral("dither"), new DitherProvider);
  engine.addImageProvider(QStringLiteral("share"), new ShareProvider);
  QObject::connect(
    &engine, &QQmlApplicationEngine::objectCreationFailed, &app,
    [] { QCoreApplication::exit(-1); }, Qt::QueuedConnection);

  // Theme before the first binding evaluates: YAP_THEME, if set. Main.qml
  // then binds it to the saved choice (Settings.theme), which starts from
  // YAP_THEME too; assigning Theme.themeId re-themes everything.
  if (QObject* theme =
        engine.singletonInstance<QObject*>("com.yap.native", "Theme")) {
    const QString id = qEnvironmentVariable("YAP_THEME");
    if (!id.isEmpty())
      theme->setProperty("themeId", id);
    theme->setProperty("reducedMotion", reducedMotion());
  }
  // YAP_EMBED_MUTED=1 starts every chat player muted (test runs).
  if (QObject* state =
        engine.singletonInstance<QObject*>("com.yap.native", "ShellState"))
  {
    state->setProperty("mediaMuted", qEnvironmentVariableIntValue("YAP_EMBED_MUTED") != 0);
    // YAP_NO_CLIPBOARD=1: copies are logged, never put on the clipboard.
    state->setProperty("clipboardOff", qEnvironmentVariableIntValue("YAP_NO_CLIPBOARD") != 0);
    // YAP_TEST_HOVER_PLAY=1 (tests): what plays only while pointed at
    // (name effects, resting clips) plays as if hovered, since posted mouse
    // moves give a window behind others no hover.
    state->setProperty("testHoverPlay", qEnvironmentVariableIntValue("YAP_TEST_HOVER_PLAY") != 0);
    // YAP_TEST_NAME_STYLE=<json> (tests): every display name wears this
    // style locally (`{"font","effect","primary","secondary","wave"}`, the
    // shape the views carry); nothing is saved.
    state->setProperty("testNameStyle", qEnvironmentVariable("YAP_TEST_NAME_STYLE"));
  }

  engine.loadFromModule("com.yap.native", "Main");
  // Every Main.qml object created (the core started in its onCompleted).
  std::fprintf(stderr, "[startup] qml loaded after %lld ms\n",
               static_cast<long long>(startup.elapsed()));

  if (engine.rootObjects().isEmpty()) {
    // Not a GPU death: the crash guard does not count this run.
    Compat::quitting();
    return -1;
  }
  if (first)
    yap_tauri_app_started();

  if (auto* window =
        qobject_cast<QQuickWindow*>(engine.rootObjects().first())) {
    // Main.qml leaves the window hidden, so the first paint is already
    // captionless. The shell restores the saved rect and shows it (or
    // keeps it in the tray on an autostart launch with start minimized).
    // YAP_DEBUG_NO_ACTIVATE=1 (test runs): every show leaves the focus
    // where it is (Qt's SW_SHOWNOACTIVATE switch).
    if (qEnvironmentVariableIntValue("YAP_DEBUG_NO_ACTIVATE") != 0)
      window->setProperty("_q_showWithoutActivating", true);
    // Before the first show: the scene graph's error and device loss, and
    // the crash guard's first frame.
    compat->attach(window);
    // Before the first show: the D3D11 device the H.264 decoder shares, so
    // decoded video is drawn without a copy (VideoHub, zero copy).
    VideoHub::shareDevice(window);
    chrome->attach(window);
    thumbbar->attach(window);
    new PopupPressSink(&engine, window);
#ifdef Q_OS_WIN
    app.installNativeEventFilter(new MotionWatch(
      engine.singletonInstance<QObject*>("com.yap.native", "Theme")));
    std::fprintf(stderr, "[motion] reduced motion %s at start\n", reducedMotion() ? "on" : "off");
    if (qEnvironmentVariableIntValue("YAP_TEST_POINTER") != 0) {
      app.installNativeEventFilter(new TestPointer(window));
      std::fprintf(stderr, "[test] in-process pointer on (YAP_TEST_POINTER)\n");
    }
#endif
    // Deep links: our own yap:// and yap-native:// (registered per user where it
    // differs), from this command line and from later instances.
    shell->setLinkHandler(
      engine.singletonInstance<QObject*>("com.yap.native", "Invites"));
    shell->attach(window, engine.singletonInstance<QObject*>(
                            "com.yap.native", "Desktop"));
    shell->registerUrlScheme();
    for (const QString& arg : app.arguments().mid(1))
      shell->openArg(arg);
    // First frame on screen: printed for the cold-start measurement.
    QObject::connect(
      window, &QQuickWindow::frameSwapped, window,
      [&startup, shell] {
        std::fprintf(stderr, "[startup] first frame after %lld ms\n",
                     static_cast<long long>(startup.elapsed()));
        std::fflush(stderr);
        // Once startup has settled: the first frames' upload batches
        // (grown op lists), pooled buffers and unused glyph caches go back
        // (performance pass 7). The next frame rebuilds its pipelines once.
        QTimer::singleShot(20000, shell, [shell] { shell->releaseGraphicsSoon(); });
      },
      Qt::SingleShotConnection);
  }
  CrashNotice::scheduleTestCrash();
  if (const int every = qEnvironmentVariableIntValue("YAP_DEBUG_ANIMATIONS"); every > 0) {
    auto* dump = new QTimer(&app);
    QObject::connect(dump, &QTimer::timeout, &app, [&engine] { dumpRunningAnimations(engine); });
    dump->start(every * 1000);
  }
  if (const int every = qEnvironmentVariableIntValue("YAP_DEBUG_OBJECTS"); every > 0) {
    // YAP_DEBUG_GC=1 also runs the JS garbage collector before each count,
    // to tell garbage not yet collected from objects still referenced.
    const bool collect = qEnvironmentVariableIntValue("YAP_DEBUG_GC") != 0;
    auto* dump = new QTimer(&app);
    QObject::connect(dump, &QTimer::timeout, &app, [&engine, collect] {
      if (collect) {
        engine.collectGarbage();
        engine.trimComponentCache();
      }
      dumpObjectCounts(engine);
    });
    dump->start(every * 1000);
  }

  // YAP_DEBUG_SOAK_TRAY=N (diagnostics, the soak in docs/PERFORMANCE.md):
  // every N seconds the window goes to the tray, and 20 s later it comes
  // back without taking the focus. Shown, it is told it is active (a posted
  // WM_ACTIVATE to itself) so animations play as in front.
  if (const int every = qEnvironmentVariableIntValue("YAP_DEBUG_SOAK_TRAY"); every > 0) {
    auto* cycle = new QTimer(&app);
    QObject::connect(cycle, &QTimer::timeout, &app, [&engine, shell] {
      auto* window = engine.rootObjects().isEmpty()
                       ? nullptr
                       : qobject_cast<QQuickWindow*>(engine.rootObjects().first());
      if (!window || !window->isVisible())
        return;
      shell->hideToTray();
      QTimer::singleShot(20000, window, [window] {
        window->setProperty("_q_showWithoutActivating", true);
        window->show();
#ifdef Q_OS_WIN
        PostMessageW(reinterpret_cast<HWND>(window->winId()), WM_ACTIVATE, WA_ACTIVE, 0);
#endif
        std::fprintf(stderr, "[soak] shown from the tray\n");
      });
    });
    cycle->start(qMax(every, 30) * 1000);
  }

  if (heaptrace::active()) {
    // A dump each time "<prefix>.now" appears: its text is the tag, after
    // "gc " the JS collector runs first. The file is removed once written.
    auto* poll = new QTimer(&app);
    QObject::connect(poll, &QTimer::timeout, &app, [&engine] {
      const QString trigger = qEnvironmentVariable("YAP_DEBUG_HEAP") + QStringLiteral(".now");
      QFile file(trigger);
      if (!file.exists() || !file.open(QIODevice::ReadOnly))
        return;
      QString tag = QString::fromUtf8(file.readAll()).trimmed();
      file.close();
      QFile::remove(trigger);
      if (tag.startsWith(QLatin1String("gc "))) {
        tag = tag.mid(3);
        engine.collectGarbage();
        engine.trimComponentCache();
      }
      heaptrace::dump(qPrintable(tag.isEmpty() ? QStringLiteral("now") : tag));
    });
    poll->start(500);
  }

  const int code = app.exec();
  // The quit ends here, without the engine's teardown. Everything that
  // outlives the process is already done on aboutToQuit (DesktopShell): the
  // window rect, the tray icon, the toasts, and the core (the call left
  // with its beacon, the cache committed). The teardown itself could stall
  // for seconds where nothing could cut it short: the QML image loader's
  // thread waits for its network thread, which releases one Network List
  // Manager COM object per cached HTTP connection (Qt's
  // QNetworkConnectionMonitor), an RPC to the network service each, and a
  // thread inside that RPC holds up even TerminateProcess.
  std::fprintf(stderr, "[quit] event loop ended (%d); exiting\n", code);
  // The crash guard's marker goes with a clean quit (cpp/compat.*).
  Compat::quitting();
#ifdef Q_OS_WIN
  // Install on quit (rust/src/updater.rs): a downloaded update goes to
  // Velopack's Update.exe, which waits for this process to exit.
  yap_updater_quit(GetSystemMetrics(SM_SHUTTINGDOWN) != 0);
  logfile::finish(500);
  TerminateProcess(GetCurrentProcess(), UINT(code));
#endif
  return code;
}
