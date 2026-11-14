// The small Markdown subset the repository's policy documents use
// (PRIVACY.md), read into blocks the /privacy page renders as React
// elements. No HTML passes through: the source is one of our own files, and
// anything outside the subset stays plain text.
//
// Blocks: `#`, `##` and `###` headings, paragraphs (lines joined until a
// blank line), and `- ` lists whose items continue on lines indented by two
// spaces. Inline: `code`, <https://...> and <name@host> autolinks,
// [text](https://...) links and **bold**.

export type PolicyInline =
  | { kind: "text"; text: string }
  | { kind: "code"; text: string }
  | { kind: "strong"; text: string }
  | { kind: "link"; text: string; href: string };

export type PolicyBlock =
  | { kind: "heading"; level: 1 | 2 | 3; text: string; id: string }
  | { kind: "paragraph"; inlines: PolicyInline[] }
  | { kind: "list"; items: PolicyInline[][] };

/** A heading's anchor: lowercase words joined by hyphens ("Deleting your account" -> "deleting-your-account"). */
export function policyHeadingId(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

const INLINE =
  /`([^`]+)`|<((?:https?:\/\/|mailto:)[^>\s]+|[^@<>\s]+@[^@<>\s]+\.[^@<>\s]+)>|\[([^\]]+)\]\(([^)\s]+)\)|\*\*([^*]+)\*\*/g;

/** Only web, mail and same-site links become anchors. */
function safeHref(href: string): string | null {
  // A path, but not `//host` or `/\host` (another host to a browser).
  if (/^(?:https:\/\/|mailto:|\/(?![/\\])|#)/i.test(href)) return href;
  return null;
}

export function parsePolicyInline(source: string): PolicyInline[] {
  const out: PolicyInline[] = [];
  const pushText = (text: string) => {
    if (!text) return;
    const last = out[out.length - 1];
    if (last?.kind === "text") last.text += text;
    else out.push({ kind: "text", text });
  };
  let at = 0;
  for (const match of source.matchAll(INLINE)) {
    pushText(source.slice(at, match.index));
    at = match.index + match[0].length;
    const [whole, code, auto, label, target, strong] = match;
    if (code !== undefined) {
      out.push({ kind: "code", text: code });
    } else if (auto !== undefined) {
      const href =
        auto.includes("://") || auto.startsWith("mailto:")
          ? safeHref(auto)
          : `mailto:${auto}`;
      const text = auto.replace(/^mailto:/, "");
      if (href) out.push({ kind: "link", text, href });
      else pushText(whole);
    } else if (label !== undefined && target !== undefined) {
      const href = safeHref(target);
      if (href) out.push({ kind: "link", text: label, href });
      else pushText(label);
    } else if (strong !== undefined) {
      out.push({ kind: "strong", text: strong });
    }
  }
  pushText(source.slice(at));
  return out;
}

export function parsePolicy(source: string): PolicyBlock[] {
  const blocks: PolicyBlock[] = [];
  let paragraph: string[] = [];
  let items: string[] | null = null;

  const flush = () => {
    if (paragraph.length > 0) {
      blocks.push({
        kind: "paragraph",
        inlines: parsePolicyInline(paragraph.join(" ")),
      });
      paragraph = [];
    }
    if (items) {
      blocks.push({ kind: "list", items: items.map(parsePolicyInline) });
      items = null;
    }
  };

  for (const raw of source.replace(/\r\n?/g, "\n").split("\n")) {
    const line = raw.trimEnd();
    if (!line.trim()) {
      flush();
      continue;
    }
    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    if (heading) {
      flush();
      const text = heading[2].trim();
      blocks.push({
        kind: "heading",
        level: heading[1].length as 1 | 2 | 3,
        text,
        id: policyHeadingId(text),
      });
      continue;
    }
    const item = /^[-*]\s+(.*)$/.exec(line);
    if (item) {
      if (paragraph.length > 0) flush();
      items ??= [];
      items.push(item[1].trim());
      continue;
    }
    if (items && /^\s{2,}\S/.test(line)) {
      items[items.length - 1] += ` ${line.trim()}`;
      continue;
    }
    if (items) flush();
    paragraph.push(line.trim());
  }
  flush();
  return blocks;
}
