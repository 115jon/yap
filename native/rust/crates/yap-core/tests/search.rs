//! Message search: the grammar of `src/lib/message-search.ts` (its web
//! tests ported), the suggestion and recents rules of
//! `components/chat/search/*`, the request paths as the web builds them,
//! and the store's panel (debounce, chips, paging, staleness, recents).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::search::{
    add_filter, autocomplete_context, build_search_query, build_suggestions, commit_typed_tokens,
    date_filters_to_range, decode_recents, excerpt_around_match, extract_search_terms,
    find_typed_tokens, flatten_suggestions, group_results_by_channel, has_searchable_input,
    highlight_segments, recent_search_label, resolve_token, save_recent_search, to_search_params,
    Autocomplete, Clock, FilterKey, HasValue, RecentSearch, ResultMessage, SearchChannel,
    SearchFilter, SearchMember, SearchResponse, Segment, Sort, Sources, SuggestionSources,
};
use yap_core::store::time::{day_number, local_midnight, parse_rfc3339};
use yap_core::store::{
    reduce, ApiRequest, Command, Effect, GatewayEvent, Input, RequestId, SearchCommand,
    SearchRequest, SearchScope, SearchView, State, ViewUpdate,
};
use yap_core::{ApiError, LocalOffset};

// The web tests' sources

/// Oct 23 2026, 14:30 local, in UTC-5.
const ZONE: LocalOffset = LocalOffset(|_| -5 * 3600);

fn clock() -> Clock {
    let day = day_number(2026, 10, 23);
    Clock {
        now_ms: local_midnight(day, ZONE) + (14 * 60 + 30) * 60_000,
        offset: ZONE,
    }
}

fn member(id: &str, username: &str, display: Option<&str>) -> SearchMember {
    SearchMember {
        id: id.into(),
        username: username.into(),
        display_name: display.map(Into::into),
        ..SearchMember::default()
    }
}

fn members() -> Vec<SearchMember> {
    vec![
        SearchMember {
            role_color: "#f0b232".into(),
            ..member("u-ada", "ada", Some("Ada Lovelace"))
        },
        member("u-grace", "grace", Some("Grace Hopper")),
        member("u-ken", "ken", None),
    ]
}

fn channels() -> Vec<SearchChannel> {
    let channel = |id: &str, name: &str| SearchChannel {
        id: id.into(),
        name: name.into(),
        voice: false,
    };
    vec![
        channel("ch-general", "general"),
        channel("ch-design", "design-system"),
    ]
}

fn resolve(key: FilterKey, value: &str) -> Option<SearchFilter> {
    let (members, channels) = (members(), channels());
    let sources = Sources {
        members: &members,
        channels: &channels,
        clock: clock(),
    };
    resolve_token(key, value, &sources)
}

fn ctx(input: &str) -> Option<Autocomplete> {
    autocomplete_context(input, input.len())
}

fn has(value: HasValue) -> SearchFilter {
    SearchFilter::Has { value }
}

// getAutocompleteContext

#[test]
fn autocomplete_should_offer_every_key_on_an_empty_input() {
    assert_eq!(
        ctx(""),
        Some(Autocomplete::Key {
            query: String::new(),
            start: 0,
            end: 0
        })
    );
}

#[test]
fn autocomplete_should_offer_keys_after_a_finished_word() {
    assert_eq!(
        ctx("token "),
        Some(Autocomplete::Key {
            query: String::new(),
            start: 6,
            end: 6
        })
    );
}

#[test]
fn autocomplete_should_match_a_partial_key() {
    assert_eq!(
        ctx("hello fr"),
        Some(Autocomplete::Key {
            query: "fr".into(),
            start: 6,
            end: 8
        })
    );
}

#[test]
fn autocomplete_should_return_none_inside_an_ordinary_word() {
    assert_eq!(ctx("hello wor"), None);
}

#[test]
fn autocomplete_should_detect_a_value_being_typed() {
    assert_eq!(
        ctx("spec from:ad"),
        Some(Autocomplete::Value {
            key: FilterKey::From,
            query: "ad".into(),
            start: 5,
            end: 12
        })
    );
}

#[test]
fn autocomplete_should_ignore_the_keys_case() {
    assert!(matches!(
        ctx("HAS:im"),
        Some(Autocomplete::Value { key: FilterKey::Has, query, .. }) if query == "im"
    ));
}

