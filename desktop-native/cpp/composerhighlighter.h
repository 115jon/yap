#pragma once

// The composer's live formatting (InputMentionOverlay.tsx): a bare
// http(s) link reads in accent ink as it is typed or pasted, as it will in
// the sent message, and an @mention takes the accent tint behind accent
// ink. Only colours change, never the font, so every glyph stays where
// the plain text put it. An @ inside a link is part of the link.
//
// QML (import Yap.Media):
//   ComposerHighlighter {
//       document: input.textDocument
//       linkColor: Theme.accentInk
//       mentionColor: Theme.accentInk
//       mentionBackground: Theme.accentTint
//   }

#include <QtCore/QPointer>
#include <QtCore/QRegularExpression>
#include <QtGui/QColor>
#include <QtGui/QSyntaxHighlighter>
#include <QtQuick/QQuickTextDocument>

class ComposerHighlighter : public QObject
{
  Q_OBJECT
  Q_PROPERTY(QQuickTextDocument* document READ document WRITE setDocument
               NOTIFY documentChanged)
  Q_PROPERTY(QColor linkColor READ linkColor WRITE setLinkColor NOTIFY
               colorsChanged)
  Q_PROPERTY(QColor mentionColor READ mentionColor WRITE setMentionColor
               NOTIFY colorsChanged)
  Q_PROPERTY(QColor mentionBackground READ mentionBackground WRITE
               setMentionBackground NOTIFY colorsChanged)

public:
  explicit ComposerHighlighter(QObject* parent = nullptr);

  QQuickTextDocument* document() const { return m_document; }
  void setDocument(QQuickTextDocument* document);
  QColor linkColor() const { return m_linkColor; }
  void setLinkColor(const QColor& color);
  QColor mentionColor() const { return m_mentionColor; }
  void setMentionColor(const QColor& color);
  QColor mentionBackground() const { return m_mentionBackground; }
  void setMentionBackground(const QColor& color);

signals:
  void documentChanged();
  void colorsChanged();

private:
  class Highlighter;
  void rehighlight();

  QPointer<QQuickTextDocument> m_document;
  Highlighter* m_highlighter = nullptr;
  QColor m_linkColor;
  QColor m_mentionColor;
  QColor m_mentionBackground;
};
