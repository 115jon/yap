//! Message search (SearchPanel.tsx `SearchPanelBody`, useMessageSearch.ts):
//! the field's rules (chips committed as they are typed, suggestions and
//! their highlight, the 350 ms debounce), one request at a time (a newer
//! one drops the older answer), offset paging, retry, and the view.

use std::time::Duration;

use yap_protocol::rest::ChannelType;

use super::Effects;
use crate::api::ApiError;
use crate::search::{
    add_filter, attachment_icon, attachment_kind, autocomplete_context, build_search_query,
    build_suggestions, byte_to_utf16, commit_typed_tokens, default_active, excerpt_runs,
    extract_search_terms, flatten_suggestions, group_results_by_channel, has_searchable_input,
    member_label, recent_search_label, result_stamps, save_recent_search, splice_out,
    suggestions_open, utf16_to_byte, Autocomplete, Clock, RecentSearch, SearchChannel,
    SearchFilter, SearchMember, SearchQuery, SearchResponse, Sources, Suggestion, SuggestionGroup,
    SuggestionSources,
};
use crate::store::search::{
    ChipRow, PillRow, ResultGroupView, ResultRow, SearchPending, SearchRequest, SearchScope,
    SearchState, SearchStatus, SearchView, SuggestionGroupView, SuggestionRow,
};
use crate::store::state::State;
use crate::store::view::{absolute_url, avatar_url, initials, ViewUpdate};
use crate::store::{ApiRequest, ApiResponse, Effect, Input, RequestId, SearchCommand};

/// SearchPanel.tsx `DEBOUNCE_MS`.
pub const SEARCH_DEBOUNCE: Duration = Duration::from_millis(350);

/// The highest coloured, non-default role's colour (`getRoleNameColor`).
fn role_color(roles: &[yap_protocol::rest::Role]) -> String {
    roles
        .iter()
        .filter(|r| !r.is_default && r.color.as_deref().is_some_and(|c| !c.is_empty()))
        .reduce(|best, r| if r.position > best.position { r } else { best })
        .and_then(|r| r.color.clone())
        .unwrap_or_default()
}

fn search_response(result: Result<ApiResponse, ApiError>) -> Result<SearchResponse, ApiError> {
    match result? {
        ApiResponse::Search(response) => Ok(*response),
        _ => Err(ApiError::Decode("unexpected response".into())),
    }
}

impl State {
    fn clock(&self) -> Clock {
        Clock {
            now_ms: self.search.now_ms,
            offset: self.local_offset,
        }
    }

    /// Where search looks from the open conversation (ChatArea.tsx
    /// `searchScope`): the DM itself, else the shown server; and its label.
    fn search_scope(&self) -> Option<(SearchScope, String)> {
        let channel = self.active_channel.as_deref()?;
        if let Some(dm) = self.dm(channel) {
            let label = match dm.recipient.label() {
                "" => dm.name.clone(),
                label => label.to_owned(),
            };
            return Some((SearchScope::Channel(channel.to_owned()), label));
        }
        let server = self.active_server.clone()?;
        let name = self.active_server_name();
        let label = if name.is_empty() {
            "this server".to_owned()
        } else {
            name
        };
        Some((SearchScope::Server(server), label))
    }

    /// People `from:` and `mentions:` offer (`useSearchSources`): the
    /// server's members, or in a DM the two of you.
    fn search_members(&self) -> Vec<SearchMember> {
        let member = |user: &yap_protocol::rest::User, color: String| SearchMember {
            id: user.id.clone(),
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            avatar: avatar_url(
                &self.api_base,
                user.avatar_url.as_deref(),
                user.avatar_display.as_ref(),
            ),
            initials: initials(user.label()),
            role_color: color,
        };
        match &self.search.scope {
            Some(SearchScope::Server(id)) => self
                .members
                .get(id)
                .map(|list| {
                    list.iter()
                        .map(|m| member(&m.user, role_color(&m.roles)))
                        .collect()
                })
                .unwrap_or_default(),
            Some(SearchScope::Channel(id)) => {
                let recipient = self.dm(id).map(|dm| &dm.recipient);
                self.me()
                    .into_iter()
                    .chain(recipient)
                    .map(|u| member(u, String::new()))
                    .collect()
            }
            None => Vec::new(),
        }
    }