#[test]
fn autocomplete_should_follow_the_caret() {
    assert_eq!(
        autocomplete_context("from:ad spec", 7),
        Some(Autocomplete::Value {
            key: FilterKey::From,
            query: "ad".into(),
            start: 0,
            end: 7
        })
    );
}

#[test]
fn autocomplete_should_keep_spaces_inside_a_quoted_value() {
    assert!(matches!(
        ctx("from:\"Ada Lo"),
        Some(Autocomplete::Value { key: FilterKey::From, query, start: 0, .. }) if query == "Ada Lo"
    ));
}

#[test]
fn autocomplete_should_ignore_unknown_keys() {
    assert_eq!(ctx("http://x"), None);
}

// findTypedTokens and commitTypedTokens

#[test]
fn tokens_should_find_plain_and_quoted_values() {
    let tokens: Vec<(FilterKey, String)> =
        find_typed_tokens("spec from:ada in:\"general\" has:image")
            .into_iter()
            .map(|t| (t.key, t.value))
            .collect();
    assert_eq!(
        tokens,
        vec![
            (FilterKey::From, "ada".into()),
            (FilterKey::In, "general".into()),
            (FilterKey::Has, "image".into()),
        ]
    );
}

#[test]
fn commit_should_turn_resolvable_tokens_into_chips_and_keep_the_rest() {
    let (members, channels) = (members(), channels());
    let sources = Sources {
        members: &members,
        channels: &channels,
        clock: clock(),
    };
    let result = commit_typed_tokens("spec from:ada has:nope in:general", &[], &sources);
    assert_eq!(
        (result.committed, result.text.trim(), result.filters),
        (
            2,
            "spec has:nope",
            vec![
                SearchFilter::From {
                    user_id: "u-ada".into(),
                    label: "Ada Lovelace".into(),
                    username: "ada".into()
                },
                SearchFilter::In {
                    channel_id: "ch-general".into(),
                    label: "general".into()
                },
            ]
        )
    );
}

#[test]
fn resolve_should_match_a_display_name() {
    assert!(matches!(
        resolve(FilterKey::Mentions, "Grace Hopper"),
        Some(SearchFilter::Mentions { user_id, .. }) if user_id == "u-grace"
    ));
}

#[test]
fn resolve_should_match_a_channel_with_a_hash() {
    assert!(matches!(
        resolve(FilterKey::In, "#design-system"),
        Some(SearchFilter::In { channel_id, .. }) if channel_id == "ch-design"
    ));
}

#[test]
fn resolve_should_refuse_an_impossible_date() {
    assert_eq!(resolve(FilterKey::Before, "2026-02-30"), None);
}

#[test]
fn resolve_should_keep_a_valid_date() {
    assert_eq!(
        resolve(FilterKey::Before, "2026-10-01"),
        Some(SearchFilter::Before {
            date: "2026-10-01".into()
        })
    );
}

#[test]
fn resolve_should_read_yesterday_in_local_time() {
    assert_eq!(
        resolve(FilterKey::During, "yesterday"),
        Some(SearchFilter::During {
            date: "2026-10-22".into()
        })
    );
}

#[test]
fn resolve_should_read_pinned_yes() {
    assert_eq!(
        resolve(FilterKey::Pinned, "yes"),
        Some(SearchFilter::Pinned { value: true })
    );
}

#[test]
fn resolve_should_refuse_pinned_maybe() {
    assert_eq!(resolve(FilterKey::Pinned, "maybe"), None);
}

// addFilter

#[test]
fn add_filter_should_stack_has_chips_and_drop_duplicates() {
    let mut filters = Vec::new();
    for value in [HasValue::Link, HasValue::Image, HasValue::Link] {
        filters = add_filter(&filters, has(value));
    }
    assert_eq!(filters, vec![has(HasValue::Image), has(HasValue::Link)]);
}

#[test]
fn add_filter_should_keep_one_pinned_and_let_during_replace_bounds() {
    let mut filters = vec![
        SearchFilter::Pinned { value: true },
        SearchFilter::Before {
            date: "2026-10-01".into(),
        },
    ];
    filters = add_filter(&filters, SearchFilter::Pinned { value: false });
    filters = add_filter(
        &filters,
        SearchFilter::During {
            date: "2026-09-12".into(),
        },
    );
    assert_eq!(
        filters,
        vec![
            SearchFilter::Pinned { value: false },
            SearchFilter::During {
                date: "2026-09-12".into()
            },
        ]
    );
}

