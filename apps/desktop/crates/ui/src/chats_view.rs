//! EXP-1294 — the CHATS page ([`Screen::Chats`], the rail's `Chats`
//! entry; chat 58cb32df: the product says chats). #EXP-1380 (chats.json
//! WIDE): the page = THE Chats list of the active team, the same pieces as
//! the phones ([`crate::chat_list`]), one centred column. #FEED-191: the Agents
//! section, then `chat_sections` under their headers (the Archived entry first
//! under Chats; the Archived page flat, no headers). A row opens the chat, its
//! right-click = THE chat menu (`menuCases`); selection follows the
//! displayed order.
//!
//! The helpers below still feed the rail's rows and the chat page: a row's
//! pull request ([`root_pull_request`]), the contract's quiet word
//! ([`chat_word`], `domain::session_tree::chat_row_word`; EXP-1339: no
//! lock, and `merged` draws no word) and the device icon
//! ([`chat_device_icon`]).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    px, App, ClickEvent, Entity, FocusHandle, InteractiveElement as _, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, Styled, Subscription, Window,
};
use gpui_component::{v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::{chat_selection_range, chat_sections, ChatListView, ChatRow, CHATS};
use domain::rows::{PullRequestRow, SessionRow};
use domain::session_tree::{chat_in_stack, chat_row_word_on, StackPr, ChatRowWord};

use crate::scroll_pane::page_scaffold_with;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation, Screen};
use crate::session_data;

/// The page column's cap — the list language's `max-w-3xl` (short rows, a
/// queue, not a table).
const CHATS_COLUMN_W: f32 = 768.;

/// The pull request a row speaks for: the root's pull requests through THE
/// Guide rule (`domain::session_tree::guide_pull_request`, EXP-1306).
pub(crate) fn root_pull_request<'a>(root_id: &str, pulls: &'a [PullRequestRow]) -> Option<&'a PullRequestRow> {
    domain::session_tree::guide_pull_request(
        pulls
            .iter()
            .filter(|pull| pull.root_session_id.as_deref() == Some(root_id)),
    )
}

/// EXP-1318: a chat row's quiet word ×4 (`chat-list.json`
/// `wordCases`): the root's pull requests, whether they sit in an open
/// stack (`stackCases`, over the team's pull requests) and the row's
/// busy / needs-you; #EXP-1703: `offline` = the row's
/// [`ChatsListRow::offline`] (`device offline` instead of `question`).
pub(crate) fn chat_word(
    root_id: &str,
    busy: bool,
    needs_you: bool,
    offline: bool,
    team_pulls: &[PullRequestRow],
) -> Option<ChatRowWord> {
    let own: Vec<StackPr> = team_pulls
        .iter()
        .filter(|pull| pull.root_session_id.as_deref() == Some(root_id))
        .map(StackPr::from_row)
        .collect();
    let team: Vec<StackPr> = team_pulls.iter().map(StackPr::from_row).collect();
    let in_stack = chat_in_stack(&own, &team);
    chat_row_word_on(own.iter().map(|pull| pull.state.as_str()), busy, needs_you, in_stack, offline)
}

/// EXP-1318: a chat row's trailing DEVICE ICON — the root session's
/// device resolved against the synced `devices` rows, its pick through the
/// device-icon concept map (the Devices page's glyph), the device's name as
/// the tooltip; an unknown or invisible device (or none) draws nothing.
pub(crate) fn chat_device_icon(device_id: Option<&str>, row_key: &str, cx: &App) -> Option<gpui::AnyElement> {
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
            .id(SharedString::from(format!("chat-device-{row_key}")))
            .flex_shrink_0()
            .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(name.clone()).build(window, cx))
            .child(Icon::new(glyph).xsmall().text_color(cx.theme().muted_foreground))
            .into_any_element(),
    )
}

/// The agent CLI a root runs.
fn root_agent(root: Option<&SessionRow>) -> Option<coding::CodingAgent> {
    root.and_then(|root| root.cli.as_deref()).and_then(coding::CodingAgent::parse)
}

