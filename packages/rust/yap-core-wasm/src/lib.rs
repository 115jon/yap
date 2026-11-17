//! yap-core without its `runtime` feature, for the web: the rules the web
//! and the native apps share, exported through wasm-bindgen
//! (`pnpm core-wasm:build` writes src/lib/yap-core-wasm/pkg, and
//! src/lib/yap-core-wasm wraps it).
//!
//! Building this crate for `wasm32-unknown-unknown` (CI's rust-shared job)
//! is also the check that the pure half of yap-core (the store's reducer,
//! state, view and effects, and the rules modules) needs no I/O crate. Code
//! that talks to the network, the disk or the keyring sits behind yap-core's
//! `runtime` feature.
//!
//! Compiling is not running: `std::time::Instant::now`, `std::fs` and
//! `std::env` build for this target but fail or panic when called, so an
//! export that reaches them needs a browser stand-in first. The search
//! text rules below reach none of them.
//!
//! Strings cross as UTF-8 (wasm-bindgen copies them); yap-core's lengths
//! and offsets are in UTF-16 units where the web's were, so the results
//! match `String.length`.

use wasm_bindgen::prelude::*;
use yap_core::search;

/// `extractSearchTerms`: "quoted phrases" whole, bare words split on
/// whitespace, single characters dropped unless quoted; case-insensitively
/// unique, at most eight.
#[wasm_bindgen(js_name = extractSearchTerms)]
pub fn extract_search_terms(text: &str) -> Vec<String> {
    search::extract_search_terms(text)
}

/// `highlightSegments`, as JSON (`[{"text": "...", "match": true}, ...]`):
/// one string across the boundary instead of an object per segment.
#[wasm_bindgen(js_name = highlightSegmentsJson)]
pub fn highlight_segments_json(content: &str, terms: Vec<String>) -> String {
    // Serializing strings and booleans cannot fail.
    serde_json::to_string(&search::highlight_segments(content, &terms)).unwrap_or_default()
}

/// `excerptAroundMatch`: lengths in UTF-16 units.
#[wasm_bindgen(js_name = excerptAroundMatch)]
pub fn excerpt_around_match(content: &str, terms: Vec<String>, max: usize, lead: usize) -> String {
    search::excerpt_around_match(content, &terms, max, lead)
}
