// yap-core's pure rules, compiled from Rust (packages/rust/yap-core-wasm),
// so the web and the native apps run one implementation.
//
// Every function here is synchronous and needs the module loaded first:
// `loadYapCore()` (./load) in the browser, `./server` in the Worker (which
// loads it on import). A call before that throws.

import {
  excerptAroundMatch as wasmExcerptAroundMatch,
  extractSearchTerms as wasmExtractSearchTerms,
  highlightSegmentsJson,
} from "./pkg/yap_core_wasm.js";

/** A run of message text, matched by a search term or not. */
export interface HighlightSegment {
  text: string;
  match: boolean;
}

/**
 * Splits free text into match terms. "Quoted phrases" stay whole; bare words
 * split on whitespace. Single characters are dropped unless quoted; at most
 * eight terms, case-insensitively unique (yap_core::search).
 */
export function extractSearchTerms(text: string): string[] {
  return wasmExtractSearchTerms(text);
}

/** Case-insensitive, overlap-safe highlight of every term occurrence. */
export function highlightSegments(
  content: string,
  terms: readonly string[],
): HighlightSegment[] {
  const segments: HighlightSegment[] = JSON.parse(
    highlightSegmentsJson(content, [...terms]),
  );
  return segments;
}

/**
 * Long messages are clamped in the result list, so a match deep in the text
 * would be cut off. Start the excerpt shortly before the first match.
 * Lengths are UTF-16 units, as `String.length` counts them.
 */
export function excerptAroundMatch(
  content: string,
  terms: readonly string[],
  maxChars = 320,
  lead = 60,
): string {
  return wasmExcerptAroundMatch(content, [...terms], maxChars, lead);
}
