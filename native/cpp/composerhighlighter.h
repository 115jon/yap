#pragma once

// The composer's live formatting (InputMentionOverlay.tsx): a bare
// http(s) link reads in accent ink as it is typed or pasted, as it will in
// the sent message, and an @mention takes the accent tint behind accent
// ink. Only colours change, never the font, so every glyph stays where
// the plain text put it. An @ inside a link is part of the link.
//
// Emoji are drawn as pictures over the text (an overlay in QML, as the
// web's is): `hidden` lists the [start, end) ranges whose glyphs go
// transparent under them, and `fixed` the positions of picked server emoji
// placeholders (one private-use character each), which also widen to the
// web's 1.35em box so the picture has its room.
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
#include <QtCore/QVariantList>
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
  Q_PROPERTY(QVariantList hidden READ hidden WRITE setHidden NOTIFY
               rangesChanged)
  Q_PROPERTY(QVariantList fixed READ fixed WRITE setFixed NOTIFY
               rangesChanged)

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
  QVariantList hidden() const { return m_hidden; }
  void setHidden(const QVariantList& ranges);
  QVariantList fixed() const { return m_fixed; }
  void setFixed(const QVariantList& positions);

signals:
  void documentChanged();
  void colorsChanged();
  void rangesChanged();

private:
  class Highlighter;
  void rehighlight();

  QPointer<QQuickTextDocument> m_document;
  Highlighter* m_highlighter = nullptr;
  QColor m_linkColor;
  QColor m_mentionColor;
  QColor m_mentionBackground;
  QVariantList m_hidden;
  QVariantList m_fixed;
};
