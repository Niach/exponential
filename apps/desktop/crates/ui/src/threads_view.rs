//! EXP-1294 — the CHATS page ([`Screen::Threads`], the rail's `Chats`
//! entry; thread 58cb32df: the product says chats). #EXP-1380 (chats.json
//! WIDE): the page = THE Chats list of the active team, the same pieces as
//! the phones ([`crate::chat_list`]): the agent bubbles row, the filter
//! chips, then the flat rows, one centred column. A row opens the chat, its
//! right-click = THE chat menu ([`chat_row_menu`], `menuCases`); a bubble
//! opens the agent's chat, its right-click the agent menu.
//!
//! The helpers below still paint the rail's rows and the styleguide's
//! `thread-parts` specimen: bands Needs you · Running · Recent
//! (`domain::session_tree::partition_threads`, through the memoised
//! [`crate::session_data::thread_lists`]), each a glass band with its glyph
//! (no count) over flat gapless ONE-line rows ([`crate::pr_rows::pr_row`],
//! no dividers, EXP-1320): the lead (EXP-1344: the amber badge when it
//! needs you beats the working mark while busy; else the root's PR glyph:
//! open emerald, merged violet, else the root session's agent mark) · the title (EXP-1316: it
//! names the issue already, no identifier slot) · the contract's quiet word
//! (`domain::session_tree::thread_row_word`; EXP-1339: no lock, and
//! `merged` draws no word, the violet lead says it) · the device icon. A row
//! opens the thread. EXP-1323: a live tree nests its children under the
//! root (tree guides; a child opens its session), an unread title is
//! semibold.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    px, App, ClickEvent, InteractiveElement as _, Entity, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::{AgentBubble, ChatFilter, ChatRow, CHATS};
use domain::rows::{PullRequestRow, SessionRow};
use domain::session_tree::{
    thread_in_stack, thread_row_mark_state, thread_row_word, SessionState, StackPr, ThreadRowWord, ThreadsListGroup, ThreadsListKey, ThreadsListRow,
    THREADS_LIST,
};

use crate::scroll_pane::page_scaffold_with;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation, Screen};
use crate::pr_rows::{pr_row, PrRowSpec};
use crate::session_data::{self, PrSnapshot};

/// The page column's cap — the list language's `max-w-3xl` (short rows, a
/// queue, not a table).
const THREADS_COLUMN_W: f32 = 768.;

/// A row's lead glyph.
const LEAD_PX: f32 = 14.;
/// The amber needs-you dot on a row's lead.
const LEAD_BADGE_PX: f32 = 6.;

/// The pull request a row speaks for: the root's pull requests through THE
/// Guide rule (`domain::session_tree::guide_pull_request`, EXP-1306).
pub(crate) fn root_pull_request<'a>(root_id: &str, pulls: &'a [PullRequestRow]) -> Option<&'a PullRequestRow> {
    domain::session_tree::guide_pull_request(
        pulls
            .iter()
            .filter(|pull| pull.root_session_id.as_deref() == Some(root_id)),
    )
}

/// EXP-1318: a thread row's quiet word ×4 (`threads-list.json`
/// `wordCases`): the root's pull requests, whether they sit in an open
/// stack (`stackCases`, over the team's pull requests) and the row's
/// busy / needs-you.
pub(crate) fn thread_word(root_id: &str, busy: bool, needs_you: bool, team_pulls: &[PullRequestRow]) -> Option<ThreadRowWord> {
    let own: Vec<StackPr> = team_pulls
        .iter()
        .filter(|pull| pull.root_session_id.as_deref() == Some(root_id))
        .map(StackPr::from_row)
        .collect();
    let team: Vec<StackPr> = team_pulls.iter().map(StackPr::from_row).collect();
    let in_stack = thread_in_stack(&own, &team);
    thread_row_word(own.iter().map(|pull| pull.state.as_str()), busy, needs_you, in_stack)
}