gpui::actions!(
    chats_view,
    [
        /// #EXP-1388 (cmd-a/ctrl-a): select every visible chat row, in
        /// list order (the issue list's `SelectAll`).
        SelectAllChats,
        /// #EXP-1388 (escape): leave the Chats selection.
        ClearChatSelection,
    ]
);

const KEY_CONTEXT: &str = "ChatsPage";

/// Register the Chats page's keys (call once from `ui::init`).
/// The issue list's bindings, scoped to the Chats page.
pub(crate) fn init(cx: &mut App) {
    const BINDING_CONTEXT: &str = "ChatsPage && !Input && !MarkdownEditor && !MentionInput && !Terminal";
    #[cfg(target_os = "macos")]
    cx.bind_keys([gpui::KeyBinding::new("cmd-a", SelectAllChats, Some(BINDING_CONTEXT))]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([gpui::KeyBinding::new("ctrl-a", SelectAllChats, Some(BINDING_CONTEXT))]);
    cx.bind_keys([gpui::KeyBinding::new("escape", ClearChatSelection, Some(BINDING_CONTEXT))]);
}

pub struct ChatsView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// #EXP-1388: the chats, or the Archived page.
    view: ChatListView,
    /// #EXP-1388: the multi-selection (selecting while non-empty).
    selected: Vec<String>,
    /// The last clicked row (shift-click's range start).
    anchor: Option<String>,
    focus_handle: FocusHandle,
    /// The page's measured width (the issue list's probe): the bar shows
    /// its labels at or above `chat_bar_label_min_width` (EXP-698's rule).
    measured_width: std::rc::Rc<std::cell::Cell<gpui::Pixels>>,
    /// #LOGS-3: a held list re-renders when it re-derives.
    held_wake: bool,
    _subscriptions: Vec<Subscription>,
}

impl ChatsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        subscriptions.extend(session_data::observe_session_collections(cx));
        let mut view = Self {
            nav,
            scroll: ScrollHandle::new(),
            view: ChatListView::List,
            selected: Vec::new(),
            anchor: None,
            focus_handle: cx.focus_handle(),
            measured_width: std::rc::Rc::new(std::cell::Cell::new(px(0.))),
            held_wake: false,
            _subscriptions: subscriptions,
        };
        view.apply_dev_selection();
        view
    }

    /// DEV-ONLY (the EXP_DEV_* family, never documented for users):
    /// `EXP_DEV_CHAT_SELECT=<id>,<id>` pre-selects those chats so a capture
    /// lands on the selection bar without synthetic input.
    fn apply_dev_selection(&mut self) {
        if let Ok(spec) = std::env::var("EXP_DEV_CHAT_SELECT") {
            self.selected = spec.split(',').map(str::trim).filter(|id| !id.is_empty()).map(str::to_string).collect();
        }
    }

    /// #FEED-191: the rows the page shows, in DISPLAYED order: the sections,
    /// the Archived page flat. Selection follows it.
    fn visible_rows(&self, model: &session_data::ChatsModel) -> Vec<ChatRow> {
        match self.view {
            ChatListView::List => chat_sections(&model.list.rows).into_iter().flat_map(|section| section.rows).collect(),
            ChatListView::Archived => model.archived.rows.clone(),
        }
    }

    fn model(&self, cx: &mut App) -> std::rc::Rc<session_data::ChatsModel> {
        active_team_id(&self.nav, cx)
            .as_deref()
            .map(|team_id| session_data::chats_model(team_id, cx))
            .unwrap_or_default()
    }

    fn clear_selection(&mut self, cx: &mut gpui::Context<Self>) {
        self.selected.clear();
        self.anchor = None;
        cx.notify();
    }

    /// Enter / grow the selection with `id` (the menu's Select, cmd-click).
    fn toggle(&mut self, id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(at) = self.selected.iter().position(|selected| selected == id) {
            self.selected.remove(at);
        } else {
            self.selected.push(id.to_string());
            window.focus(&self.focus_handle, cx);
        }
        self.anchor = Some(id.to_string());
        cx.notify();
    }

    /// A row click: shift = the range from the anchor; cmd/ctrl or while
    /// selecting = toggle; else open the chat.
    fn click_row(&mut self, id: &str, event: &ClickEvent, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let modifiers = event.modifiers();
        if modifiers.shift {
            let model = self.model(cx);
            let order: Vec<String> = self.visible_rows(&model).into_iter().map(|row| row.session_id).collect();
            for id in chat_selection_range(&order, self.anchor.as_deref(), id) {
                if !self.selected.contains(&id) {
                    self.selected.push(id);
                }
            }
            self.anchor = Some(id.to_string());
            window.focus(&self.focus_handle, cx);
            cx.notify();
            return;
        }
        if modifiers.secondary(cx) || !self.selected.is_empty() {
            self.toggle(id, window, cx);
            return;
        }
        crate::navigation::navigate(window, cx, Screen::Chat { session_id: id.to_string() });
    }

    fn set_view(&mut self, view: ChatListView, cx: &mut gpui::Context<Self>) {
        self.view = view;
        self.selected.clear();
        self.anchor = None;
        cx.notify();
    }
}