#[test]
fn add_filter_should_stop_at_ten_of_a_key() {
    let mut filters = Vec::new();
    for i in 0..11 {
        filters = add_filter(
            &filters,
            SearchFilter::In {
                channel_id: format!("c{i}"),
                label: format!("c{i}"),
            },
        );
    }
    assert_eq!(filters.len(), 10);
}

// Dates to range

#[test]
fn bounds_should_map_to_local_day_boundaries() {
    let range = date_filters_to_range(
        &[
            SearchFilter::After {
                date: "2026-10-01".into(),
            },
            SearchFilter::Before {
                date: "2026-10-10".into(),
            },
        ],
        ZONE,
    );
    assert_eq!(
        range,
        (
            Some("2026-10-02T05:00:00.000Z".into()),
            Some("2026-10-10T05:00:00.000Z".into())
        )
    );
}

#[test]
fn during_should_span_one_local_day() {
    let range = date_filters_to_range(
        &[SearchFilter::During {
            date: "2026-10-05".into(),
        }],
        ZONE,
    );
    assert_eq!(
        range,
        (
            Some("2026-10-05T05:00:00.000Z".into()),
            Some("2026-10-06T05:00:00.000Z".into())
        )
    );
}

// Terms and highlighting

#[test]
fn terms_should_keep_phrases_and_drop_single_letters() {
    assert_eq!(
        extract_search_terms("token \"design system\" a Token"),
        vec!["token".to_owned(), "design system".to_owned()]
    );
}

#[test]
fn highlight_should_mark_every_occurrence_and_merge_overlaps() {
    let seg = |text: &str, matched: bool| Segment {
        text: text.into(),
        matched,
    };
    assert_eq!(
        highlight_segments("Token tokens TOK", &["tok".into(), "token".into()]),
        vec![
            seg("Token", true),
            seg(" ", false),
            seg("token", true),
            seg("s ", false),
            seg("TOK", true),
        ]
    );
}

#[test]
fn highlight_should_return_the_whole_text_without_a_match() {
    assert_eq!(
        highlight_segments("hello", &["zzz".into()]),
        vec![Segment {
            text: "hello".into(),
            matched: false
        }]
    );
}

#[test]
fn excerpt_should_start_near_the_first_match() {
    let content = format!(
        "{}the needle is here {}",
        "lorem ipsum ".repeat(60),
        "dolor ".repeat(20)
    );
    let excerpt = excerpt_around_match(&content, &["needle".into()], 200, 40);
    assert!(
        excerpt.starts_with('…')
            && excerpt.contains("needle")
            && excerpt.encode_utf16().count() <= 203,
        "{excerpt}"
    );
}

#[test]
fn excerpt_should_keep_short_content() {
    assert_eq!(
        excerpt_around_match("short", &["x".into()], 320, 60),
        "short"
    );
}

// Query building

fn web_filters() -> Vec<SearchFilter> {
    vec![
        SearchFilter::From {
            user_id: "u-ada".into(),
            label: "Ada".into(),
            username: "ada".into(),
        },
        SearchFilter::In {
            channel_id: "ch-general".into(),
            label: "general".into(),
        },
        has(HasValue::Image),
        SearchFilter::Pinned { value: true },
    ]
}

#[test]
fn params_should_match_the_webs_url_search_params() {
    let query = build_search_query("token scale", &web_filters(), Sort::Relevant, 25, ZONE);
    assert_eq!(
        to_search_params(&query),
        "q=token+scale&from=u-ada&in=ch-general&has=image&pinned=true&sort=relevant&limit=25&offset=25"
    );
}

#[test]
fn params_should_leave_out_newest_and_a_zero_offset() {
    let query = build_search_query("hi", &[], Sort::Newest, 0, ZONE);
    assert_eq!(to_search_params(&query), "q=hi&limit=25");
}

#[test]
fn params_should_encode_dates_and_lists() {
    let filters = vec![
        SearchFilter::Mentions {
            user_id: "u1".into(),
            label: "A".into(),
            username: "a".into(),
        },
        SearchFilter::Mentions {
            user_id: "u2".into(),
            label: "B".into(),
            username: "b".into(),
        },
        SearchFilter::During {
            date: "2026-10-05".into(),
        },
    ];
    let query = build_search_query("", &filters, Sort::Oldest, 0, ZONE);
    assert_eq!(
        to_search_params(&query),
        "mentions=u1%2Cu2&before=2026-10-06T05%3A00%3A00.000Z&after=2026-10-05T05%3A00%3A00.000Z&sort=oldest&limit=25"
    );
}