/// The lead of a thread row ×4 (threads-list.json, EXP-1344
/// `threadRowMarkState` first): needs you (the amber badge) beats busy (the
/// working mark); otherwise the root's Guide PR glyph (open emerald, merged
/// violet), else the root session's mark. NEW-41: needs you draws the brand
/// mark with the badge BEFORE the PR glyph, like web's `rowLead`.
pub(crate) fn thread_row_lead(
    root_id: &str,
    busy: bool,
    needs_you: bool,
    team_pulls: &[PullRequestRow],
    cx: &App,
) -> gpui::AnyElement {
    let root = session_data::session(root_id, cx);
    let working = thread_row_mark_state(needs_you, busy) == SessionState::Working;
    match root_pull_request(root_id, team_pulls) {
        _ if needs_you && root.is_some() => {
            let root = root.as_ref().expect("checked by the guard");
            crate::thread_screen::root_mark(root, false, true, LEAD_PX, LEAD_BADGE_PX)
        }
        _ if working => crate::coding_selects::agent_working_mark(root_agent(root.as_ref()), LEAD_PX).into_any_element(),
        Some(pull) => {
            let (glyph, tint) = if pull.is_merged() {
                (registry::PR_MERGED, crate::pr_rows::PR_MERGED_TINT)
            } else {
                (registry::PR_OPEN, theme::tokens::GREEN)
            };
            crate::thread_screen::with_needs_you_dot(
                Icon::new(glyph).with_size(px(LEAD_PX)).text_color(tint.to_hsla()),
                needs_you,
                LEAD_BADGE_PX,
            )
        }
        None => match root.as_ref() {
            Some(root) => crate::thread_screen::root_mark(root, false, needs_you, LEAD_PX, LEAD_BADGE_PX),
            None => Icon::new(registry::SETTINGS_AGENTS)
                .with_size(px(LEAD_PX))
                .text_color(cx.theme().muted_foreground)
                .into_any_element(),
        },
    }
}

/// EXP-1323: a nested child row's lead — that session's own mark (EXP-1344:
/// the amber badge while it needs you beats the working mark while busy).
fn child_row_lead(session_id: &str, busy: bool, needs_you: bool, cx: &App) -> gpui::AnyElement {
    match session_data::session(session_id, cx) {
        Some(session) => crate::thread_screen::root_mark(&session, busy, needs_you, LEAD_PX, LEAD_BADGE_PX),
        None => Icon::new(registry::SETTINGS_AGENTS)
            .with_size(px(LEAD_PX))
            .text_color(cx.theme().muted_foreground)
            .into_any_element(),
    }
}

/// EXP-1318: a thread row's trailing DEVICE ICON — the root session's
/// device resolved against the synced `devices` rows, its pick through the
/// device-icon concept map (the Devices page's glyph), the device's name as
/// the tooltip; an unknown or invisible device (or none) draws nothing.
pub(crate) fn thread_device_icon(device_id: Option<&str>, row_key: &str, cx: &App) -> Option<gpui::AnyElement> {
    use gpui::{InteractiveElement as _, StatefulInteractiveElement as _};
    let device = crate::entity_preview::find_device(device_id?, cx)?;
    let name = device
        .label
        .clone()
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| "Device".to_string());
    let glyph = crate::icons::device_icon(device.icon.as_deref(), device.is_server());
    Some(
        gpui::div()
            .id(SharedString::from(format!("thread-device-{row_key}")))
            .flex_shrink_0()
            .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(name.clone()).build(window, cx))
            .child(Icon::new(glyph).xsmall().text_color(cx.theme().muted_foreground))
            .into_any_element(),
    )
}

/// The row's trailing slot: the quiet word (the stack word carries the
/// stack glyph in front; EXP-1339: `Merged` has no text, the violet lead
/// says it), then the device icon; `None` when both are absent.
pub(crate) fn thread_row_trailing(
    word: Option<ThreadRowWord>,
    device: Option<gpui::AnyElement>,
    cx: &App,
) -> Option<gpui::AnyElement> {
    let muted = cx.theme().muted_foreground;
    let word = word.and_then(|word| word.text().map(|text| (word, text)));
    if word.is_none() && device.is_none() {
        return None;
    }
    Some(
        h_flex()
            .flex_shrink_0()
            .gap_2()
            .items_center()
            .children(word.map(|(word, text)| {
                h_flex()
                    .flex_shrink_0()
                    .gap_1()
                    .items_center()
                    .text_size(px(11.5))
                    .text_color(muted)
                    .when(word == ThreadRowWord::Stack, |this| {
                        this.child(Icon::new(registry::PR_STACK).xsmall().text_color(muted))
                    })
                    .child(text)
            }))
            .children(device)
            .into_any_element(),
    )
}

