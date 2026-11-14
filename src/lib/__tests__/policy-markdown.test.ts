import { describe, expect, it } from "vitest";
import privacyPolicy from "../../../PRIVACY.md?raw";
import {
  parsePolicy,
  parsePolicyInline,
  policyHeadingId,
  type PolicyInline,
} from "../policy-markdown";

function inlinesOf(source: string): PolicyInline[] {
  return parsePolicy(source).flatMap((block) => {
    if (block.kind === "paragraph") return block.inlines;
    if (block.kind === "list") return block.items.flat();
    return [];
  });
}

describe("parsePolicy", () => {
  it("reads headings, paragraphs and lists with continuation lines", () => {
    const blocks = parsePolicy(
      [
        "# Title",
        "",
        "One line",
        "and the next.",
        "",
        "## Where it goes",
        "",
        "- first item",
        "  continues here;",
        "- second item.",
        "After the list.",
      ].join("\n"),
    );
    expect(blocks).toEqual([
      { kind: "heading", level: 1, text: "Title", id: "title" },
      {
        kind: "paragraph",
        inlines: [{ kind: "text", text: "One line and the next." }],
      },
      {
        kind: "heading",
        level: 2,
        text: "Where it goes",
        id: "where-it-goes",
      },
      {
        kind: "list",
        items: [
          [{ kind: "text", text: "first item continues here;" }],
          [{ kind: "text", text: "second item." }],
        ],
      },
      {
        kind: "paragraph",
        inlines: [{ kind: "text", text: "After the list." }],
      },
    ]);
  });

  it("reads code, autolinks, links and bold", () => {
    expect(
      parsePolicyInline(
        "Use `yap.example` or mail <a@b.example>, see <https://x.example/p> and [docs](https://d.example) **now**.",
      ),
    ).toEqual([
      { kind: "text", text: "Use " },
      { kind: "code", text: "yap.example" },
      { kind: "text", text: " or mail " },
      { kind: "link", text: "a@b.example", href: "mailto:a@b.example" },
      { kind: "text", text: ", see " },
      {
        kind: "link",
        text: "https://x.example/p",
        href: "https://x.example/p",
      },
      { kind: "text", text: " and " },
      { kind: "link", text: "docs", href: "https://d.example" },
      { kind: "text", text: " " },
      { kind: "strong", text: "now" },
      { kind: "text", text: "." },
    ]);
  });

  it("keeps links to other schemes as text", () => {
    expect(
      parsePolicyInline("[x](javascript:alert(1)) <http://plain.example>"),
    ).toEqual([{ kind: "text", text: "x) <http://plain.example>" }]);
  });

  it("anchors headings by their words", () => {
    expect(policyHeadingId("Deleting your account")).toBe(
      "deleting-your-account",
    );
    expect(policyHeadingId("Microphone, camera and screen sharing")).toBe(
      "microphone-camera-and-screen-sharing",
    );
  });
});

describe("PRIVACY.md", () => {
  const blocks = parsePolicy(privacyPolicy);
  const headings = blocks.filter((block) => block.kind === "heading");

  it("opens with its title and has unique anchors", () => {
    expect(blocks[0]).toMatchObject({ kind: "heading", level: 1 });
    const ids = headings.map((heading) => heading.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("keeps the anchors other pages link to", () => {
    // Play's account deletion link and the Data safety notes point here.
    expect(headings.map((heading) => heading.id)).toEqual(
      expect.arrayContaining([
        "deleting-your-account",
        "retention-and-deletion",
        "where-information-is-sent",
      ]),
    );
  });

  it("uses only the subset the page renders", () => {
    for (const inline of inlinesOf(privacyPolicy)) {
      if (inline.kind !== "text") continue;
      expect(inline.text).not.toMatch(/`|\]\(|\*\*|<[a-z]+:|<[^>\s]+@/i);
    }
    for (const line of privacyPolicy.split(/\r?\n/)) {
      expect(line).not.toMatch(/^(#{4,}|\d+\.\s|>|\||```|<[a-z])/);
    }
  });
});