#[test]
fn query_should_cut_text_at_two_hundred_units() {
    let query = build_search_query(&"a".repeat(250), &[], Sort::Newest, 0, ZONE);
    assert_eq!(query.text.len(), 200);
}

#[test]
fn searchable_should_refuse_one_letter_alone() {
    assert!(!has_searchable_input("a", &[]));
}

#[test]
fn searchable_should_allow_a_filter_alone() {
    assert!(has_searchable_input("", &web_filters()));
}

// groupResultsByChannel

#[test]
fn grouping_should_join_consecutive_results_of_a_channel() {
    let m = |id: &str, channel: &str| ResultMessage {
        id: id.into(),
        channel_id: channel.into(),
        ..ResultMessage::default()
    };
    let list = [m("1", "a"), m("2", "a"), m("3", "b"), m("4", "a")];
    let groups: Vec<(&str, usize)> = group_results_by_channel(&list)
        .into_iter()
        .map(|(_, channel, _, items)| (channel, items.len()))
        .collect();
    assert_eq!(groups, vec![("a", 2), ("b", 1), ("a", 1)]);
}

// Suggestions (search-suggestions.test.ts)

fn suggest_with(input: &str, allow_channel_filter: bool, recent: &[RecentSearch]) -> Vec<String> {
    let channels = vec![
        SearchChannel {
            id: "c-general".into(),
            name: "general".into(),
            voice: false,
        },
        SearchChannel {
            id: "c-design".into(),
            name: "design-system".into(),
            voice: false,
        },
        SearchChannel {
            id: "c-lounge".into(),
            name: "Lounge".into(),
            voice: true,
        },
    ];
    let members = vec![
        member("u-ada", "ada", Some("Ada Lovelace")),
        member("u-grace", "grace", Some("Grace Hopper")),
        member("u-mh", "mhamilton", Some("Margaret Hamilton")),
    ];
    let sources = SuggestionSources {
        members: &members,
        channels: &channels,
        clock: clock(),
        allow_channel_filter,
        recent,
    };
    let groups = build_suggestions(ctx(input).as_ref(), &sources);
    flatten_suggestions(&groups)
        .into_iter()
        .map(|s| s.id())
        .collect()
}

fn suggest(input: &str) -> Vec<String> {
    suggest_with(input, true, &[])
}

#[test]
fn suggestions_should_list_every_key_then_recents_on_an_empty_field() {
    let recent = [RecentSearch {
        text: "token".into(),
        filters: Vec::new(),
    }];
    assert_eq!(
        suggest_with("", true, &recent),
        [
            "key:from",
            "key:mentions",
            "key:in",
            "key:has",
            "key:before",
            "key:after",
            "key:during",
            "key:pinned",
            "recent:0"
        ]
    );
}

#[test]
fn suggestions_should_hide_in_where_there_is_one_channel() {
    assert!(!suggest_with("", false, &[]).contains(&"key:in".to_owned()));
}

#[test]
fn suggestions_should_offer_no_channels_in_a_dm() {
    assert!(suggest_with("in:", false, &[]).is_empty());
}

#[test]
fn suggestions_should_narrow_keys_by_prefix() {
    assert_eq!(suggest("pi"), ["key:pinned"]);
}

#[test]
fn suggestions_should_rank_word_starts_of_names() {
    assert_eq!(suggest("from:ham"), ["user:from:u-mh"]);
}

#[test]
fn suggestions_should_rank_prefixes_before_word_starts() {
    assert_eq!(
        suggest("mentions:g"),
        ["user:mentions:u-grace", "user:mentions:u-mh"]
    );
}

#[test]
fn suggestions_should_find_a_channel_after_a_hash() {
    assert_eq!(suggest("in:#des"), ["channel:c-design"]);
}

#[test]
fn suggestions_should_narrow_has_values() {
    assert_eq!(suggest("has:i"), ["has:image"]);
}

#[test]
fn suggestions_should_offer_both_pinned_values() {
    assert_eq!(suggest("pinned:"), ["pinned:true", "pinned:false"]);
}

#[test]
fn suggestions_should_offer_the_date_presets() {
    assert_eq!(
        suggest("before:"),
        [
            "date:before:2026-10-23",
            "date:before:2026-10-22",
            "date:before:2026-10-16",
            "date:before:2026-09-23"
        ]
    );
}