/// A row's click (`session_id`, the event).
type OnRowClick = std::rc::Rc<dyn Fn(&str, &ClickEvent, &mut Window, &mut App)>;
/// The menu's Select (`session_id`, false: only ever adds) and the checkbox
/// cell (true: toggles).
type OnSelect = std::rc::Rc<dyn Fn(&str, bool, &mut Window, &mut App)>;

/// #EXP-1380: one Chats page row — THE list row ([`crate::chat_list`]) in
/// the issue row's selectable frame ([`crate::chat_list::chat_selectable`],
/// #EXP-1388): the checkbox cell (hover-revealed, pinned while selecting;
/// its click toggles), the flat hover / selected fill; a click opens the
/// chat (or toggles it while selecting), a right-click its menu: Select,
/// then the chat's entries.
fn chats_page_row(
    row: &ChatRow,
    selected: bool,
    selecting: bool,
    on_click: OnRowClick,
    on_select: OnSelect,
    cx: &App,
) -> gpui::AnyElement {
    use crate::controls::PointerContextMenuExt as _;
    use gpui::StatefulInteractiveElement as _;
    let agent = root_agent(session_data::session(&row.session_id, cx).as_ref());
    let lead = crate::chat_list::chat_lead_disc(row.lead, agent, selected, cx);
    let session_id = row.session_id.clone();
    let toggle_id = row.session_id.clone();
    let menu_root = row.session_id.clone();
    let on_toggle = on_select.clone();
    crate::chat_list::chat_selectable(
        SharedString::from(format!("{}-{}", CHATS.test_ids.chat_row, row.session_id)),
        &row.session_id,
        Some((selected, selecting)),
        move |_: &ClickEvent, window: &mut Window, cx: &mut App| on_toggle(&toggle_id, true, window, cx),
        crate::chat_list::chat_row(row, lead, cx),
        cx,
    )
    .on_click(move |event: &ClickEvent, window: &mut Window, cx: &mut App| on_click(&session_id, event, window, cx))
    .pointer_context_menu(move |menu, _, cx| {
        let root = menu_root.clone();
        let on_select = on_select.clone();
        let menu = menu.min_w(px(220.)).item(crate::controls::pointer_menu_item(
            Some(registry::UI_SELECTED),
            CHATS.copy.selection.select.clone(),
            |_| None,
            move |window, cx| on_select(&root, false, window, cx),
        ));
        crate::chat_actions::chat_menu_items(menu, &menu_root, true, cx)
    })
    .into_any_element()
}