    /// Text and voice channels of the server (none in a DM).
    fn search_channels(&self) -> Vec<SearchChannel> {
        let Some(SearchScope::Server(id)) = &self.search.scope else {
            return Vec::new();
        };
        self.channels
            .get(id)
            .map(|tree| {
                tree.channels
                    .iter()
                    .filter(|c| matches!(c.channel_type, ChannelType::Text | ChannelType::Voice))
                    .map(|c| SearchChannel {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        voice: c.channel_type == ChannelType::Voice,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn scope_recents(&self) -> &[RecentSearch] {
        self.search
            .scope
            .as_ref()
            .and_then(|scope| self.search.recents.get(&scope.key()))
            .map_or(&[], Vec::as_slice)
    }

    fn context(&self) -> Option<Autocomplete> {
        autocomplete_context(&self.search.text, self.search.caret)
    }

    /// Runs `f` with the sources suggestions and token resolution use.
    fn with_sources<T>(&self, f: impl FnOnce(&SuggestionSources<'_>, &Sources<'_>) -> T) -> T {
        let members = self.search_members();
        let channels = self.search_channels();
        let clock = self.clock();
        let suggest = SuggestionSources {
            members: &members,
            channels: &channels,
            clock,
            allow_channel_filter: matches!(self.search.scope, Some(SearchScope::Server(_))),
            recent: self.scope_recents(),
        };
        let resolve = Sources {
            members: &members,
            channels: &channels,
            clock,
        };
        f(&suggest, &resolve)
    }

    fn suggestion_groups(
        &self,
        context: Option<&Autocomplete>,
        recents: bool,
    ) -> Vec<SuggestionGroup> {
        self.with_sources(|sources, _| {
            let sources = SuggestionSources {
                recent: if recents { sources.recent } else { &[] },
                ..*sources
            };
            build_suggestions(context, &sources)
        })
    }

    fn idle_context() -> Autocomplete {
        Autocomplete::Key {
            query: String::new(),
            start: 0,
            end: 0,
        }
    }

    /// The suggestions on screen are open (`suggestionsOpen`).
    fn search_suggestions_open(&self) -> bool {
        let context = self.context();
        let groups = self.suggestion_groups(context.as_ref(), false);
        suggestions_open(
            self.search.focused,
            self.search.dismissed,
            context.as_ref(),
            &groups,
        )
    }

    fn stamp(&mut self, now_ms: i64) {
        self.search.now_ms = now_ms;
    }

    /// `updateText`: the field, un-dismissed, the first suggestion lit.
    fn update_text(&mut self, text: String, caret: usize) {
        self.search.text = text;
        self.search.caret = caret.min(self.search.text.len());
        self.search.dismissed = false;
        let context = self.context();
        self.search.active_id =
            self.with_sources(|sources, _| default_active(context.as_ref(), sources));
    }

    /// The store rewrote the field (`placeCaret`).
    fn rewrite_text(&mut self, text: String, caret: usize) {
        self.update_text(text, caret);
        self.search.revision += 1;
    }

    // Commands

    pub(super) fn on_search(&mut self, fx: &mut Effects, command: SearchCommand) {
        if !self.search.open && !matches!(command, SearchCommand::Open { .. }) {
            return;
        }
        match command {
            SearchCommand::Open {
                text,
                now_ms,
                recents,
            } => self.open_search(fx, text, now_ms, recents),
            SearchCommand::Close => self.close_search(),
            SearchCommand::Edit {
                text,
                caret,
                now_ms,
            } => {
                self.stamp(now_ms);
                let caret = utf16_to_byte(&text, caret);
                self.edit_search(fx, text, caret);
            }
            SearchCommand::Caret(caret) => {
                self.search.caret = utf16_to_byte(&self.search.text, caret);
            }
            SearchCommand::Focus(focused) => self.search.focused = focused,
            SearchCommand::Move(delta) => self.move_active(delta),
            SearchCommand::Hover(id) => self.search.active_id = Some(id),
            SearchCommand::Pick { id, now_ms } => {
                self.stamp(now_ms);
                self.pick(fx, &id);
            }
            SearchCommand::RemoveFilter(id) => self.remove_filter(fx, &id),
            SearchCommand::RemoveLastFilter => {
                if let Some(id) = self.search.filters.last().map(SearchFilter::id) {
                    self.remove_filter(fx, &id);
                }
            }
            SearchCommand::Clear => {
                self.search.filters.clear();
                self.rewrite_text(String::new(), 0);
                self.run_now(fx);
            }
            SearchCommand::Submit { now_ms } => {
                self.stamp(now_ms);
                self.submit(fx);
            }
            SearchCommand::Dismiss => self.search.dismissed = true,
            SearchCommand::SetSort(sort) => {
                if sort != self.search.sort {
                    self.search.sort = sort;
                    self.run_now(fx);
                }
            }
            SearchCommand::LoadMore => self.load_more(fx),
            SearchCommand::Retry => self.retry(fx),
            SearchCommand::Debounced(generation) => {
                if generation == self.search.debounce && self.search.debounce_live {
                    self.search.debounce_live = false;
                    self.search.active_result_id = None;
                    self.run_search(fx);
                }
            }
            SearchCommand::Opened(message_id) => self.opened(message_id),
        }
    }

    fn open_search(
        &mut self,
        fx: &mut Effects,
        text: String,
        now_ms: i64,
        recents: std::collections::BTreeMap<String, Vec<RecentSearch>>,
    ) {
        let scope = self.search_scope();
        let same = self.search.open
            && scope.as_ref().map(|(s, _)| s) == self.search.scope.as_ref()
            && text.is_empty();
        self.search.recents = recents;
        self.stamp(now_ms);
        if same {
            return;
        }
        self.reset_search(scope);
        // Complete tokens in handed-over text (`from:ada token`) open as chips.
        let initial = self.with_sources(|_, sources| commit_typed_tokens(&text, &[], sources));
        self.search.filters = initial.filters;
        let caret = initial.text.len();
        self.rewrite_text(initial.text, caret);
        // A half-typed filter (`from:`) waits for a pick instead of searching.
        if matches!(self.context(), Some(Autocomplete::Value { .. })) {
            return;
        }
        if has_searchable_input(&self.search.text, &self.search.filters) {
            self.run_search(fx);
        }
    }

    /// A fresh panel for `scope` (SearchPanel.tsx keys the body by scope).
    fn reset_search(&mut self, scope: Option<(SearchScope, String)>) {
        let recents = std::mem::take(&mut self.search.recents);
        let (now_ms, revision, generation) = (
            self.search.now_ms,
            self.search.revision,
            self.search.generation,
        );
        let (scope, scope_label) = scope.map_or((None, String::new()), |(s, l)| (Some(s), l));
        self.search = SearchState {
            open: true,
            scope,
            scope_label,
            now_ms,
            revision: revision + 1,
            generation: generation + 1,
            recents,
            ..SearchState::default()
        };
        self.pending.search = None;
    }

    fn close_search(&mut self) {
        let recents = std::mem::take(&mut self.search.recents);
        self.search = SearchState {
            recents,
            ..SearchState::default()
        };
        self.pending.search = None;
    }

    /// Sign-out: nothing searched by one account shows for the next.
    pub(super) fn clear_search(&mut self) {
        self.search = SearchState::default();
        self.pending.search = None;
    }

    /// `handleChange`.
    fn edit_search(&mut self, fx: &mut Effects, text: String, caret: usize) {
        // A finished token (`from:ada `) becomes a chip as soon as it is typed.
        if text.ends_with(char::is_whitespace) && caret >= text.len() {
            let committed = self.with_sources(|_, sources| {
                commit_typed_tokens(&text, &self.search.filters, sources)
            });
            if committed.committed > 0 {
                self.search.filters = committed.filters;
                let end = committed.text.len();
                self.rewrite_text(committed.text, end);
                self.run_now(fx);
                return;
            }
        }
        self.update_text(text, caret);
        self.search.debounce_live = false;
        if matches!(self.context(), Some(Autocomplete::Value { .. })) {
            // Still choosing a filter value.
            return;
        }
        self.search.debounce += 1;
        self.search.debounce_live = true;
        fx.push(Effect::Later {
            delay: SEARCH_DEBOUNCE,
            input: Box::new(Input::User(crate::store::Command::Search(
                SearchCommand::Debounced(self.search.debounce),
            ))),
        });
    }

    fn move_active(&mut self, delta: i32) {
        if !self.search_suggestions_open() {
            return;
        }
        let context = self.context();
        let groups = self.suggestion_groups(context.as_ref(), false);
        let flat = flatten_suggestions(&groups);
        let n = i64::try_from(flat.len()).unwrap_or(i64::MAX);
        let index = self
            .search
            .active_id
            .as_ref()
            .and_then(|id| flat.iter().position(|s| &s.id() == id))
            .map(|i| i64::try_from(i).unwrap_or_default());
        let delta = i64::from(delta.signum());
        let base = index.unwrap_or(if delta == 1 { -1 } else { 0 });
        let next = (base + delta).rem_euclid(n);
        self.search.active_id = usize::try_from(next)
            .ok()
            .and_then(|i| flat.get(i))
            .map(|s| s.id());
    }

    /// `applySuggestion`, for a row of the open list or the idle list.
    fn pick(&mut self, fx: &mut Effects, id: &str) {
        let context = self.context();
        let found = |groups: Vec<SuggestionGroup>| {
            groups
                .into_iter()
                .flat_map(|g| g.items)
                .find(|s| s.id() == id)
        };
        let suggestion = found(self.suggestion_groups(context.as_ref(), false))
            .or_else(|| found(self.suggestion_groups(Some(&Self::idle_context()), true)));
        let Some(suggestion) = suggestion else {
            return;
        };
        match suggestion {
            Suggestion::Recent { recent, .. } => {
                self.search.filters = recent.filters;
                let end = recent.text.len();
                self.rewrite_text(recent.text, end);
                self.search.dismissed = true;
                self.run_now(fx);
            }
            Suggestion::Key(key) => {
                let insert = format!("{}:", key.as_str());
                let text = &self.search.text;
                let (next, caret) = match &context {
                    Some(Autocomplete::Key { start, end, .. }) => (
                        format!("{}{insert}{}", &text[..*start], &text[*end..]),
                        start + insert.len(),
                    ),
                    _ => {
                        let base = text.trim_end();
                        let next = if base.is_empty() {
                            insert
                        } else {
                            format!("{base} {insert}")
                        };
                        let caret = next.len();
                        (next, caret)
                    }
                };
                self.rewrite_text(next, caret);
            }
            value => {
                let Some(filter) = value.to_filter() else {
                    return;
                };
                self.search.filters = add_filter(&self.search.filters, filter);
                let next = match &context {
                    Some(Autocomplete::Value { start, end, .. }) => {
                        splice_out(&self.search.text, *start, *end)
                    }
                    _ => self.search.text.clone(),
                };
                let end = next.len();
                self.rewrite_text(next, end);
                self.search.dismissed = true;
                self.run_now(fx);
            }
        }
    }

    fn remove_filter(&mut self, fx: &mut Effects, id: &str) {
        self.search.filters.retain(|f| f.id() != id);
        self.run_now(fx);
    }

    /// `submit`: commit what resolves, close the list, search now.
    fn submit(&mut self, fx: &mut Effects) {
        let committed = self.with_sources(|_, sources| {
            commit_typed_tokens(&self.search.text, &self.search.filters, sources)
        });
        if committed.committed > 0 {
            self.search.filters = committed.filters;
            let end = committed.text.len();
            self.rewrite_text(committed.text, end);
        }
        self.search.dismissed = true;
        self.run_now(fx);
    }

    /// `handleJump`: the search goes on top of this scope's recents.
    fn opened(&mut self, message_id: String) {
        self.search.active_result_id = Some(message_id);
        let Some(key) = self.search.scope.as_ref().map(SearchScope::key) else {
            return;
        };
        let next = RecentSearch {
            text: self.search.results_text.clone(),
            filters: self.search.filters.clone(),
        };
        let current = self.search.recents.get(&key).cloned().unwrap_or_default();
        self.search
            .recents
            .insert(key, save_recent_search(&current, next));
    }

    // Requests (useMessageSearch.ts)

    /// `runNow`: no pending debounce, no lit result, search.
    fn run_now(&mut self, fx: &mut Effects) {
        self.search.debounce_live = false;
        self.search.active_result_id = None;
        self.run_search(fx);
    }

    /// `run`: nothing to search resets the results.
    fn run_search(&mut self, fx: &mut Effects) {
        let s = &self.search;
        if !has_searchable_input(&s.text, &s.filters) {
            self.pending.search = None;
            let generation = s.generation + 1;
            let s = &mut self.search;
            s.last_query = None;
            s.status = SearchStatus::Idle;
            s.results.clear();
            s.total = 0;
            s.has_more = false;
            s.loading_more = false;
            s.load_more_failed = false;
            s.results_text.clear();
            s.generation = generation;
            return;
        }
        let query = build_search_query(&s.text, &s.filters, s.sort, 0, self.local_offset);
        self.search_request(fx, query, false);
    }

    fn search_request(&mut self, fx: &mut Effects, query: SearchQuery, append: bool) {
        let Some(scope) = self.search.scope.clone() else {
            return;
        };
        let request = ApiRequest::Search(SearchRequest {
            scope,
            query: query.clone(),
        });
        let Some(id) = self.http(fx, request) else {
            return;
        };
        self.pending.search = Some(SearchPending { id, append });
        let s = &mut self.search;
        if append {
            s.loading_more = true;
            s.load_more_failed = false;
        } else {
            s.status = SearchStatus::Loading;
            s.results.clear();
            s.total = 0;
            s.has_more = false;
            s.loading_more = false;
            s.load_more_failed = false;
            s.results_text.clone_from(&query.text);
            s.generation += 1;
        }
        s.last_query = Some(query);
    }

    fn load_more(&mut self, fx: &mut Effects) {
        let s = &self.search;
        let Some(last) = s.last_query.clone() else {
            return;
        };
        if !s.has_more || s.loading_more {
            return;
        }
        let offset = u32::try_from(s.results.len()).unwrap_or(u32::MAX);
        self.search_request(fx, SearchQuery { offset, ..last }, true);
    }

    fn retry(&mut self, fx: &mut Effects) {
        if let Some(last) = self.search.last_query.clone() {
            let append = last.offset > 0;
            self.search_request(fx, last, append);
        }
    }

    /// The search answer; `false` if `id` is not the one in flight (a
    /// newer search replaced it, or the panel closed: dropped).
    pub(super) fn route_search_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        result: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(pending) = self.pending.search.filter(|p| p.id == id) else {
            return false;
        };
        let Some(result) = result.take() else {
            return true;
        };
        self.pending.search = None;
        if let Err(error) = &result {
            if error.is_unauthorized() {
                self.fail(fx, error);
                return true;
            }
        }
        let s = &mut self.search;
        match (search_response(result), pending.append) {
            (Ok(page), true) => {
                let seen: std::collections::HashSet<String> =
                    s.results.iter().map(|m| m.id.clone()).collect();
                s.results
                    .extend(page.messages.into_iter().filter(|m| !seen.contains(&m.id)));
                s.loading_more = false;
                s.total = page.total;
                s.has_more = page.has_more;
            }
            (Ok(page), false) => {
                s.status = SearchStatus::Success;
                s.results = page.messages;
                s.total = page.total;
                s.has_more = page.has_more;
            }
            (Err(_), true) => {
                s.loading_more = false;
                s.load_more_failed = true;
            }
            (Err(_), false) => {
                s.status = SearchStatus::Error;
                s.results.clear();
                s.has_more = false;
            }
        }
        true
    }

    // The view

    fn chip_rows(&self, members: &[SearchMember], channels: &[SearchChannel]) -> Vec<ChipRow> {
        self.search
            .filters
            .iter()
            .map(|f| {
                let mut row = ChipRow {
                    id: f.id(),
                    key: f.key().as_str().into(),
                    value: f.value_label(),
                    ..ChipRow::default()
                };
                match f {
                    SearchFilter::From { user_id, label, .. }
                    | SearchFilter::Mentions { user_id, label, .. } => {
                        let member = members.iter().find(|m| &m.id == user_id);
                        row.person = member.is_some();
                        row.avatar = member.map(|m| m.avatar.clone()).unwrap_or_default();
                        row.initials =
                            member.map_or_else(|| initials(label), |m| m.initials.clone());
                    }
                    SearchFilter::In { channel_id, .. } => {
                        if let Some(channel) = channels.iter().find(|c| &c.id == channel_id) {
                            row.icon = channel_icon(channel.voice).into();
                        }
                    }
                    _ => {}
                }
                row
            })
            .collect()
    }

    fn result_groups(
        &self,
        members: &[SearchMember],
        channels: &[SearchChannel],
    ) -> Vec<ResultGroupView> {
        let s = &self.search;
        let terms = extract_search_terms(&s.results_text);
        let clock = self.clock();
        let server_members = match &s.scope {
            Some(SearchScope::Server(id)) => self.members.get(id).map(Vec::as_slice),
            _ => None,
        };
        group_results_by_channel(&s.results)
            .into_iter()
            .map(|(key, channel_id, channel_name, messages)| {
                let channel = channels.iter().find(|c| c.id == channel_id);
                ResultGroupView {
                    key,
                    channel_id: channel_id.to_owned(),
                    name: channel_name
                        .map(str::to_owned)
                        .or_else(|| channel.map(|c| c.name.clone()))
                        .unwrap_or_else(|| "Unknown channel".into()),
                    icon: channel_icon(channel.is_some_and(|c| c.voice)).into(),
                    items: messages
                        .into_iter()
                        .map(|m| {
                            // `useUserResolution`: the member as loaded, else
                            // the author the result carried.
                            let known = server_members
                                .and_then(|list| list.iter().find(|x| x.user.id == m.author_id))
                                .map(|x| &x.user);
                            let display = known
                                .and_then(|u| u.display_name.clone())
                                .or_else(|| m.author.display_name.clone());
                            let username = known
                                .map(|u| u.username.clone())
                                .filter(|u| !u.is_empty())
                                .unwrap_or_else(|| m.author.username.clone());
                            let name = match display.as_deref().map(str::trim) {
                                Some(d) if !d.is_empty() => d.to_owned(),
                                _ => username,
                            };
                            let avatar = known
                                .and_then(|u| u.avatar_url.clone())
                                .or_else(|| m.author.avatar_url.clone());
                            let name_style = crate::store::view::name_style(
                                known
                                    .and_then(|u| u.display_name_style.as_ref())
                                    .or(m.author.display_name_style.as_ref()),
                            );
                            let styled = !name_style.is_empty();
                            let color = members
                                .iter()
                                .find(|x| x.id == m.author_id)
                                .map(|x| x.role_color.clone())
                                .filter(|_| !styled)
                                .unwrap_or_default();
                            let (time, when) = result_stamps(&m.created_at, clock);
                            let has_text = !m.content.trim().is_empty();
                            ResultRow {
                                id: m.id.clone(),
                                channel_id: m.channel_id.clone(),
                                initials: initials(&name),
                                name,
                                color,
                                name_style,
                                avatar: absolute_url(&self.api_base, avatar.as_deref()),
                                time,
                                when,
                                pinned: m.is_pinned,
                                runs: if has_text {
                                    excerpt_runs(&m.content, &terms)
                                } else {
                                    Vec::new()
                                },
                                placeholder: if has_text {
                                    String::new()
                                } else if m.attachments.is_empty() {
                                    "No text".into()
                                } else {
                                    "Sent an attachment".into()
                                },
                                pills: m
                                    .attachments
                                    .iter()
                                    .take(2)
                                    .map(|a| PillRow {
                                        id: a.id.clone(),
                                        name: a.filename.clone(),
                                        icon: attachment_icon(
                                            &a.filename,
                                            a.content_type.as_deref(),
                                        )
                                        .into(),
                                        kind: attachment_kind(a.content_type.as_deref()).into(),
                                    })
                                    .collect(),
                                more: u64::try_from(m.attachments.len().saturating_sub(2))
                                    .unwrap_or_default(),
                                embeds: m.embed_count,
                                active: s.active_result_id.as_deref() == Some(m.id.as_str()),
                            }
                        })
                        .collect(),
                }
            })
            .collect()
    }

    fn search_view(&self) -> SearchView {
        let s = &self.search;
        if !s.open {
            return SearchView::default();
        }
        let members = self.search_members();
        let channels = self.search_channels();
        let context = self.context();
        let groups = self.suggestion_groups(context.as_ref(), false);
        let open = suggestions_open(s.focused, s.dismissed, context.as_ref(), &groups);
        let idle = self.suggestion_groups(Some(&Self::idle_context()), true);
        let announcement = match s.status {
            SearchStatus::Loading => "Searching".to_owned(),
            SearchStatus::Error => "Search failed".to_owned(),
            SearchStatus::Success if s.total == 0 => "No results".to_owned(),
            SearchStatus::Success if s.total == 1 => "1 result".to_owned(),
            SearchStatus::Success => format!("{} results", s.total),
            SearchStatus::Idle => String::new(),
        };
        SearchView {
            open: true,
            scope: match &s.scope {
                Some(SearchScope::Server(_)) => "server",
                Some(SearchScope::Channel(_)) => "channel",
                None => "",
            }
            .into(),
            scope_key: s.scope.as_ref().map(SearchScope::key).unwrap_or_default(),
            scope_label: s.scope_label.clone(),
            placeholder: if s.filters.is_empty() {
                format!("Search {}", s.scope_label)
            } else {
                "Add words or filters".into()
            },
            text: s.text.clone(),
            caret: byte_to_utf16(&s.text, s.caret),
            revision: s.revision,
            chips: self.chip_rows(&members, &channels),
            suggestions_open: open,
            active_id: if open {
                s.active_id.clone().unwrap_or_default()
            } else {
                String::new()
            },
            groups: if open {
                group_views(&groups)
            } else {
                Vec::new()
            },
            idle: group_views(&idle),
            status: s.status.as_str().into(),
            total: s.total,
            has_more: s.has_more,
            loading_more: s.loading_more,
            load_more_failed: s.load_more_failed,
            generation: s.generation,
            sort: s.sort.as_str().into(),
            announcement,
            results: self.result_groups(&members, &channels),
            empty_description: if s.filters.is_empty() {
                "Try other words, or a filter like from: or has:.".into()
            } else {
                "Nothing matches every filter. Remove one or try other words.".into()
            },
            filtered: !s.filters.is_empty(),
            recents: self.scope_recents().to_vec(),
        }
    }

    /// Follows the open conversation (another server or DM starts a fresh
    /// search) and sends the view when it changed.
    pub(super) fn sync_search(&mut self, fx: &mut Effects) {
        if self.search.open {
            let scope = self.search_scope();
            if scope.as_ref().map(|(s, _)| s) != self.search.scope.as_ref() {
                self.reset_search(scope);
            } else if let Some((_, label)) = scope {
                self.search.scope_label = label;
            }
        }
        let view = self.search_view();
        if view == self.view.search {
            return;
        }
        self.view.search = view.clone();
        fx.view(ViewUpdate::Search(Box::new(view)));
    }
}

fn channel_icon(voice: bool) -> &'static str {
    if voice {
        "voiceChannel"
    } else {
        "textChannel"
    }
}

fn group_views(groups: &[SuggestionGroup]) -> Vec<SuggestionGroupView> {
    groups
        .iter()
        .map(|g| SuggestionGroupView {
            id: g.id.into(),
            heading: g.heading.into(),
            items: g.items.iter().map(suggestion_row).collect(),
        })
        .collect()
}

/// SuggestionContent.tsx, as data.
fn suggestion_row(suggestion: &Suggestion) -> SuggestionRow {
    let row = SuggestionRow {
        id: suggestion.id(),
        ..SuggestionRow::default()
    };
    match suggestion {
        Suggestion::Key(key) => SuggestionRow {
            kind: "key".into(),
            label: format!("{}:", key.as_str()),
            hint: key.hint().into(),
            icon: key.icon().into(),
            ..row
        },
        Suggestion::User { member, .. } => SuggestionRow {
            kind: "user".into(),
            label: member_label(member).into(),
            meta: format!("@{}", member.username),
            avatar: member.avatar.clone(),
            initials: member.initials.clone(),
            color: member.role_color.clone(),
            ..row
        },
        Suggestion::Channel(channel) => SuggestionRow {
            kind: "channel".into(),
            label: channel.name.clone(),
            meta: if channel.voice {
                "Voice".into()
            } else {
                String::new()
            },
            icon: channel_icon(channel.voice).into(),
            ..row
        },
        Suggestion::Has(value) => {
            let (icon, hint) = value.meta();
            SuggestionRow {
                kind: "has".into(),
                label: value.as_str().into(),
                meta: hint.into(),
                icon: icon.into(),
                ..row
            }
        }
        Suggestion::Date { date, label, .. } => SuggestionRow {
            kind: "date".into(),
            label: label.clone(),
            meta: if label == date {
                String::new()
            } else {
                date.clone()
            },
            icon: "calendar".into(),
            ..row
        },
        Suggestion::Pinned(value) => SuggestionRow {
            kind: "pinned".into(),
            label: if *value { "Yes" } else { "No" }.into(),
            meta: if *value {
                "Only pinned"
            } else {
                "Exclude pinned"
            }
            .into(),
            icon: "pin".into(),
            ..row
        },
        Suggestion::Recent { recent, .. } => SuggestionRow {
            kind: "recent".into(),
            label: recent_search_label(recent),
            icon: "clock".into(),
            ..row
        },
    }
}