#[test]
fn suggestions_should_offer_a_typed_date() {
    assert_eq!(suggest("after:2026-01-05"), ["date:after:2026-01-05"]);
}

#[test]
fn suggestions_should_be_empty_inside_plain_words() {
    assert!(suggest("hello wor").is_empty());
}

// Recents (recent-searches.ts)

#[test]
fn recent_label_should_read_chips_then_text() {
    let recent = RecentSearch {
        text: " token ".into(),
        filters: web_filters(),
    };
    assert_eq!(
        recent_search_label(&recent),
        "from:ada in:general has:image pinned:yes token"
    );
}

#[test]
fn saving_a_recent_should_put_it_first_once() {
    let a = RecentSearch {
        text: "a".into(),
        filters: Vec::new(),
    };
    let b = RecentSearch {
        text: "b".into(),
        filters: Vec::new(),
    };
    let list = save_recent_search(&[a.clone(), b.clone()], b.clone());
    assert_eq!(list, vec![b, a]);
}

#[test]
fn saving_a_blank_search_should_change_nothing() {
    assert!(save_recent_search(&[], RecentSearch::default()).is_empty());
}

#[test]
fn recents_should_read_the_webs_shape_and_skip_the_rest() {
    let stored = vec![
        json!({ "text": "hi", "filters": [{ "key": "from", "userId": "u", "label": "U", "username": "u" }] }),
        json!({ "text": 3 }),
    ];
    assert_eq!(
        decode_recents(stored),
        vec![RecentSearch {
            text: "hi".into(),
            filters: vec![SearchFilter::From {
                user_id: "u".into(),
                label: "U".into(),
                username: "u".into()
            }],
        }]
    );
}

#[test]
fn chips_should_serialize_as_the_web_stores_them() {
    assert_eq!(
        serde_json::to_value(SearchFilter::In {
            channel_id: "c".into(),
            label: "general".into()
        })
        .unwrap_or_default(),
        json!({ "key": "in", "channelId": "c", "label": "general" })
    );
}

#[test]
fn response_should_skip_a_result_that_does_not_decode() {
    let body = json!({
        "messages": [
            { "id": "m1", "channel_id": "c", "channel_name": "general", "author_id": "u",
              "author": { "id": "u", "username": "u" }, "content": "hi", "is_pinned": true,
              "created_at": "2026-10-23T12:00:00.000Z", "attachments": [], "embed_count": 1 },
            { "id": 7 },
        ],
        "total": 2, "limit": 25, "offset": 0, "has_more": false,
    });
    let page = SearchResponse::decode(&body);
    assert_eq!((page.messages.len(), page.total), (1, 2));
}

// Request paths

#[test]
fn server_search_should_go_to_the_servers_endpoint() {
    let request = ApiRequest::Search(SearchRequest {
        scope: SearchScope::Server("srv 1".into()),
        query: build_search_query("hi", &[], Sort::Newest, 0, ZONE),
    });
    assert_eq!(request.path(), "/api/servers/srv%201/search?q=hi&limit=25");
}

#[test]
fn dm_search_should_go_to_the_channels_endpoint() {
    let request = ApiRequest::Search(SearchRequest {
        scope: SearchScope::Channel("dm1".into()),
        query: build_search_query("hi", &[], Sort::Newest, 0, ZONE),
    });
    assert_eq!(request.path(), "/api/channels/dm1/search?q=hi&limit=25");
}

// The store

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";
const ME: &str = "user_fixture_me";
const ALICE: &str = "user_fixture_alice";
const DM: &str = "dm_fixture_alice";
/// 2026-10-23T12:00:00Z.
const NOW: i64 = 1_792_756_800_000;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("fixture");
    serde_json::from_str(&text).expect("json")
}

fn user(id: &str, username: &str, display: &str) -> Value {
    json!({ "id": id, "username": username, "display_name": display, "avatar_url": null })
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    effects: Vec<Effect>,
    view: SearchView,
}