/// The band glyph ×4: Needs you = coding-needs-input amber, Running = the
/// working mark, Recent = ui-check.
fn band_glyph(key: ThreadsListKey, cx: &App) -> gpui::AnyElement {
    let quiet = cx.theme().foreground.opacity(0.7);
    match key {
        ThreadsListKey::NeedsYou => Icon::new(registry::CODING_NEEDS_INPUT)
            .xsmall()
            .flex_shrink_0()
            .text_color(theme::tokens::YELLOW.to_hsla())
            .into_any_element(),
        ThreadsListKey::Running => crate::coding_selects::agent_mark(coding::CodingAgent::default())
            .xsmall()
            .flex_shrink_0()
            .into_any_element(),
        ThreadsListKey::Recent => Icon::new(registry::UI_CHECK)
            .xsmall()
            .flex_shrink_0()
            .text_color(quiet)
            .into_any_element(),
    }
}

/// One row of the Threads page (the styleguide's `thread-parts` specimen
/// draws the same): the lead, the title, the trailing word + device icon.
fn thread_list_row(
    row: &ThreadsListRow,
    guides: domain::tree_guides::Guides,
    snapshot: &PrSnapshot,
    cx: &App,
) -> gpui::AnyElement {
    // EXP-1323: a nested child = its own session's mark, no word; it
    // opens that session (a thread row opens the thread).
    let child = row.depth > 0;
    let lead = if child {
        child_row_lead(&row.session_id, row.busy, row.needs_you, cx)
    } else {
        thread_row_lead(&row.session_id, row.busy, row.needs_you, &snapshot.pulls, cx)
    };
    let word = (!child)
        .then(|| thread_word(&row.session_id, row.busy, row.needs_you, &snapshot.pulls))
        .flatten();
    let device = thread_device_icon(row.device_id.as_deref(), &row.session_id, cx);
    let trailing = thread_row_trailing(word, device, cx);
    let session_id = row.session_id.clone();
    let element = pr_row(
        PrRowSpec {
            lead: Some(lead),
            trailing,
            guides,
            strong: row.unread,
            on_click: Some(Box::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                let session_id = session_id.clone();
                let screen = if child {
                    Screen::Session { session_id }
                } else {
                    Screen::Thread { session_id }
                };
                crate::navigation::navigate(window, cx, screen);
            })),
            ..PrRowSpec::open(
                format!("{}-{}", THREADS_LIST.test_ids.row, row.session_id),
                row.title.clone(),
            )
        },
        cx,
    );
    if child {
        return element;
    }
    chat_row_menu(&row.session_id, element, cx)
}

/// Thread 58cb32df (chats.json MENUS): a chat row's right-click = THE chat
/// menu (`threadMenuEntries`); a viewer who cannot steer it gets none.
pub(crate) fn chat_row_menu(root_id: &str, element: gpui::AnyElement, cx: &App) -> gpui::AnyElement {
    use crate::controls::PointerContextMenuExt as _;
    if crate::chat_actions::chat_entries(root_id, cx).is_empty() {
        return element;
    }
    let menu_root = root_id.to_string();
    gpui::div()
        .id(SharedString::from(format!("chat-row-menu-{root_id}")))
        .w_full()
        .min_w_0()
        .child(element)
        .pointer_context_menu(move |menu, _, cx| crate::chat_actions::chat_menu_items(menu.min_w(px(220.)), &menu_root, false, cx))
        .into_any_element()
}

