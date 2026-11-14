#include "ditherprovider.h"

#include <QtCore/QMutexLocker>
#include <QtGui/QColor>

#include <algorithm>
#include <cmath>
#include <functional>
#include <initializer_list>
#include <vector>

namespace {

using Cell = std::function<bool(int, int)>;

constexpr int kBayer4[4][4] = {
  { 0, 8, 2, 10 },
  { 12, 4, 14, 6 },
  { 3, 11, 1, 9 },
  { 15, 7, 13, 5 },
};

constexpr int kBayer8[8][8] = {
  { 0, 32, 8, 40, 2, 34, 10, 42 },  { 48, 16, 56, 24, 50, 18, 58, 26 },
  { 12, 44, 4, 36, 14, 46, 6, 38 }, { 60, 28, 52, 20, 62, 30, 54, 22 },
  { 3, 35, 11, 43, 1, 33, 9, 41 },  { 51, 19, 59, 27, 49, 17, 57, 25 },
  { 15, 47, 7, 39, 13, 45, 5, 37 }, { 63, 31, 55, 23, 61, 29, 53, 21 },
};

// ui/dither.tsx bayerLit
bool bayerLit(int x, int y, double coverage)
{
  return coverage * 64 > kBayer8[y % 8][x % 8] + 0.5;
}

Cell rect(int x0, int y0, int w, int h)
{
  return [=](int x, int y) {
    return x >= x0 && x < x0 + w && y >= y0 && y < y0 + h;
  };
}

QColor color(const QString& hex)
{
  return QColor::fromString(QLatin1Char('#') + hex);
}

QImage canvas(int w, int h)
{
  QImage image(w, h, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  return image;
}

void paint(QImage& image, const Cell& cell, const QColor& ink)
{
  const QRgb rgb = ink.rgba();
  for (int y = 0; y < image.height(); ++y)
    for (int x = 0; x < image.width(); ++x)
      if (cell(x, y))
        image.setPixel(x, y, qPremultiply(rgb));
}

// ui/yap-bark.tsx: the master mark on a 32-cell grid.
QImage bark(const QString& frame, const QColor& ink)
{
  constexpr int kCells = 32;
  const auto ear = [](int x, int y) { return x >= 2 && y < 10 && x < y + 2; };
  const auto innerEar = [](int x, int y) {
    return x >= 4 && y >= 5 && y < 9 && x < y - 1;
  };
  const Cell skull = rect(2, 10, 19, 14);
  const Cell snout = rect(21, 12, 11, 7);
  const Cell neck = rect(4, 24, 14, 8);
  const Cell eye = rect(13, 13, 4, 4);
  const Cell nose = rect(29, 12, 3, 3);
  const Cell jaw = frame == QLatin1String("closed") ? rect(17, 19, 13, 4)
                   : frame == QLatin1String("half") ? rect(17, 20, 12, 4)
                                                    : rect(17, 22, 11, 4);
  const bool ticks = frame == QLatin1String("open");
  const auto cutout = [&](int x, int y) {
    return innerEar(x, y) || eye(x, y) || nose(x, y);
  };
  const auto light = [](int x, int y) {
    return 0.98 - (x + y) / (2.0 * (kCells - 1)) * 0.3;
  };
  const auto lit = [&](int x, int y) {
    const bool body = ear(x, y) || skull(x, y) || snout(x, y) || neck(x, y);
    return (body || jaw(x, y)) && !cutout(x, y) && bayerLit(x, y, light(x, y));
  };
  constexpr int kTicks[6][2] = { { 29, 10 }, { 30, 9 },  { 31, 8 },
                                 { 29, 28 }, { 30, 29 }, { 31, 30 } };
  QImage image = canvas(kCells, kCells);
  paint(
    image,
    [&](int x, int y) {
      if (ticks)
        for (const auto& t : kTicks)
          if (t[0] == x && t[1] == y)
            return true;
      return lit(x, y);
    },
    ink);
  return image;
}

// The too-big poses (toobig-spec.md, the mockup's cell geometry): Yappy's
// parts on the mark's grid, shifted onto a canvas, printed through the
// mockup's accent dither (15 of 16 cells lit); the file, its fold and lines, the strain or sweat
// ticks and the tear drawn solid over it. Every coordinate is the mark's
// (unshifted) grid; `ox`, `oy` place it on the canvas.
struct PoseInks
{
  QColor accent, warn, tear, paper, fold, lines;
};

QImage pose(const QString& name, const PoseInks& ink)
{
  const auto ear = [](int x, int y) { return x >= 2 && y < 10 && x < y + 2; };
  const auto innerEar = [](int x, int y) {
    return x >= 4 && y >= 5 && y < 9 && x < y - 1;
  };
  const Cell skull = rect(2, 10, 19, 14);
  const Cell snout = rect(21, 12, 11, 7);
  const Cell neck = rect(4, 24, 14, 8);
  const Cell eye = rect(13, 13, 4, 4);
  const Cell nose = rect(29, 12, 3, 3);
  const auto any = [](std::initializer_list<Cell> cells) {
    std::vector<Cell> list(cells);
    return Cell([list](int x, int y) {
      for (const Cell& c : list)
        if (c(x, y))
          return true;
      return false;
    });
  };
  int cols = 34, rows = 34, ox = 1, oy = 0;
  Cell body, cut, paper, fold, lines, warn, tear;
  const Cell none = [](int, int) { return false; };
  paper = fold = lines = warn = tear = none;
  if (name == QLatin1String("stuffed")) {
    // A: the jaw wide open on a file that sticks out past the snout, and
    // three strain ticks over the head.
    cols = 40;
    ox = 0;
    oy = 2;
    body = any({ ear, skull, snout, neck, rect(17, 22, 11, 4) });
    cut = any({ innerEar, eye, nose });
    fold = [](int x, int y) {
      return x >= 32 && x < 36 && y >= 17 && y < 21 && (x - 32) <= (y - 17);
    };
    lines = any({ rect(26, 20, 7, 1), rect(26, 22, 8, 1) });
    const Cell sheet = rect(24, 17, 12, 9);
    paper = [=](int x, int y) { return sheet(x, y) && !fold(x, y) && !lines(x, y); };
    warn = any({ rect(10, 1, 1, 3), rect(13, 0, 1, 3), rect(16, 1, 1, 3) });
  } else if (name == QLatin1String("sad")) {
    // B: the ear drooped back and down, the eye half shut, a one-pixel
    // tear, the jaw closed.
    const auto droopEar = [](int x, int y) {
      return y >= 10 && y < 16 && x >= 0 && x < 6 && (y - 10) <= (x + 1);
    };
    body = any({ droopEar, skull, snout, neck, rect(17, 19, 13, 4) });
    cut = any({ rect(13, 15, 4, 2), nose });
    tear = any({ rect(14, 18, 1, 1), rect(14, 20, 1, 2) });
  } else {
    // C: the head flattened under a file page, with sweat ticks.
    body = any({ rect(2, 18, 19, 9), rect(21, 20, 11, 5), rect(17, 25, 13, 2),
                 rect(4, 27, 14, 5) });
    cut = any({ rect(13, 20, 4, 2), rect(29, 20, 3, 2) });
    fold = [](int x, int y) {
      return x >= 18 && x < 23 && y >= 3 && y < 8 && (x - 18) <= (y - 3);
    };
    lines = any({ rect(6, 8, 10, 1), rect(6, 10, 13, 1), rect(6, 12, 13, 1),
                  rect(6, 14, 9, 1) });
    const Cell block = rect(3, 3, 20, 14);
    paper = [=](int x, int y) { return block(x, y) && !fold(x, y) && !lines(x, y); };
    warn = any({ rect(0, 14, 2, 1), rect(24, 14, 2, 1), rect(1, 12, 1, 1),
                 rect(25, 12, 1, 1) });
  }
  const auto at = [&](const Cell& cell) {
    return [&, cell](int x, int y) { return cell(x - ox, y - oy); };
  };
  QImage image = canvas(cols, rows);
  paint(
    image,
    [&](int x, int y) {
      // The approved mockup's print: a 4x4 Bayer tile with only its last
      // cell dark (15 of 16 lit), on the canvas grid.
      const int gx = x - ox, gy = y - oy;
      return body(gx, gy) && !cut(gx, gy) && kBayer4[y % 4][x % 4] < 15;
    },
    ink.accent);
  paint(image, at(paper), ink.paper);
  paint(image, at(fold), ink.fold);
  paint(image, at(lines), ink.lines);
  paint(image, at(warn), ink.warn);
  paint(image, at(tear), ink.tear);
  return image;
}

// lib/badge-art.ts: one 16 x 16 drawing per badge. "#" body, lit through
// the Bayer dither (dense top left, thinning toward the bottom right);
// "@" edge, always lit; "+" detail in the second ink; "." empty.
struct BadgeArt
{
  const char* id;
  const char* rows[16];
};

constexpr BadgeArt kBadgeArt[] = {
  { "founder",
    { "................", "................", ".......++.......", ".@.....++.....@.",
      ".@@....@@....@@.", ".@#@..@##@..@#@.", ".@##@@####@@##@.", ".@############@.",
      ".@############@.", ".@############@.", ".@############@.", ".@@@@@@@@@@@@@@.",
      ".@@+@@@++@@@+@@.", ".@@@@@@@@@@@@@@.", "................", "................" } },
  { "loud",
    { "................", "...........+....", "......@@....+...", ".....@#@.+...+..",
      "....@##@..+...+.", "@@@@###@..+...+.", "@######@...+...+", "@######@...+...+",
      "@######@...+...+", "@######@...+...+", "@@@@###@..+...+.", "....@##@..+...+.",
      ".....@#@.+...+..", "......@@....+...", "...........+....", "................" } },
  { "yap-team",
    { "................", ".@..............", ".@@.............", ".@.@............",
      ".@@#@...........", ".@###@@@@@@.....", ".@#####@###@@@@.", ".@####@.@######@",
      ".@#####@#######@", ".@#########@@@@@", ".@########@.....", ".@#########@@@..",
      "..@######@@@@@..", "..@#####@.......", "..@#####@.......", "..@@@@@@@......." } },
  { "bug-hunter",
    { "................", "....+......+....", ".....+....+.....", "......@@@@......",
      ".....@@@@@@.....", "+...@##@@##@...+", ".+.@###@@###@.+.", "..+@###@@###@+..",
      "...@###@@###@...", "+++@###@@###@+++", "...@###@@###@...", "..+@###@@###@+..",
      ".+..@##@@##@..+.", "+....@@@@@@....+", "................", "................" } },
  { "early-yapper",
    { "................", "................", ".......++.......", "..+....++....+..",
      "...+........+...", "......@@@@......", "....@@####@@....", "...@########@...",
      "..@##########@..", "..@##########@..", ".@############@.", "++++++++++++++++",
      "................", "...++++++++++...", "................", "......++++......" } },
  { "server-owner",
    { "................", "................", ".@@..@@..@@..@@.", ".@#@@#@@@@#@@#@.",
      ".@############@.", ".@############@.", ".@###++##++###@.", ".@###++##++###@.",
      ".@############@.", ".@#####++#####@.", ".@####++++####@.", ".@####++++####@.",
      ".@####++++####@.", ".@@@@@@@@@@@@@@.", "................", "................" } },
  { "voice-regular",
    { "................", "......@@@@......", ".....@####@.....", ".....@#++#@.....",
      ".....@####@.....", ".....@#++#@.....", ".....@####@.....", "...+.@####@.+...",
      "...+.@####@.+...", "...+..@@@@..+...", "....+......+....", ".....++++++.....",
      ".......@@.......", ".......@@.......", ".....@@@@@@.....", "................" } },
  { "night-owl",
    { "................", "...........+....", ".....@@...+++...", "...@@@.....+....",
      "..@@#@..........", "..@#@.......+...", ".@##@......+++..", ".@###@......+...",
      ".@###@..........", ".@###@@.........", ".@####@@........", "..@####@@@..@...",
      "..@@######@@@...", "...@@#####@@....", ".....@@@@@......", "................" } },
};

// user-badges.tsx badgeMasks: the lit cells (edge, and body where the
// light's Bayer cell is on) in `lit`, details in `detail`. An unknown id
// is an empty 16 x 16.
QImage badge(const QString& id, const QColor& lit, const QColor& detail)
{
  constexpr int kCells = 16;
  QImage image = canvas(kCells, kCells);
  const BadgeArt* art = nullptr;
  for (const BadgeArt& a : kBadgeArt)
    if (id == QLatin1String(a.id))
      art = &a;
  if (!art)
    return image;
  const auto cell = [&](int x, int y) { return art->rows[y][x]; };
  // badgeLight: 0.98 at the top left down to 0.64 at the bottom right.
  const auto light = [](int x, int y) {
    return 0.98 - (x + y) / (2.0 * (kCells - 1)) * 0.34;
  };
  paint(
    image,
    [&](int x, int y) {
      const char c = cell(x, y);
      return c == '@' || (c == '#' && bayerLit(x, y, light(x, y)));
    },
    lit);
  paint(image, [&](int x, int y) { return cell(x, y) == '+'; }, detail);
  return image;
}

// ui/dither-glyph.tsx VOICE: seven two-cell bars with peak-hold cells.
QImage voice(const QColor& frameInk, const QColor& lightInk)
{
  constexpr int kBars[7] = { 5, 9, 13, 16, 11, 7, 4 };
  const auto bar = [&](int x) {
    const int i = (x - 1) / 4;
    return x >= 1 && (x - 1) % 4 < 2 && i < 7 ? i : -1;
  };
  QImage image = canvas(27, 20);
  paint(
    image,
    [&](int x, int y) {
      const int i = bar(x);
      const double top = 18 - (i >= 0 ? kBars[i] : 0);
      if (i < 0 || y > 17 || y < top)
        return false;
      return bayerLit(x, y, 0.98 - ((17 - y) / 17.0) * 0.5);
    },
    lightInk);
  paint(
    image,
    [&](int x, int y) {
      if (y == 19)
        return true;
      const int i = bar(x);
      return i >= 0 && y == 17 - kBars[i] - 1;
    },
    frameInk);
  return image;
}

// ui/dither-glyph.tsx EMPTY: a dashed slot (solid corners, runs two on,
// one off) with light pooled in its lower-left corner.
QImage emptySlot(const QColor& frameInk, const QColor& lightInk)
{
  QImage image = canvas(26, 20);
  paint(
    image,
    [&](int x, int y) {
      if (x < 2 || x > 23 || y < 2 || y > 17)
        return false;
      const double d = std::hypot((x - 2) / 21.0, (17 - y) / 15.0) / std::sqrt(2.0);
      return bayerLit(x, y, std::pow(std::clamp(1 - d * 1.35, 0.0, 1.0), 1.4) * 0.95);
    },
    lightInk);
  paint(
    image,
    [&](int x, int y) {
      const bool edgeX = x == 0 || x == 25;
      const bool edgeY = y == 0 || y == 19;
      if (!edgeX && !edgeY)
        return false;
      if (edgeX && edgeY)
        return true;
      return (edgeY ? x : y) % 3 != 0;
    },
    frameInk);
  return image;
}

// ui/dither.tsx bayerTileMask, repeated over w x h cells.
QImage dots(int coveragePercent, const QColor& ink, int w, int h)
{
  const int key = static_cast<int>(std::lround(coveragePercent / 100.0 * 16));
  QImage image = canvas(std::max(1, w), std::max(1, h));
  paint(
    image, [&](int x, int y) { return key > kBayer4[y % 4][x % 4]; }, ink);
  return image;
}

// ui/dither.tsx bayerRampMask: coverage eased from `from` at the leading
// edge to `to` at the trailing one, lifted mid-height by `lift`.
QImage ramp(double from, double to, double lift, const QColor& ink, int cols,
            int rows)
{
  cols = std::max(1, cols);
  rows = std::max(1, rows);
  QImage image = canvas(cols, rows);
  paint(
    image,
    [&](int x, int y) {
      const double v = rows > 1 ? y / double(rows - 1) : 0.5;
      const double mid = 1 - std::abs(v - 0.5) * 2;
      const double u = cols > 1 ? x / double(cols - 1) : 0;
      const double run = from + (to - from) * (1 - std::pow(1 - u, 1.6));
      const double cov = std::clamp(run + lift * mid * run, 0.0, 1.0);
      return bayerLit(x, y, cov);
    },
    ink);
  return image;
}

} // namespace

DitherProvider::DitherProvider()
  : QQuickImageProvider(QQuickImageProvider::Image)
{
}

QImage DitherProvider::requestImage(const QString& id, QSize* size,
                                    const QSize& /*requestedSize*/)
{
  const QStringList parts = id.split(QLatin1Char('/'));
  const QString kind = parts.value(0);
  QImage image;
  if (kind == QLatin1String("badge")) {
    QMutexLocker locker(&badgesLock_);
    auto found = badges_.constFind(id);
    if (found == badges_.constEnd())
      found = badges_.insert(
        id, badge(parts.value(1), color(parts.value(2)), color(parts.value(3))));
    image = *found;
  } else if (kind == QLatin1String("bark"))
    image = bark(parts.value(1), color(parts.value(2)));
  else if (kind == QLatin1String("pose"))
    image = pose(parts.value(1),
                 { color(parts.value(2)), color(parts.value(3)), color(parts.value(4)),
                   color(parts.value(5)), color(parts.value(6)), color(parts.value(7)) });
  else if (kind == QLatin1String("voice"))
    image = voice(color(parts.value(1)), color(parts.value(2)));
  else if (kind == QLatin1String("empty"))
    image = emptySlot(color(parts.value(1)), color(parts.value(2)));
  else if (kind == QLatin1String("dots"))
    image = dots(parts.value(1).toInt(), color(parts.value(2)),
                 parts.value(3).toInt(), parts.value(4).toInt());
  else if (kind == QLatin1String("ramp"))
    image = ramp(parts.value(1).toInt() / 100.0, parts.value(2).toInt() / 100.0,
                 parts.value(3).toInt() / 100.0, color(parts.value(4)),
                 parts.value(5).toInt(), parts.value(6).toInt());
  else
    image = canvas(1, 1);
  if (size)
    *size = image.size();
  return image;
}
