//! yap_core::search's text rules against the fixtures the web checks its
//! wasm build with (src/lib/yap-core-wasm/__tests__/search-text.test.ts):
//! what the web's TypeScript returned before the wasm build replaced it.

use serde_json::Value;
use yap_core::search::{excerpt_around_match, extract_search_terms, highlight_segments};

fn fixtures() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/search-text.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture file")).expect("JSON")
}

fn strings(value: &Value) -> Vec<String> {
    serde_json::from_value(value.clone()).expect("a list of strings")
}

fn content<'a>(fixtures: &'a Value, case: &Value) -> &'a str {
    let index = case["content"].as_u64().expect("content index") as usize;
    fixtures["contents"][index].as_str().expect("content")
}

#[test]
fn terms_should_match_the_fixtures() {
    let fixtures = fixtures();
    for case in fixtures["terms"].as_array().expect("terms") {
        let text = case["text"].as_str().expect("text");
        assert_eq!(
            extract_search_terms(text),
            strings(&case["terms"]),
            "{text:?}"
        );
    }
}

#[test]
fn highlights_should_match_the_fixtures() {
    let fixtures = fixtures();
    for case in fixtures["highlight"].as_array().expect("highlight") {
        let content = content(&fixtures, case);
        let got = serde_json::to_value(highlight_segments(content, &strings(&case["terms"])))
            .expect("segments");
        assert_eq!(got, case["segments"], "{content:?} {}", case["terms"]);
    }
}

#[test]
fn excerpts_should_match_the_fixtures() {
    let fixtures = fixtures();
    for case in fixtures["excerpt"].as_array().expect("excerpt") {
        let content = content(&fixtures, case);
        let max = case["max"].as_u64().expect("max") as usize;
        let lead = case["lead"].as_u64().expect("lead") as usize;
        let got = excerpt_around_match(content, &strings(&case["terms"]), max, lead);
        assert_eq!(
            got,
            case["excerpt"].as_str().expect("excerpt"),
            "{content:?}"
        );
    }
}
