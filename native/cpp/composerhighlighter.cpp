#include "composerhighlighter.h"

#include <QtGui/QFontMetricsF>
#include <QtGui/QTextCharFormat>
#include <QtGui/QTextDocument>

// InputMentionOverlay.tsx's pattern, minus the emoji it draws as images:
// a link (to the first space or <>"'`)]), else an @mention.
static const QRegularExpression& pattern()
{
  static const QRegularExpression re(
    QStringLiteral(R"((https?://[^\s<>"'`)\]]+)|@([a-zA-Z0-9_]+))"),
    QRegularExpression::CaseInsensitiveOption);
  return re;
}

// `isInsideUrl` (lib/emoji.ts): the whitespace-separated word holding
// `index` starts with a link, as tiktok.com/@user does after a `)`.
static bool insideUrl(const QString& text, qsizetype index)
{
  qsizetype start = index;
  while (start > 0 && !text.at(start - 1).isSpace())
    --start;
  return text.mid(start, 8).startsWith(QLatin1String("https://"),
                                       Qt::CaseInsensitive) ||
         text.mid(start, 7).startsWith(QLatin1String("http://"),
                                       Qt::CaseInsensitive);
}

class ComposerHighlighter::Highlighter : public QSyntaxHighlighter
{
public:
  Highlighter(ComposerHighlighter* owner, QTextDocument* document)
    : QSyntaxHighlighter(document)
    , m_owner(owner)
  {
  }

protected:
  void highlightBlock(const QString& text) override
  {
    QTextCharFormat link;
    link.setForeground(m_owner->linkColor());
    QTextCharFormat mention;
    mention.setForeground(m_owner->mentionColor());
    mention.setBackground(m_owner->mentionBackground());
    auto it = pattern().globalMatch(text);
    while (it.hasNext()) {
      const auto match = it.next();
      if (match.capturedStart(1) >= 0)
        setFormat(int(match.capturedStart(1)), int(match.capturedLength(1)),
                  link);
      else if (!insideUrl(text, match.capturedStart(0)))
        setFormat(int(match.capturedStart(0)), int(match.capturedLength(0)),
                  mention);
    }
    // Under the pictures: no glyph (the caret and the selection stay).
    const int from = currentBlock().position();
    const int to = from + int(text.size());
    QTextCharFormat clear;
    clear.setForeground(Qt::transparent);
    for (const QVariant& range : m_owner->hidden()) {
      const QVariantList pair = range.toList();
      if (pair.size() != 2)
        continue;
      const int start = qMax(from, pair[0].toInt());
      const int end = qMin(to, pair[1].toInt());
      if (end > start)
        setFormat(start - from, end - start, clear);
    }
    // A placeholder widens to 1.35em (the web's fixed emoji box).
    const QFont font = document()->defaultFont();
    const qreal box = font.pixelSize() > 0 ? font.pixelSize() * 1.35
                                           : QFontMetricsF(font).height() * 1.1;
    for (const QVariant& at : m_owner->fixed()) {
      const int pos = at.toInt();
      if (pos < from || pos >= to)
        continue;
      QTextCharFormat wide = clear;
      const qreal advance = QFontMetricsF(font).horizontalAdvance(text.at(pos - from));
      wide.setFontLetterSpacingType(QFont::AbsoluteSpacing);
      wide.setFontLetterSpacing(box - advance);
      setFormat(pos - from, 1, wide);
    }
  }

private:
  ComposerHighlighter* m_owner;
};

ComposerHighlighter::ComposerHighlighter(QObject* parent)
  : QObject(parent)
{
}

void ComposerHighlighter::setDocument(QQuickTextDocument* document)
{
  if (m_document == document)
    return;
  m_document = document;
  delete m_highlighter;
  m_highlighter = nullptr;
  if (document && document->textDocument())
    m_highlighter = new Highlighter(this, document->textDocument());
  emit documentChanged();
}

void ComposerHighlighter::setLinkColor(const QColor& color)
{
  if (m_linkColor == color)
    return;
  m_linkColor = color;
  rehighlight();
  emit colorsChanged();
}

void ComposerHighlighter::setMentionColor(const QColor& color)
{
  if (m_mentionColor == color)
    return;
  m_mentionColor = color;
  rehighlight();
  emit colorsChanged();
}

void ComposerHighlighter::setMentionBackground(const QColor& color)
{
  if (m_mentionBackground == color)
    return;
  m_mentionBackground = color;
  rehighlight();
  emit colorsChanged();
}

void ComposerHighlighter::setHidden(const QVariantList& ranges)
{
  if (m_hidden == ranges)
    return;
  m_hidden = ranges;
  rehighlight();
  emit rangesChanged();
}

void ComposerHighlighter::setFixed(const QVariantList& positions)
{
  if (m_fixed == positions)
    return;
  m_fixed = positions;
  rehighlight();
  emit rangesChanged();
}

void ComposerHighlighter::rehighlight()
{
  if (m_highlighter)
    m_highlighter->rehighlight();
}