/// The Threads page's bands: per group its glass band (glyph + title, no
/// count) over its rows, the nested children's tree guides off the band's
/// depth sequence (TreeGuides, 14px/level, the corner tee; EXP-1323). The
/// page and the styleguide's `thread-parts` specimen both draw this.
pub(crate) fn thread_list_bands(
    groups: &[ThreadsListGroup],
    snapshot: &PrSnapshot,
    cx: &App,
) -> Vec<gpui::AnyElement> {
    groups
        .iter()
        .map(|group| {
            let band = crate::surface::glass_section_band(
                Some(band_glyph(group.key, cx)),
                SharedString::from(group.title.clone()),
                None,
                cx,
            );
            let guides = domain::tree_guides::guides_for(&group.rows.iter().map(|row| row.depth).collect::<Vec<_>>());
            let rows: Vec<gpui::AnyElement> = group
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| thread_list_row(row, guides.get(index).cloned().unwrap_or_default(), snapshot, cx))
                .collect();
            v_flex().min_w_0().pb_2().child(band).children(rows).into_any_element()
        })
        .collect()
}

/// The agent CLI a root runs.
fn root_agent(root: Option<&SessionRow>) -> Option<coding::CodingAgent> {
    root.and_then(|root| root.cli.as_deref()).and_then(coding::CodingAgent::parse)
}

pub struct ThreadsView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// #EXP-1380: the selected filter chip.
    filter: ChatFilter,
    _subscriptions: Vec<Subscription>,
}

impl ThreadsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        subscriptions.extend(session_data::observe_session_collections(cx));
        Self {
            nav,
            scroll: ScrollHandle::new(),
            filter: ChatFilter::All,
            _subscriptions: subscriptions,
        }
    }
}

/// #EXP-1380: one Chats page row — THE list row ([`crate::chat_list`]),
/// hover-lit; a click opens the chat, a right-click its menu.
fn chats_page_row(row: &ChatRow, cx: &App) -> gpui::AnyElement {
    use gpui::StatefulInteractiveElement as _;
    let agent = root_agent(session_data::session(&row.session_id, cx).as_ref());
    let lead = crate::chat_list::chat_lead_disc(row.lead, agent, cx);
    let session_id = row.session_id.clone();
    let element = gpui::div()
        .id(SharedString::from(format!("{}-{}", CHATS.test_ids.chat_row, row.session_id)))
        .w_full()
        .min_w_0()
        .px_2()
        .rounded_lg()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            crate::navigation::navigate(window, cx, Screen::Thread { session_id: session_id.clone() });
        })
        .child(crate::chat_list::chat_row(row, lead, cx))
        .into_any_element();
    chat_row_menu(&row.session_id, element, cx)
}

/// #EXP-1380: a bubble's click opens the agent's chat, its right-click the
/// agent menu.
fn chats_page_bubble(index: usize, bubble: &AgentBubble, element: gpui::Div) -> gpui::AnyElement {
    use crate::controls::PointerContextMenuExt as _;
    use gpui::StatefulInteractiveElement as _;
    let open_id = bubble.agent.agent_id.clone();
    let menu_id = bubble.agent.agent_id.clone();
    gpui::div()
        .id((SharedString::from(CHATS.test_ids.agent_bubble.clone()), index))
        .cursor_pointer()
        .child(element)
        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            crate::navigation::navigate(window, cx, Screen::Agent { agent_id: open_id.clone() });
        })
        .pointer_context_menu(move |menu, _, cx| {
            crate::chat_actions::agent_menu_items(menu.min_w(px(220.)), &menu_id, cx)
        })
        .into_any_element()
}