impl Render for ChatsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "ChatsView").entered();
        use gpui::StatefulInteractiveElement as _;
        let ready = sync::Store::try_global(cx)
            .is_some_and(|store| store.collections().sessions.read(cx).is_ready());
        // S-3: the viewer's Assistant is created on first use.
        if let Some(team_id) = active_team_id(&self.nav, cx) {
            session_data::ensure_assistant(&team_id, cx);
        }
        let model = self.model(cx);
        if let Some(at) = session_data::chats_model_stale_until(cx).filter(|_| !self.held_wake) {
            self.held_wake = true;
            let wait = at.saturating_duration_since(cx.background_executor().now());
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |this, cx| {
                    this.held_wake = false;
                    cx.notify();
                });
            })
            .detach();
        }
        let view_kind = self.view;
        let rows = self.visible_rows(&model);
        // The selection keeps only rows still in the list, in list order.
        if ready && !self.selected.is_empty() {
            let kept: Vec<String> = rows
                .iter()
                .filter(|row| self.selected.contains(&row.session_id))
                .map(|row| row.session_id.clone())
                .collect();
            if kept.len() != self.selected.len() || kept != self.selected {
                self.selected = kept;
            }
        }
        let selecting = !self.selected.is_empty();
        let entity = cx.entity().downgrade();
        let on_click: OnRowClick = {
            let view = entity.clone();
            std::rc::Rc::new(move |id, event, window, cx| {
                view.update(cx, |view, cx| view.click_row(id, event, window, cx)).ok();
            })
        };
        let on_select: OnSelect = {
            let view = entity.clone();
            std::rc::Rc::new(move |id, toggle, window, cx| {
                view.update(cx, |view, cx| {
                    if toggle || !view.selected.iter().any(|selected| selected == id) {
                        view.toggle(id, window, cx);
                    }
                })
                .ok();
            })
        };
        let column = if !ready {
            // Skeletons, never "Loading…" (feedback-states.json).
            v_flex().min_w_0().child(crate::feedback::skeleton_rows(
                "chats-skeleton",
                crate::feedback::SKELETON_ROWS,
                crate::settings::SETTINGS_ROW_H,
                cx,
            ))
        } else {
            let archived_entry = (view_kind == ChatListView::List && model.list.archived > 0)
                .then(|| {
                    let view = entity.clone();
                    crate::chat_list::chat_selectable(
                        SharedString::from(CHATS.test_ids.chat_archived.clone()),
                        "",
                        None,
                        |_, _, _| {},
                        crate::chat_list::chat_archived_row(model.list.archived, cx),
                        cx,
                    )
                    .on_click(move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                        view.update(cx, |view, cx| view.set_view(ChatListView::Archived, cx)).ok();
                    })
                    .into_any_element()
                });
            // S-3: the AGENT rows pinned on top, whatever their state; none on
            // the Archived page.
            let agent_rows: &[domain::chats::AgentChatRow] = match view_kind {
                ChatListView::Archived => &[],
                ChatListView::List => &model.agent_rows,
            };
            let agents_section = (!agent_rows.is_empty()).then(|| {
                v_flex()
                    .w_full()
                    .min_w_0()
                    .child(crate::chat_list::chat_section_header(&CHATS.copy.sections.agents, cx))
                    .children(agent_rows.iter().map(|row| {
                        let open_id = row.agent_id.clone();
                        let menu_id = row.agent_id.clone();
                        use crate::controls::PointerContextMenuExt as _;
                        use gpui::StatefulInteractiveElement as _;
                        crate::chat_list::chat_selectable(
                            SharedString::from(format!("{}-agent-{}", CHATS.test_ids.chat_row, row.agent_id)),
                            &row.agent_id,
                            None,
                            |_, _, _| {},
                            crate::chat_list::agent_chat_row(row, cx),
                            cx,
                        )
                        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                            crate::navigation::navigate(window, cx, Screen::Agent { agent_id: open_id.clone() });
                        })
                        .pointer_context_menu(move |menu, _, cx| {
                            crate::chat_actions::agent_menu_items(menu.min_w(px(220.)), &menu_id, cx)
                        })
                        .into_any_element()
                    }))
                    .into_any_element()
            });
            // #EXP-1733: the APP rows between the agents and the sessions
            // (never selectable); none on the Archived page. A click opens
            // the app's chat = its editor.
            let app_rows: &[domain::chats::AppChatRow] = match view_kind {
                ChatListView::Archived => &[],
                ChatListView::List => &model.app_rows,
            };
            let apps_section = (!app_rows.is_empty()).then(|| {
                v_flex()
                    .w_full()
                    .min_w_0()
                    .child(crate::chat_list::chat_section_header(&CHATS.copy.sections.apps, cx))
                    .children(app_rows.iter().map(|row| {
                        let open_id = row.app_id.clone();
                        crate::chat_list::chat_selectable(
                            SharedString::from(format!("{}-{}", CHATS.test_ids.app_row, row.app_id)),
                            &row.app_id,
                            None,
                            |_, _, _| {},
                            crate::chat_list::app_chat_row(row, cx),
                            cx,
                        )
                        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                            crate::navigation::navigate(window, cx, Screen::App { app: open_id.clone() });
                        })
                        .into_any_element()
                    }))
                    .into_any_element()
            });
            let paint = |row: &ChatRow, cx: &mut gpui::Context<Self>| {
                let selected = self.selected.contains(&row.session_id);
                chats_page_row(row, selected, selecting, on_click.clone(), on_select.clone(), cx)
            };
            let list = if rows.is_empty() && archived_entry.is_none() && agents_section.is_none() && apps_section.is_none() {
                let (icon, copy): (Icon, SharedString) = match view_kind {
                    ChatListView::Archived => (Icon::new(registry::UI_ARCHIVE), CHATS.copy.archived.empty.clone().into()),
                    ChatListView::List => (Icon::new(registry::NAV_CHATS), crate::empty_state_copy::CHATS.into()),
                };
                crate::controls::empty_state(icon, copy, None, cx).into_any_element()
            } else if view_kind == ChatListView::Archived {
                let painted: Vec<gpui::AnyElement> = rows.iter().map(|row| paint(row, cx)).collect();
                v_flex().w_full().min_w_0().children(painted).into_any_element()
            } else {
                // #FEED-191: Agents, then the sections under their headers;
                // the Archived entry leads Chats (its header shows while the
                // entry does).
                let sections = chat_sections(&model.list.rows);
                let mut archived_entry = archived_entry;
                let mut bands: Vec<gpui::AnyElement> = Vec::new();
                for key in domain::chats::ChatSectionKey::ALL {
                    let own = sections.iter().find(|section| section.key == key).map_or(&[][..], |section| &section.rows[..]);
                    let lead = if key == domain::chats::ChatSectionKey::Chats { archived_entry.take() } else { None };
                    if own.is_empty() && lead.is_none() {
                        continue;
                    }
                    let painted: Vec<gpui::AnyElement> = own.iter().map(|row| paint(row, cx)).collect();
                    bands.push(
                        v_flex()
                            .w_full()
                            .min_w_0()
                            .child(crate::chat_list::chat_section_header(key.title(), cx))
                            .children(lead)
                            .children(painted)
                            .into_any_element(),
                    );
                }
                let none = rows.is_empty() && bands.is_empty();
                v_flex()
                    .w_full()
                    .min_w_0()
                    .children(agents_section)
                    .children(apps_section)
                    .children(none.then(|| {
                        crate::controls::empty_state(Icon::new(registry::NAV_CHATS), crate::empty_state_copy::CHATS, None, cx)
                    }))
                    .children(bands)
                    .into_any_element()
            };
            let back = (view_kind == ChatListView::Archived).then(|| {
                let view = entity.clone();
                crate::session_screen::back_link_row("chats-archived-back", CHATS.copy.title.clone(), move |_window, cx| {
                    view.update(cx, |view, cx| view.set_view(ChatListView::List, cx)).ok();
                }, cx)
            });
            v_flex()
                .min_w_0()
                .gap_4()
                .children(back)
                .child(list)
                // Room under the last row for the floating bar.
                .when(selecting, |this| this.pb(px(72.)))
        };
        // EXP-698's rule, the issue list's: the bar's labels show while the
        // measured page is as wide as the labeled bar needs.
        let mut label_min_w = None;
        let bar = selecting.then(|| {
            let ids = self.selected.clone();
            let actions = domain::chats::chat_selection_actions(
                &crate::chat_actions::selection_rows(&ids, &rows, cx),
                view_kind,
            );
            let min_w = crate::chat_list::chat_bar_label_min_width(
                &actions.iter().map(|(action, _)| *action).collect::<Vec<_>>(),
            );
            label_min_w = Some(min_w);
            let labels = self.measured_width.get() >= min_w;
            let clear = entity.clone();
            let act = entity.clone();
            crate::chat_list::chat_selection_bar(
                ids.len(),
                actions,
                false,
                labels,
                false,
                move |_, _, cx| {
                    clear.update(cx, |view, cx| view.clear_selection(cx)).ok();
                },
                Some(std::rc::Rc::new(move |action, ids, window, cx| {
                    // Every action leaves the selection.
                    act.update(cx, |view, cx| view.clear_selection(cx)).ok();
                    crate::chat_actions::run_selection_action(action, ids, window, cx);
                })),
                cx,
            )
        });
        let measured_width = self.measured_width.clone();
        let rendered_labels = label_min_w.map(|min_w| measured_width.get() >= min_w);
        v_flex()
            .id("chats-page")
            .size_full()
            .relative()
            .child(
                // Width probe (the issue list's): re-render once when a
                // resize crosses the bar's label threshold. The notify is
                // deferred, this view is leased while it prepaints.
                gpui::canvas(
                    move |bounds, _, cx| {
                        measured_width.set(bounds.size.width);
                        let (Some(min_w), Some(rendered)) = (label_min_w, rendered_labels) else {
                            return;
                        };
                        if (bounds.size.width >= min_w) != rendered {
                            let entity = entity.clone();
                            cx.defer(move |cx| {
                                entity.update(cx, |_, cx| cx.notify()).ok();
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &SelectAllChats, _, cx| {
                let model = this.model(cx);
                let ids: Vec<String> = this.visible_rows(&model).into_iter().map(|row| row.session_id).collect();
                if ids.is_empty() {
                    return;
                }
                this.selected = ids;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ClearChatSelection, _, cx| {
                if this.selected.is_empty() {
                    cx.propagate();
                    return;
                }
                this.clear_selection(cx);
            }))
            .child(page_scaffold_with("chats-screen-scroll", &self.scroll, column, CHATS_COLUMN_W))
            .children(bar.map(crate::issue_list::bulk_bar_dock))
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
    fn the_chat_word_follows_the_contract() {
        let mut stacked = pull("p2", "r1", "open", "2026-10-13T08:00:00.000Z");
        stacked.base_pull_request_id = Some("p1".into());
        let pulls = vec![
            pull("p1", "r0", "open", "2026-10-13T07:00:00.000Z"),
            stacked,
            pull("p3", "r2", "open", "2026-10-13T08:00:00.000Z"),
            pull("p4", "r3", "merged", "2026-10-13T08:00:00.000Z"),
        ];
        assert_eq!(chat_word("r1", false, false, false, &pulls), Some(ChatRowWord::Stack));
        assert_eq!(chat_word("r2", false, true, false, &pulls), Some(ChatRowWord::ReadyToMerge));
        assert_eq!(chat_word("r2", true, false, false, &pulls), None);
        assert_eq!(chat_word("r3", false, true, false, &pulls), Some(ChatRowWord::Merged));
        assert_eq!(chat_word("r4", false, true, false, &pulls), Some(ChatRowWord::Question));
        assert_eq!(chat_word("r4", false, false, false, &pulls), None);
        assert_eq!(ChatRowWord::Merged.text(), None);
        assert_eq!(ChatRowWord::ReadyToMerge.text(), Some("ready to merge"));
        assert_eq!(ChatRowWord::Stack.text(), Some("stack"));
        assert_eq!(ChatRowWord::Question.text(), Some("question"));
    }
}
