//! The ⌘K issue quick-open — web parity target
//! `apps/web/src/components/issue-search-sheet.tsx` (masterplan-v3 §4.2:
//! "`IssueSearchSheet` is a `Dialog` (⌘K quick-open by title/identifier)").
//!
//! Desktop renders the web component's desktop branch only (§4.9 — no
//! mobile bottom sheet): a centered `Dialog` at ~15% from the top with a
//! borderless search input and the result rows (status icon + title +
//! project-dot · identifier). Picking a row navigates to the issue's detail
//! screen and closes the dialog.
//!
//! Built on gpui-component's `List` (`ListState` + [`ListDelegate`]): the
//! searchable query input, ↑/↓ selection, Enter-confirm, Esc-cancel and
//! virtualization all come from the component (the same machinery its
//! Combobox uses), so keyboard nav is first-class, not bolted on.
//!
//! [`init`] registers the App-global [`OpenSearch`] handler (the sidebar's
//! Search row dispatches it) and the global ⌘K / Ctrl-K binding (EXP-1 #3).

use gpui::{
    div, px, App, AppContext as _, IntoElement, KeyBinding, ParentElement, SharedString, Styled,
    Task, Window,
};
use gpui_component::{
    h_flex,
    list::{List, ListDelegate, ListItem, ListState},
    v_flex, ActiveTheme as _, Icon, IconName, IndexPath, Sizable as _, WindowExt as _,
};
use sync::{SessionPhase, Store};

use domain::options::get_issue_status_config;
use domain::IssueStatus;

use crate::actions::OpenSearch;
use crate::icons::option_icon;
use crate::issue_list::parse_hex_color;
use crate::navigation::{active_workspace_id, nav_for_window, navigate, Screen};

/// Web `.slice(0, 30)` — cap the result list.
const MAX_RESULTS: usize = 30;

/// Web `sm:max-w-lg` (32rem).
const DIALOG_WIDTH: f32 = 512.;

/// Register the App-global open handler + the quick-open keybinding. Called
/// once from `ui::init`.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &OpenSearch, cx| {
        let Some(window) = cx.active_window() else {
            return;
        };
        let _ = window.update(cx, |_, window, cx| open_search(window, cx));
    });
    #[cfg(target_os = "macos")]
    cx.bind_keys([KeyBinding::new("cmd-k", OpenSearch, None)]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("ctrl-k", OpenSearch, None)]);
}

/// Open the search dialog on `window` (no-op unless the session is `Synced`
/// with a resolvable workspace — ⌘K on the login surface must do nothing).
pub fn open_search(window: &mut Window, cx: &mut App) {
    if !matches!(
        Store::global(cx).session(cx),
        SessionPhase::Synced { .. }
    ) {
        return;
    }
    // Never stack search over an already-open dialog (⌘K spam / ⌘K while a
    // modal is up).
    if window.has_active_dialog(cx) {
        return;
    }
    let nav = nav_for_window(window, cx);
    let Some(workspace_id) = active_workspace_id(&nav, cx) else {
        return;
    };

    let list = cx.new(|cx| {
        ListState::new(IssueSearchDelegate::new(workspace_id), window, cx).searchable(true)
    });

    // Web: top-[15%], max-h-[60vh]. The List auto-grows with results up to
    // max_h (its `Infer` sizing), so the empty prompt stays a small dialog.
    let viewport = window.viewport_size();
    let margin_top = viewport.height * 0.15;
    let max_h = viewport.height * 0.6;

    window.open_dialog(cx, {
        let list = list.clone();
        move |dialog, _window, _cx| {
            dialog
                .close_button(false)
                .w(px(DIALOG_WIDTH))
                .margin_top(margin_top)
                .p_0()
                .child(
                    List::new(&list)
                        .search_placeholder("Search issues...")
                        .max_h(max_h),
                )
        }
    });
    // Focus the query input (searchable list → input handle) so typing starts
    // immediately, like the web autoFocus.
    list.update(cx, |list, cx| list.focus(window, cx));
}

/// One resolved search hit — project name/color denormalized at search time
/// (web `projectMap.get(issue.projectId)`).
struct SearchHit {
    issue_id: String,
    identifier: String,
    title: String,
    status: IssueStatus,
    project_name: Option<String>,
    project_color: Option<String>,
}

pub struct IssueSearchDelegate {
    workspace_id: String,
    query: String,
    hits: Vec<SearchHit>,
    selected: Option<IndexPath>,
}

