// The search text rules run from yap-core-wasm on the web. The fixtures are
// what the web's own TypeScript returned before it was replaced (bar the
// entries with a note, where it mis-sliced text whose lowercase is longer),
// and the Rust side checks the same file (packages/rust/yap-core-wasm/tests).
import { describe, expect, it } from "vitest";
import fixtures from "../../../../packages/rust/yap-core-wasm/fixtures/search-text.json";
import {
  excerptAroundMatch,
  extractSearchTerms,
  highlightSegments,
} from "@/lib/yap-core-wasm";

const { contents } = fixtures;

describe("yap-core-wasm search text", () => {
  it.each(fixtures.terms)("extractSearchTerms($text)", ({ text, terms }) => {
    expect(extractSearchTerms(text)).toEqual(terms);
  });

  it.each(fixtures.highlight)(
    "highlightSegments(#$content, $terms)",
    ({ content, terms, segments }) => {
      expect(highlightSegments(contents[content], terms)).toEqual(segments);
    },
  );

  it.each(fixtures.excerpt)(
    "excerptAroundMatch(#$content, $terms, $max, $lead)",
    ({ content, terms, max, lead, excerpt }) => {
      expect(excerptAroundMatch(contents[content], terms, max, lead)).toBe(
        excerpt,
      );
    },
  );

  it("keeps the defaults of the web's version (320 chars, 60 lead)", () => {
    const long = `${"word ".repeat(100)}needle ${"tail ".repeat(100)}`;
    expect(excerptAroundMatch(long, ["needle"])).toBe(
      excerptAroundMatch(long, ["needle"], 320, 60),
    );
  });
});
