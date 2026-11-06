#include "composerhighlighter.h"

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

void ComposerHighlighter::rehighlight()
{
  if (m_highlighter)
    m_highlighter->rehighlight();
}