impl IssueSearchDelegate {
    fn new(workspace_id: String) -> Self {
        Self {
            workspace_id,
            query: String::new(),
            hits: Vec::new(),
            selected: None,
        }
    }

    /// Filter the synced issues by title/identifier (web matches title; §4.2
    /// adds identifier for the desktop quick-open). Hits snapshot at search
    /// time — an Electric echo mid-dialog refreshes on the next keystroke.
    fn search(&mut self, cx: &App) {
        self.hits.clear();
        let query = self.query.to_lowercase();
        if query.is_empty() {
            return;
        }
        let collections = Store::global(cx).collections();
        let projects = collections.projects.read(cx);
        self.hits = collections
            .issues_in_workspace(&self.workspace_id, cx)
            .into_iter()
            .filter(|issue| {
                issue.title.to_lowercase().contains(&query)
                    || issue.identifier.to_lowercase().contains(&query)
            })
            .take(MAX_RESULTS)
            .map(|issue| {
                let project = projects.get(&issue.project_id);
                SearchHit {
                    issue_id: issue.id,
                    identifier: issue.identifier,
                    title: issue.title,
                    status: issue.status,
                    project_name: project.map(|p| p.name.clone()),
                    project_color: project.and_then(|p| p.color.clone()),
                }
            })
            .collect();
    }
}

impl ListDelegate for IssueSearchDelegate {
    type Item = ListItem;

    fn perform_search(
        &mut self,
        query: &str,
        _window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) -> Task<()> {
        self.query = query.trim().to_string();
        self.search(cx);
        Task::ready(())
    }

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.hits.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let hit = self.hits.get(ix.row)?;
        let status_config = get_issue_status_config(hit.status);
        let project_dot = hit
            .project_color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(cx.theme().muted_foreground);
        let subtitle: SharedString = match &hit.project_name {
            Some(name) => format!("{name} · {}", hit.identifier).into(),
            None => hit.identifier.clone().into(),
        };

        // Web row: status icon + (title / project-dot subtitle), px-4 py-3
        // border-b — compacted to a fixed two-line row.
        Some(
            ListItem::new(("issue-search-hit", ix.row))
                .h(px(44.))
                .px_3()
                .border_b_1()
                .border_color(cx.theme().border.opacity(0.3))
                .child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .overflow_hidden()
                        .child(option_icon(status_config, cx).small())
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_sm()
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(SharedString::from(hit.title.clone())),
                                )
                                .child(
                                    h_flex()
                                        .gap_1p5()
                                        .items_center()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(
                                            div()
                                                .size_1p5()
                                                .rounded_full()
                                                .flex_shrink_0()
                                                .bg(project_dot),
                                        )
                                        .child(
                                            div()
                                                .whitespace_nowrap()
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .child(subtitle),
                                        ),
                                ),
                        ),
                ),
        )
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut gpui::Context<ListState<Self>>,
    ) {
        self.selected = ix;
    }

    /// Click or Enter: navigate to the issue's detail and close the dialog
    /// (web `handlePick`).
    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) {
        let Some(ix) = self.selected else {
            return;
        };
        let Some(hit) = self.hits.get(ix.row) else {
            return;
        };
        let issue_id = hit.issue_id.clone();
        navigate(window, cx, Screen::IssueDetail { issue_id });
        window.close_dialog(cx);
    }

    /// Esc closes (the List consumes Escape ahead of the dialog's own
    /// binding, so the delegate owns the close).
    fn cancel(&mut self, window: &mut Window, cx: &mut gpui::Context<ListState<Self>>) {
        window.close_dialog(cx);
    }

    /// Web: the pre-query hint ("Type to search issues").
    fn render_initial(
        &mut self,
        _window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) -> Option<gpui::AnyElement> {
        Some(
            v_flex()
                .items_center()
                .justify_center()
                .p_8()
                .gap_2()
                .text_color(cx.theme().muted_foreground)
                .child(
                    Icon::new(IconName::Search)
                        .size_6()
                        .text_color(cx.theme().muted_foreground.opacity(0.5)),
                )
                .child(div().text_sm().child("Type to search issues"))
                .into_any_element(),
        )
    }

    /// Web: `No issues match "{query}"`.
    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) -> impl IntoElement {
        v_flex()
            .items_center()
            .justify_center()
            .p_8()
            .text_color(cx.theme().muted_foreground)
            .child(
                div()
                    .text_sm()
                    .child(SharedString::from(format!(
                        "No issues match \"{}\"",
                        self.query
                    ))),
            )
    }
}