impl Client {
    fn input(&mut self, input: Input) {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Search(view)) => self.view = (**view).clone(),
                _ => {}
            }
        }
        self.effects = fx;
    }

    fn search(&mut self, command: SearchCommand) {
        self.input(Input::User(Command::Search(command)));
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) {
        let id = self
            .asked
            .iter()
            .rev()
            .find(|(_, r)| r == request)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("no {request:?}"));
        let result = request.decode(body);
        self.input(Input::Http { id, result });
    }

    /// The newest search request and its id.
    fn last_search(&self) -> (RequestId, String) {
        self.asked
            .iter()
            .rev()
            .find_map(|(id, r)| matches!(r, ApiRequest::Search(_)).then(|| (*id, r.path())))
            .unwrap_or_else(|| panic!("no search in {:#?}", self.asked))
    }

    fn searches(&self) -> usize {
        self.asked
            .iter()
            .filter(|(_, r)| matches!(r, ApiRequest::Search(_)))
            .count()
    }

    fn answer(&mut self, id: RequestId, result: Result<Value, ApiError>) {
        let request = self
            .asked
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, r)| r.clone())
            .unwrap_or_else(|| panic!("no request {id:?}"));
        let result = result.and_then(|body| request.decode(body));
        self.input(Input::Http { id, result });
    }

    /// Signed in on the fixture server's `general`, members and DMs loaded.
    fn online() -> Self {
        let mut client = Self {
            state: State::new(BASE).with_local_offset(LocalOffset::UTC),
            asked: Vec::new(),
            effects: Vec::new(),
            view: SearchView::default(),
        };
        client.input(Input::User(Command::UseToken("tok_fixture".into())));
        client.respond(&ApiRequest::Me, fixture("user-me.json"));
        client.respond(&ApiRequest::Servers, fixture("servers-list.json"));
        let channels = ApiRequest::Channels {
            server_id: SERVER.into(),
        };
        client.respond(&channels, fixture("channels-response.json"));
        let page = ApiRequest::Messages {
            channel_id: TEXT.into(),
            before: None,
        };
        client.respond(&page, fixture("messages-page.json"));
        client.respond(
            &ApiRequest::Members {
                server_id: SERVER.into(),
            },
            json!([
                { "user": user(ME, "me", "Fixture Me"), "roles": [] },
                { "user": user(ALICE, "alice", "Alice"),
                  "roles": [{ "id": "r", "name": "Mods", "position": 2, "color": "#ff0000", "is_default": false }] },
            ]),
        );
        client.respond(
            &ApiRequest::Dms,
            json!([{ "id": DM, "name": "x", "created_at": null, "recipient": user(ALICE, "alice", "Alice") }]),
        );
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client
    }

    fn open(&mut self, text: &str) {
        self.search(SearchCommand::Open {
            text: text.into(),
            now_ms: NOW,
            recents: BTreeMap::new(),
        });
        self.search(SearchCommand::Focus(true));
    }

    fn edit(&mut self, text: &str) {
        self.search(SearchCommand::Edit {
            text: text.into(),
            caret: text.encode_utf16().count(),
            now_ms: NOW,
        });
    }

    /// The debounce the last edit scheduled, run.
    fn fire_debounce(&mut self) {
        let input = self
            .effects
            .iter()
            .find_map(|e| match e {
                Effect::Later { input, .. } => Some((**input).clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no debounce in {:#?}", self.effects));
        self.input(input);
    }
}

fn page(ids: &[&str], total: u64, has_more: bool) -> Value {
    let messages: Vec<Value> = ids
        .iter()
        .map(|id| {
            json!({
                "id": id, "channel_id": TEXT, "channel_name": "general", "author_id": ALICE,
                "author": { "id": ALICE, "username": "alice", "display_name": "Alice" },
                "content": "the token scale", "is_pinned": false,
                "created_at": "2026-10-23T10:00:00.000Z", "attachments": [], "embed_count": 0,
            })
        })
        .collect();
    json!({ "messages": messages, "total": total, "limit": 25, "offset": 0, "has_more": has_more })
}

#[test]
fn open_should_search_handed_over_text_with_its_chips() {
    let mut client = Client::online();
    client.open("from:alice token");
    assert_eq!(
        client.last_search().1,
        format!("/api/servers/{SERVER}/search?q=token&from={ALICE}&limit=25")
    );
}

#[test]
fn open_should_show_the_handed_over_chip() {
    let mut client = Client::online();
    client.open("from:alice token");
    assert_eq!(
        (
            client.view.text.as_str(),
            client.view.chips[0].value.as_str()
        ),
        ("token", "Alice")
    );
}

#[test]
fn a_dm_should_search_its_channel() {
    let mut client = Client::online();
    client.input(Input::User(Command::SelectChannel(DM.into())));
    client.open("hello");
    assert_eq!(
        client.last_search().1,
        format!("/api/channels/{DM}/search?q=hello&limit=25")
    );
}

#[test]
fn typing_should_wait_for_the_debounce() {
    let mut client = Client::online();
    client.open("");
    client.edit("tok");
    assert_eq!(client.searches(), 0);
}

#[test]
fn the_debounce_should_search_the_text() {
    let mut client = Client::online();
    client.open("");
    client.edit("tok");
    client.fire_debounce();
    assert_eq!(
        client.last_search().1,
        format!("/api/servers/{SERVER}/search?q=tok&limit=25")
    );
}

#[test]
fn an_older_debounce_should_not_search() {
    let mut client = Client::online();
    client.open("");
    client.edit("to");
    let older = client.effects.clone();
    client.edit("tok");
    for effect in older {
        if let Effect::Later { input, .. } = effect {
            client.input(*input);
        }
    }
    assert_eq!(client.searches(), 0);
}

#[test]
fn a_finished_token_should_become_a_chip_and_search_at_once() {
    let mut client = Client::online();
    client.open("");
    client.edit("has:image ");
    assert_eq!(
        (client.view.chips.len(), client.last_search().1),
        (
            1,
            format!("/api/servers/{SERVER}/search?has=image&limit=25")
        )
    );
}

#[test]
fn a_half_typed_value_should_not_search() {
    let mut client = Client::online();
    client.open("");
    client.edit("from:al");
    assert!(!client
        .effects
        .iter()
        .any(|e| matches!(e, Effect::Later { .. })));
}

#[test]
fn a_value_being_typed_should_light_its_first_suggestion() {
    let mut client = Client::online();
    client.open("");
    client.edit("from:al");
    assert_eq!(
        (client.view.suggestions_open, client.view.active_id.as_str()),
        (true, "user:from:user_fixture_alice")
    );
}

#[test]
fn picking_a_suggestion_should_make_a_chip_and_search() {
    let mut client = Client::online();
    client.open("");
    client.edit("spec from:al");
    client.search(SearchCommand::Pick {
        id: format!("user:from:{ALICE}"),
        now_ms: NOW,
    });
    assert_eq!(
        client.last_search().1,
        format!("/api/servers/{SERVER}/search?q=spec&from={ALICE}&limit=25")
    );
}

#[test]
fn picking_a_key_should_type_it() {
    let mut client = Client::online();
    client.open("");
    client.edit("hello fr");
    client.search(SearchCommand::Pick {
        id: "key:from".into(),
        now_ms: NOW,
    });
    assert_eq!(
        (client.view.text.as_str(), client.view.caret),
        ("hello from:", 11)
    );
}

#[test]
fn arrows_should_wrap_through_the_suggestions() {
    let mut client = Client::online();
    client.open("");
    client.edit("pinned:");
    client.search(SearchCommand::Move(1));
    client.search(SearchCommand::Move(1));
    assert_eq!(client.view.active_id, "pinned:true");
}

#[test]
fn the_newest_answer_should_win() {
    let mut client = Client::online();
    client.open("first");
    let (old, _) = client.last_search();
    client.search(SearchCommand::SetSort(Sort::Oldest));
    client.answer(old, Ok(page(&["stale"], 1, false)));
    assert_eq!(client.view.status, "loading");
}

#[test]
fn an_answer_should_show_grouped_results() {
    let mut client = Client::online();
    client.open("token");
    let (id, _) = client.last_search();
    client.answer(id, Ok(page(&["m1", "m2"], 2, false)));
    let group = &client.view.results[0];
    assert_eq!(
        (
            group.name.as_str(),
            group.items.len(),
            group.items[0].color.as_str()
        ),
        ("general", 2, "#ff0000")
    );
}

#[test]
fn a_result_should_mark_its_matches() {
    let mut client = Client::online();
    client.open("token");
    let (id, _) = client.last_search();
    client.answer(id, Ok(page(&["m1"], 1, false)));
    let runs: Vec<(&str, &str)> = client.view.results[0].items[0]
        .runs
        .iter()
        .map(|r| (r.text.as_str(), r.kind))
        .collect();
    assert_eq!(
        runs,
        [("the ", "text"), ("token", "match"), (" scale", "text")]
    );
}

#[test]
fn load_more_should_ask_for_the_next_offset() {
    let mut client = Client::online();
    client.open("token");
    let (id, _) = client.last_search();
    client.answer(id, Ok(page(&["m1", "m2"], 4, true)));
    client.search(SearchCommand::LoadMore);
    assert_eq!(
        client.last_search().1,
        format!("/api/servers/{SERVER}/search?q=token&limit=25&offset=2")
    );
}

#[test]
fn the_next_page_should_append_only_new_results() {
    let mut client = Client::online();
    client.open("token");
    let (id, _) = client.last_search();
    client.answer(id, Ok(page(&["m1", "m2"], 4, true)));
    client.search(SearchCommand::LoadMore);
    let (id, _) = client.last_search();
    client.answer(id, Ok(page(&["m2", "m3"], 4, false)));
    let ids: Vec<&str> = client.view.results[0]
        .items
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(ids, ["m1", "m2", "m3"]);
}

#[test]
fn a_failed_page_should_retry_the_same_offset() {
    let mut client = Client::online();
    client.open("token");
    let (id, _) = client.last_search();
    client.answer(id, Ok(page(&["m1", "m2"], 4, true)));
    client.search(SearchCommand::LoadMore);
    let (id, _) = client.last_search();
    client.answer(
        id,
        Err(ApiError::Http {
            status: 500,
            message: "nope".into(),
        }),
    );
    let failed = client.view.load_more_failed;
    client.search(SearchCommand::Retry);
    assert_eq!(
        (failed, client.last_search().1),
        (
            true,
            format!("/api/servers/{SERVER}/search?q=token&limit=25&offset=2")
        )
    );
}

#[test]
fn a_failed_search_should_show_the_error() {
    let mut client = Client::online();
    client.open("token");
    let (id, _) = client.last_search();
    client.answer(
        id,
        Err(ApiError::Http {
            status: 500,
            message: "nope".into(),
        }),
    );
    assert_eq!(
        (
            client.view.status.as_str(),
            client.view.announcement.as_str()
        ),
        ("error", "Search failed")
    );
}

#[test]
fn clearing_should_go_back_to_idle() {
    let mut client = Client::online();
    client.open("from:alice token");
    client.search(SearchCommand::Clear);
    assert_eq!(
        (client.view.status.as_str(), client.view.chips.len()),
        ("idle", 0)
    );
}

#[test]
fn backspace_at_the_start_should_remove_the_last_chip() {
    let mut client = Client::online();
    client.open("has:image has:link");
    client.search(SearchCommand::RemoveLastFilter);
    assert_eq!(
        client.last_search().1,
        format!("/api/servers/{SERVER}/search?has=image&limit=25")
    );
}

#[test]
fn opening_a_result_should_remember_the_search() {
    let mut client = Client::online();
    client.open("has:image token");
    let (id, _) = client.last_search();
    client.answer(id, Ok(page(&["m1"], 1, false)));
    client.search(SearchCommand::Opened("m1".into()));
    assert_eq!(
        (
            client.view.recents.len(),
            client.view.idle[1].items[0].label.as_str(),
            client.view.results[0].items[0].active
        ),
        (1, "has:image token", true)
    );
}

#[test]
fn another_server_should_start_a_fresh_search() {
    let mut client = Client::online();
    client.open("token");
    client.input(Input::User(Command::SelectChannel(DM.into())));
    assert_eq!(
        (client.view.scope.as_str(), client.view.text.as_str()),
        ("channel", "")
    );
}

#[test]
fn reopening_in_the_same_scope_should_keep_the_search() {
    let mut client = Client::online();
    client.open("token");
    let before = client.searches();
    client.search(SearchCommand::Open {
        text: String::new(),
        now_ms: NOW,
        recents: BTreeMap::new(),
    });
    assert_eq!(
        (client.searches(), client.view.text.as_str()),
        (before, "token")
    );
}

#[test]
fn closing_should_drop_the_answer_in_flight() {
    let mut client = Client::online();
    client.open("token");
    let (id, _) = client.last_search();
    client.search(SearchCommand::Close);
    client.answer(id, Ok(page(&["m1"], 1, false)));
    assert!(!client.view.open);
}

#[test]
fn a_result_time_should_read_like_the_web() {
    let mut client = Client::online();
    client.open("token");
    let (id, _) = client.last_search();
    client.answer(id, Ok(page(&["m1"], 1, false)));
    let row = &client.view.results[0].items[0];
    assert_eq!(
        (row.time.as_str(), row.when.as_str()),
        ("10:00 AM", "Today at 10:00 AM")
    );
}

#[test]
fn now_should_be_the_fixture_noon() {
    assert_eq!(parse_rfc3339("2026-10-23T12:00:00.000Z"), Some(NOW));
}