impl Render for ThreadsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let ready = sync::Store::try_global(cx)
            .is_some_and(|store| store.collections().sessions.read(cx).is_ready());
        let team_id = active_team_id(&self.nav, cx);
        let model = team_id
            .as_deref()
            .map(|team_id| session_data::chats_model(team_id, cx))
            .unwrap_or_default();
        let filter = self.filter;
        let on_filter: crate::chat_list::OnFilter = {
            let view = cx.entity().downgrade();
            std::rc::Rc::new(move |filter, _window, cx| {
                view.update(cx, |view, cx| {
                    view.filter = filter;
                    cx.notify();
                })
                .ok();
            })
        };
        let column = if !ready {
            // Skeletons, never "Loading…" (feedback-states.json).
            v_flex().min_w_0().child(crate::feedback::skeleton_rows(
                "threads-skeleton",
                crate::feedback::SKELETON_ROWS,
                crate::settings::SETTINGS_ROW_H,
                cx,
            ))
        } else {
            let rows: Vec<&ChatRow> = model.list.rows.iter().filter(|row| filter.keeps(row)).collect();
            // The empty state for an empty filter too (web `ChatList`).
            let list = if rows.is_empty() {
                crate::controls::empty_state(
                    Icon::new(registry::NAV_THREADS),
                    crate::empty_state_copy::THREADS,
                    None,
                    cx,
                )
                .into_any_element()
            } else {
                v_flex()
                    .w_full()
                    .min_w_0()
                    .children(rows.into_iter().map(|row| chats_page_row(row, cx)))
                    .into_any_element()
            };
            v_flex()
                .min_w_0()
                .gap_4()
                .children(crate::chat_list::agent_bubbles_row(&model.bubbles, chats_page_bubble, cx))
                .child(crate::chat_list::chat_filter_chips(&model.list.counts, filter, Some(on_filter), cx))
                .child(list)
        };
        page_scaffold_with("threads-screen-scroll", &self.scroll, column, THREADS_COLUMN_W)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pull(id: &str, root: &str, state: &str, created: &str) -> PullRequestRow {
        serde_json::from_value(json!({
            "id": id, "root_session_id": root, "state": state, "created_at": created,
        }))
        .unwrap()
    }

    /// The row speaks for the newest OPEN pull request, else the newest
    /// merged one; a closed one never counts.
    #[test]
    fn the_root_pull_request_prefers_open_then_merged() {
        let pulls = vec![
            pull("p1", "r1", "merged", "2026-10-13T09:00:00.000Z"),
            pull("p2", "r1", "open", "2026-10-13T08:00:00.000Z"),
            pull("p3", "r1", "closed", "2026-10-13T10:00:00.000Z"),
            pull("p4", "r2", "open", "2026-10-13T11:00:00.000Z"),
        ];
        assert_eq!(root_pull_request("r1", &pulls).map(|pull| pull.id.as_str()), Some("p2"));
        let merged = vec![pulls[0].clone(), pulls[2].clone()];
        assert_eq!(root_pull_request("r1", &merged).map(|pull| pull.id.as_str()), Some("p1"));
        assert!(root_pull_request("r3", &pulls).is_none());
    }

    /// The word follows the contract: open in a stack → stack, open idle →
    /// ready to merge, merged, else a question while it needs you.
    /// EXP-1339: `merged` has no text (the violet lead says it).
    #[test]
    fn the_thread_word_follows_the_contract() {
        let mut stacked = pull("p2", "r1", "open", "2026-10-13T08:00:00.000Z");
        stacked.base_pull_request_id = Some("p1".into());
        let pulls = vec![
            pull("p1", "r0", "open", "2026-10-13T07:00:00.000Z"),
            stacked,
            pull("p3", "r2", "open", "2026-10-13T08:00:00.000Z"),
            pull("p4", "r3", "merged", "2026-10-13T08:00:00.000Z"),
        ];
        assert_eq!(thread_word("r1", false, false, &pulls), Some(ThreadRowWord::Stack));
        assert_eq!(thread_word("r2", false, true, &pulls), Some(ThreadRowWord::ReadyToMerge));
        assert_eq!(thread_word("r2", true, false, &pulls), None);
        assert_eq!(thread_word("r3", false, true, &pulls), Some(ThreadRowWord::Merged));
        assert_eq!(thread_word("r4", false, true, &pulls), Some(ThreadRowWord::Question));
        assert_eq!(thread_word("r4", false, false, &pulls), None);
        assert_eq!(ThreadRowWord::Merged.text(), None);
        assert_eq!(ThreadRowWord::ReadyToMerge.text(), Some("ready to merge"));
        assert_eq!(ThreadRowWord::Stack.text(), Some("stack"));
        assert_eq!(ThreadRowWord::Question.text(), Some("question"));
    }
}
