#include "eyedropper.h"

#include <QtCore/QTimer>
#include <QtGui/QCursor>
#include <QtGui/QFont>
#include <QtGui/QGuiApplication>
#include <QtGui/QKeyEvent>
#include <QtGui/QMouseEvent>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtGui/QPixmap>
#include <QtGui/QRasterWindow>
#include <QtGui/QScreen>

#include <cmath>
#include <cstdio>

#ifdef Q_OS_WIN
#include <windows.h>
#ifndef WDA_EXCLUDEFROMCAPTURE
#define WDA_EXCLUDEFROMCAPTURE 0x00000011
#endif
#endif

namespace {

// The grab: this many logical pixels each side of the pointer.
constexpr int kRadius = 7;
// The loupe: kCells x kCells device pixels around the pointer, each drawn
// kCell logical pixels wide, in a circle with a ring of the colour.
constexpr int kCells = 11;
constexpr int kCell = 9;
constexpr int kRing = 4;

} // namespace

// One screen's overlay: a frameless, topmost, all but transparent window
// (alpha 1, so it still takes the clicks) that draws the loupe while the
// pointer is over its screen.
class EyedropperOverlay : public QRasterWindow
{
public:
  EyedropperOverlay(Eyedropper* owner, QScreen* screen)
    : m_owner(owner)
  {
    setFlags(Qt::FramelessWindowHint | Qt::WindowStaysOnTopHint | Qt::Tool |
             Qt::NoDropShadowWindowHint);
    QSurfaceFormat format = this->format();
    format.setAlphaBufferSize(8);
    setFormat(format);
    setScreen(screen);
    setGeometry(screen->geometry());
    setTitle(QStringLiteral("Yap eyedropper"));
  }

  // Shows it, kept out of every capture (our own grab included).
  void open()
  {
    create();
#ifdef Q_OS_WIN
    m_excluded =
      SetWindowDisplayAffinity(reinterpret_cast<HWND>(winId()), WDA_EXCLUDEFROMCAPTURE) != 0;
#endif
    // Over the pointer when the grab cannot see the loupe; beside it
    // otherwise, out of the grabbed square.
    setCursor(m_excluded ? Qt::BlankCursor : Qt::CrossCursor);
    show();
  }

  QColor color() const { return m_color; }
  bool sampled() const { return m_shown; }

  // Reads the square at `global` (logical, Qt's virtual desktop) and
  // redraws the loupe there.
  void sampleAt(const QPointF& global)
  {
    QScreen* s = screen();
    if (!s)
      return;
    const QRect geo = s->geometry();
    const QPointF local = global - QPointF(geo.topLeft());
    const QPoint at(int(std::floor(local.x())), int(std::floor(local.y())));
    const QRect area =
      QRect(at.x() - kRadius, at.y() - kRadius, 2 * kRadius + 1, 2 * kRadius + 1) &
      QRect(QPoint(0, 0), geo.size());
    if (area.isEmpty())
      return;
    const QImage image = s->grabWindow(0, area.x(), area.y(), area.width(), area.height())
                           .toImage()
                           .convertToFormat(QImage::Format_RGB32);
    if (image.isNull())
      return;
    // Device pixels per logical one, as the grab came back.
    const qreal dpr = qreal(image.width()) / area.width();
    const int cx = qBound(0, int(std::floor((local.x() - area.x()) * dpr)), image.width() - 1);
    const int cy = qBound(0, int(std::floor((local.y() - area.y()) * dpr)), image.height() - 1);
    m_color = image.pixelColor(cx, cy);
    m_cells = QImage(kCells, kCells, QImage::Format_ARGB32);
    m_cells.fill(Qt::transparent);
    for (int y = 0; y < kCells; ++y) {
      for (int x = 0; x < kCells; ++x) {
        const int px = cx + x - kCells / 2;
        const int py = cy + y - kCells / 2;
        if (px >= 0 && py >= 0 && px < image.width() && py < image.height())
          m_cells.setPixel(x, y, image.pixel(px, py));
      }
    }
    m_local = local;
    m_shown = true;
    update();
  }

  void hideLoupe()
  {
    if (!m_shown)
      return;
    m_shown = false;
    update();
  }

protected:
  void paintEvent(QPaintEvent*) override
  {
    QPainter p(this);
    p.setCompositionMode(QPainter::CompositionMode_Source);
    p.fillRect(QRect(QPoint(0, 0), size()), QColor(0, 0, 0, 1));
    p.setCompositionMode(QPainter::CompositionMode_SourceOver);
    if (!m_shown)
      return;
    p.setRenderHint(QPainter::Antialiasing);
    const int grid = kCells * kCell;
    const int outer = grid + 2 * kRing;
    // Centred on the pointer, or below and right of the grabbed square
    // (flipped at the screen's far edges).
    QPointF centre = m_local;
    if (!m_excluded) {
      const qreal off = kRadius + 6 + outer / 2.0;
      centre += QPointF(m_local.x() + off + outer / 2.0 > width() ? -off : off,
                        m_local.y() + off + outer / 2.0 + 28 > height() ? -off : off);
    }
    const QRectF ring(centre.x() - outer / 2.0, centre.y() - outer / 2.0, outer, outer);
    const QRectF cells = ring.adjusted(kRing, kRing, -kRing, -kRing);
    // The ring in the colour, dark and light hairlines either side.
    p.setPen(Qt::NoPen);
    p.setBrush(QColor(0, 0, 0, 90));
    p.drawEllipse(ring.adjusted(-1, -1, 1, 1));
    p.setBrush(m_color);
    p.drawEllipse(ring);
    {
      QPainterPath clip;
      clip.addEllipse(cells);
      p.save();
      p.setClipPath(clip);
      p.fillRect(cells, QColor(128, 128, 128));
      p.setRenderHint(QPainter::SmoothPixmapTransform, false);
      p.drawImage(cells, m_cells);
      // The pixel picked: a white square with a dark edge.
      const QRectF mid(cells.x() + (kCells / 2) * kCell, cells.y() + (kCells / 2) * kCell, kCell,
                       kCell);
      p.setRenderHint(QPainter::Antialiasing, false);
      p.setBrush(Qt::NoBrush);
      p.setPen(QPen(QColor(0, 0, 0, 200), 1));
      p.drawRect(mid.adjusted(-1, -1, 0, 0));
      p.setPen(QPen(QColor(255, 255, 255), 1));
      p.drawRect(mid.adjusted(0, 0, -1, -1));
      p.restore();
    }
    p.setBrush(Qt::NoBrush);
    p.setPen(QPen(QColor(255, 255, 255, 220), 1));
    p.drawEllipse(cells);
    // The hex under it.
    QFont font(QStringLiteral("Yap Geist Mono"));
    font.setPixelSize(12);
    p.setFont(font);
    const QString text = Eyedropper::hex(m_color);
    const QRectF label(centre.x() - 40, ring.bottom() + 6, 80, 20);
    p.setPen(Qt::NoPen);
    p.setBrush(QColor(17, 17, 17, 230));
    p.drawRoundedRect(label, 4, 4);
    p.setPen(QColor(245, 245, 245));
    p.drawText(label, Qt::AlignCenter, text);
  }

  void mouseMoveEvent(QMouseEvent* event) override
  {
    m_owner->moved(this);
    sampleAt(event->globalPosition());
  }

  void mousePressEvent(QMouseEvent* event) override
  {
    if (event->button() != Qt::LeftButton) {
      m_owner->cancel();
      return;
    }
    sampleAt(event->globalPosition());
    if (m_shown)
      m_owner->finish(Eyedropper::hex(m_color));
    else
      m_owner->cancel();
  }

  void keyPressEvent(QKeyEvent* event) override
  {
    switch (event->key()) {
    case Qt::Key_Escape:
      m_owner->cancel();
      break;
    case Qt::Key_Return:
    case Qt::Key_Enter:
    case Qt::Key_Space:
      if (m_shown)
        m_owner->finish(Eyedropper::hex(m_color));
      break;
    default:
      break;
    }
  }

private:
  Eyedropper* m_owner;
  QImage m_cells;
  QPointF m_local;
  QColor m_color;
  bool m_shown = false;
  bool m_excluded = false;
};

Eyedropper::Eyedropper(QObject* parent)
  : QObject(parent)
{
}

Eyedropper::~Eyedropper()
{
  qDeleteAll(m_overlays);
}

QString Eyedropper::hex(const QColor& color)
{
  return QStringLiteral("#%1%2%3")
    .arg(color.red(), 2, 16, QLatin1Char('0'))
    .arg(color.green(), 2, 16, QLatin1Char('0'))
    .arg(color.blue(), 2, 16, QLatin1Char('0'))
    .toUpper();
}

void Eyedropper::start()
{
  if (m_active)
    return;
  m_active = true;
  emit activeChanged();
  const QString test = qEnvironmentVariable("YAP_TEST_EYEDROPPER");
  if (!test.isEmpty()) {
    // Tests: never a window, never a read of the screen.
    QTimer::singleShot(150, this, [this, test] {
      if (!m_active)
        return;
      const QColor color = QColor::fromString(test.trimmed());
      if (test == QLatin1String("-") || !color.isValid()) {
        std::fprintf(stderr, "[eyedropper] test cancel\n");
        cancel();
      } else {
        std::fprintf(stderr, "[eyedropper] test pick %s\n", qPrintable(hex(color)));
        finish(hex(color));
      }
      std::fflush(stderr);
    });
    return;
  }
  m_previous = QGuiApplication::focusWindow();
  const QPoint pointer = QCursor::pos();
  QScreen* under = QGuiApplication::screenAt(pointer);
  EyedropperOverlay* first = nullptr;
  for (QScreen* screen : QGuiApplication::screens()) {
    auto* overlay = new EyedropperOverlay(this, screen);
    m_overlays << overlay;
    overlay->open();
    if (screen == under)
      first = overlay;
  }
  if (!first && !m_overlays.isEmpty())
    first = m_overlays.first();
  if (first) {
    first->requestActivate();
    if (under)
      first->sampleAt(pointer);
  }
  // Switching away from the app ends it, as Chromium's does.
  connect(qApp, &QGuiApplication::applicationStateChanged, this,
          [this](Qt::ApplicationState state) {
            if (state != Qt::ApplicationActive)
              cancel();
          });
}

void Eyedropper::cancel()
{
  if (!m_active)
    return;
  close();
  emit cancelled();
}

void Eyedropper::moved(EyedropperOverlay* from)
{
  for (EyedropperOverlay* overlay : std::as_const(m_overlays)) {
    if (overlay != from)
      overlay->hideLoupe();
  }
}

void Eyedropper::finish(const QString& hex)
{
  if (!m_active)
    return;
  close();
  emit picked(hex);
}

void Eyedropper::close()
{
  disconnect(qApp, &QGuiApplication::applicationStateChanged, this, nullptr);
  // From inside an overlay's own event handler: gone once it returns.
  for (EyedropperOverlay* overlay : std::as_const(m_overlays)) {
    overlay->hide();
    overlay->deleteLater();
  }
  m_overlays.clear();
  m_active = false;
  emit activeChanged();
  if (m_previous)
    m_previous->requestActivate();
  m_previous = nullptr;
}
