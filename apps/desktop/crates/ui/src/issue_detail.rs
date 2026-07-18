//! Full-page issue detail (masterplan-v3 §4.2; web parity target:
//! `apps/web/src/components/issue-detail-view.tsx` at compact density).
//!
//! Layout mirrors the web's desktop branch exactly: a breadcrumb header
//! (project → identifier → title, with the subscribe toggle + `…` actions
//! menu on the right), the duplicate-of banner, then a two-pane body — left:
//! borderless title input (save-on-blur) + description + attachment rail +
//! coding-now presence strip + PR section + timeline, all in one scroll;
//! right: the [`crate::properties_panel::PropertiesPanel`].
//!
//! **Description editor seam (§4.5).** The from-scratch GFM block editor
//! lands concurrently in `markdown_editor.rs`; this file must not depend on
//! its API. Instead it defines the [`DescriptionEditor`] trait + the
//! [`DescriptionEditorFactory`] global: the integrator installs a factory
//! (via [`install_description_editor`]) that adapts the real editor, and this
//! view builds one instance per issue, forwards Electric echoes through
//! `set_markdown`, and passes an `on_save` hook that runs the §4.1 un-gated
//! `issues.update`. Until a factory is installed the description renders as
//! **read-only markdown** (`TextView::markdown` — correct GFM rendering, no
//! editing), which is the safe v1 fallback.
//!
//! Mark-as-duplicate mirrors `issue-detail-view.tsx` L361-398: the actions
//! menu opens an issue picker `Dialog` (search over the synced `issues`
//! collection, current issue excluded); picking calls
//! `issues.update({ duplicate_of_id })` — the server atomically sets
//! `status='duplicate'`; "Unmark duplicate" clears the link and the server
//! restores the prior status.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    div, px, App, AppContext as _, Entity, FocusHandle, Focusable as _, FontWeight,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{self, Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    skeleton::Skeleton,
    text::TextView,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
};
use serde::Serialize;
use sync::Store;

use domain::rows::Issue;

use crate::actions::{NextIssue, PrevIssue};
use crate::coding_flow::{window_terminal_manager, CodingHub, LocalSessions, StartCodingControl};
use crate::icons::ExpIcon;
use crate::issue_list::IssueQuery;
use crate::navigation::{go_back, navigate, replace_screen, Screen};
use crate::properties_panel::{spawn_issue_update, PropertiesPanel};
use crate::queries;
use crate::timeline::IssueTimeline;
use crate::{attachments_row, comments};

/// The detail root's key context (terminal-dock pattern: `key_context` +
/// `track_focus` + `on_action`, bindings scoped via [`init`]).
const KEY_CONTEXT: &str = "IssueDetail";

/// The Details body's centered content width (web `max-w-3xl` parity) —
/// shared with the timeline, whose full-bleed divider re-centers its content
/// to this same column.
pub(crate) const DETAIL_COLUMN_W: f32 = 768.;

/// Center a detail column to [`DETAIL_COLUMN_W`] while keeping its width
/// DEFINITE (EXP-179). As a FLEX item, `max_w` + `mx_auto` disables stretch
/// and taffy sizes the column fit-content — gpui's text then gets measured
/// at unconstrained width and caches that layout: paragraphs paint wrapped
/// but occupy one line of layout height (the section below overlaps them),
/// and at wide sizes render as one clipped line. Under a display-BLOCK
/// wrapper (gpui's div default) the same `max_w` + `mx_auto` resolves like
/// CSS block flow — width = min(container, max), auto margins split the
/// rest — with no content-measure pass above the wrapping text.
pub(crate) fn centered_column(column: gpui::Div) -> gpui::Div {
    div()
        .w_full()
        .child(column.w_full().max_w(px(DETAIL_COLUMN_W)).mx_auto())
}

/// Register the EXP-48 J/K switcher bindings (call once from `ui::init`).
///
/// The predicate guards bare-letter keys against every editable surface that
/// can hold focus INSIDE the detail subtree: the pinned gpui evaluates `!X`
/// negations against the FULL focused dispatch path (`eval_inner` walks
/// `all_contexts`), so a focused title `Input`, description `MarkdownEditor`
/// or comment `MentionInput` disables the binding and the keystroke reaches
/// the text input untouched. `!Terminal` is belt-and-braces — the terminal
/// dock is a sibling panel whose focus path never contains `IssueDetail`.
pub(crate) fn init(cx: &mut App) {
    const SWITCHER_CONTEXT: &str =
        "IssueDetail && !Input && !MarkdownEditor && !MentionInput && !Terminal";
    cx.bind_keys([
        KeyBinding::new("j", NextIssue, Some(SWITCHER_CONTEXT)),
        KeyBinding::new("k", PrevIssue, Some(SWITCHER_CONTEXT)),
    ]);
}

// ---------------------------------------------------------------------------
// §4.5 editor seam
// ---------------------------------------------------------------------------

/// What the detail view needs from the (concurrently built) markdown editor.
/// Object-safe on purpose — this file never sees the editor's concrete type.
pub trait DescriptionEditor {
    /// Replace the buffer (an Electric echo of another client's edit). The
    /// editor may ignore it while the user is mid-edit (dirty), like web.
    fn set_markdown(&self, markdown: &str, window: &mut Window, cx: &mut App);
    /// Current GFM source.
    fn markdown(&self, cx: &App) -> String;
    /// Whether the editor currently owns keyboard focus (the user is
    /// mid-edit) — remote echoes must not rebuild the buffer under them.
    fn is_focused(&self, window: &Window, cx: &App) -> bool;
    /// The element to mount in the description slot.
    fn element(&self, window: &mut Window, cx: &mut App) -> gpui::AnyElement;
    /// Move keyboard focus into the editor (Tab from the title lands here —
    /// web EXP-10 parity).
    fn focus(&self, window: &mut Window, cx: &mut App);
}

/// Save hook of one description editor (markdown source at save time).
pub type OnSaveDescription = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// Everything the factory gets to build one editor instance.
pub struct DescriptionEditorParams {
    /// Image uploads target this issue (`/api/issues/{id}/images`).
    pub issue_id: String,
    pub initial_markdown: String,
    pub placeholder: SharedString,
    /// Save hook — called by the editor on blur / explicit save with the
    /// current source. The detail view wires this to `issues.update`.
    pub on_save: OnSaveDescription,
}

/// Builds a [`DescriptionEditor`] for one issue.
pub type DescriptionEditorBuilder =
    Rc<dyn Fn(&DescriptionEditorParams, &mut Window, &mut App) -> Rc<dyn DescriptionEditor>>;

/// Global seam the integrator fills from `markdown_editor.rs` (§4.5). Absent
/// → the read-only markdown fallback renders.
pub struct DescriptionEditorFactory {
    build: DescriptionEditorBuilder,
}

impl gpui::Global for DescriptionEditorFactory {}

/// Install the editor factory (call once at bootstrap, before windows open).
pub fn install_description_editor(cx: &mut App, build: DescriptionEditorBuilder) {
    cx.set_global(DescriptionEditorFactory { build });
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// EXP-48 switcher position: where the displayed issue sits in the active
/// issue list's flattened visible ordering.
struct SwitcherState {
    /// 0-based index in the flattened list.
    position: usize,
    total: usize,
    prev_id: Option<String>,
    next_id: Option<String>,
}

pub struct IssueDetailView {
    issue_id: Option<String>,
    /// Focus target of the detail root: holding it puts `IssueDetail` on the
    /// dispatch path so the scoped J/K bindings fire (focused on
    /// `set_issue`, re-acquired by clicking the body).
    focus_handle: FocusHandle,
    /// The Details body's scroll position. gpui persists scroll offsets per
    /// element id, and this view is ONE shared instance re-pointed across
    /// issues — without an explicit reset, issue B opens at issue A's scroll
    /// offset and the title sits above the viewport ("the title vanishes",
    /// EXP-67).
    body_scroll: gpui::ScrollHandle,
    /// The window's shared rail state — the EXP-48 switcher reads the active
    /// issue board's query + filters from it.
    rail_shared: Entity<crate::sidebar::RailShared>,
    title_input: Entity<InputState>,
    /// Last title pushed from sync — guards the echo loop (web's
    /// title-sync effect).
    synced_title: String,
    /// Seam-built editor (None → read-only fallback).
    editor: Option<Rc<dyn DescriptionEditor>>,
    /// Which issue the editor instance belongs to.
    editor_issue: Option<String>,
    /// Last description we saved or synced — dedupes echoes (web
    /// `lastSavedDescriptionRef`). Shared with the editor's `on_save`.
    last_saved_description: Rc<RefCell<String>>,
    /// Subscribe-toggle in-flight flag (web `busy`).
    subscribe_busy: bool,
    /// §7.1/§4.2 header affordance: the Start-coding button (play↔stop),
    /// driven by live `repositories.forIssue` + doctor state.
    start_coding: Entity<StartCodingControl>,
    properties: Entity<PropertiesPanel>,
    timeline: Entity<IssueTimeline>,
    _subscriptions: Vec<Subscription>,
}

impl IssueDetailView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let title_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Issue title"));
        let start_coding = cx.new(StartCodingControl::new);
        let properties = cx.new(|cx| PropertiesPanel::new(window, cx));
        let timeline = cx.new(|cx| IssueTimeline::new(window, cx));

        let mut subscriptions = Vec::new();
        // Title saves on blur when changed (web `handleTitleBlur`).
        subscriptions.push(cx.subscribe_in(
            &title_input,
            window,
            |this, _, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Blur) {
                    this.save_title(cx);
                }
            },
        ));
        // Keep local state mirroring the synced issue (remote title /
        // description edits land here).
        let collections = Store::global(cx).collections().clone();
        subscriptions.push(cx.observe_in(
            &collections.issues,
            window,
            |this, _, window, cx| {
                this.sync_from_issue(window, cx);
                cx.notify();
            },
        ));
        // Header affordances read these directly.
        subscriptions.push(cx.observe(&collections.projects, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.issue_subscribers, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.users, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.attachments, |_, _, cx| cx.notify()));
        // EXP-48 switcher: the counter follows the ACTIVE issue list — tool
        // swaps notify the shared rail state, filter changes notify the
        // boards (issue reorders already ride the issues observer above).
        let rail_shared = crate::sidebar::rail_shared_for_window(window, cx);
        subscriptions.push(cx.observe(&rail_shared, |_, _, cx| cx.notify()));
        let boards = rail_shared.read(cx).issue_boards().map(Clone::clone);
        for board in boards {
            subscriptions.push(cx.observe(&board, |_, _, cx| cx.notify()));
        }

        Self {
            issue_id: None,
            focus_handle: cx.focus_handle(),
            body_scroll: gpui::ScrollHandle::new(),
            rail_shared,
            title_input,
            synced_title: String::new(),
            editor: None,
            editor_issue: None,
            last_saved_description: Rc::new(RefCell::new(String::new())),
            subscribe_busy: false,
            start_coding,
            properties,
            timeline,
            _subscriptions: subscriptions,
        }
    }

    /// Point the view at an issue (the screens panel calls this on
    /// navigation, never mid-render). Local state fully resets — web resets
    /// on `issue.id` change.
    pub fn set_issue(
        &mut self,
        issue_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.issue_id.as_deref() == Some(issue_id.as_str()) {
            return;
        }
        // Commit an in-flight title edit to the OUTGOING issue before the
        // swap (its blur won't fire until `issue_id` already points at the
        // new issue — saving there would write onto the wrong row).
        self.save_title(cx);
        // Same for a pending description edit (EXP-68): the editor's
        // save-on-blur never fires when the view is re-pointed (tab switch)
        // — the focused input just vanishes from the tree — so the text was
        // silently dropped with the editor below.
        self.flush_description(cx);
        self.issue_id = Some(issue_id.clone());
        // Opening an issue clears its inbox notifications (EXP-92) — the
        // read-on-open safety net for list/search/deep-link navigation that
        // never passes the sidebar inbox's own per-row markRead.
        // Fire-and-forget: the Electric echo clears the dot.
        if let Some(trpc) = queries::trpc_client(cx) {
            let marked_issue_id = issue_id.clone();
            cx.background_executor()
                .spawn(async move {
                    if let Err(err) = api::notifications::notifications_mark_read_by_issue(
                        &trpc,
                        &marked_issue_id,
                    ) {
                        log::warn!(
                            "[ui] notifications.markReadByIssue({marked_issue_id}) failed: {err}"
                        );
                    }
                })
                .detach();
        }
        self.editor = None;
        self.editor_issue = None;
        self.synced_title = String::new();
        *self.last_saved_description.borrow_mut() = String::new();
        self.subscribe_busy = false;
        // Back to the top: the scroll offset belongs to the PREVIOUS issue
        // (gpui keys scroll state by element id and this view is shared) —
        // without this the new issue opens mid-scroll with its title hidden.
        self.body_scroll
            .set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
        // Swap the title UNCONDITIONALLY on an issue switch. The focused-input
        // guard in `sync_from_issue` exists for remote echoes of the SAME
        // issue; across a switch it would leave the old issue's title in the
        // input, and the blur that follows `window.focus` below would then
        // save it onto the NEW issue.
        if let Some(issue) = Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(&issue_id)
            .cloned()
        {
            self.synced_title = issue.title.clone();
            self.title_input
                .update(cx, |input, cx| input.set_value(issue.title, window, cx));
        } else {
            self.title_input
                .update(cx, |input, cx| input.set_value("", window, cx));
        }

        self.start_coding.update(cx, |control, cx| {
            control.set_issue(Some(issue_id.clone()), cx)
        });
        self.properties.update(cx, |panel, cx| {
            panel.set_issue(Some(issue_id.clone()), window, cx)
        });
        self.timeline
            .update(cx, |timeline, cx| {
                timeline.set_issue(Some(issue_id), window, cx)
            });

        self.sync_from_issue(window, cx);
        // Land keyboard focus on the detail root so the scoped J/K switcher
        // bindings are live immediately (clicking into an editor moves focus
        // and the guarded bindings go quiet — by design).
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn issue(&self, cx: &App) -> Option<Issue> {
        let issue_id = self.issue_id.as_deref()?;
        Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(issue_id)
            .cloned()
    }

    // -- sync: collection → local edit state -----------------------------------

    /// Mirror remote changes into the title input and the description editor
    /// (web's two sync effects). Skips the title while the user is typing in
    /// it (focused), exactly like the web guard.
    fn sync_from_issue(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(issue) = self.issue(cx) else {
            return;
        };

        // Title.
        if issue.title != self.synced_title {
            let focused = self
                .title_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window);
            if !focused {
                self.synced_title = issue.title.clone();
                let title = issue.title.clone();
                self.title_input
                    .update(cx, |input, cx| input.set_value(title, window, cx));
            }
        }

        // Description: build the editor when the seam is filled, then forward
        // echoes. Skipped while the editor owns focus (same rule as the
        // title) — `last_saved_description` stays stale on purpose so the
        // next non-focused sync still applies the remote text.
        self.ensure_editor(&issue, window, cx);
        let incoming = issue.description.clone().unwrap_or_default();
        let normalized = incoming.trim().to_string();
        if normalized != *self.last_saved_description.borrow() {
            let focused = self
                .editor
                .as_ref()
                .is_some_and(|editor| editor.is_focused(window, cx));
            if !focused {
                *self.last_saved_description.borrow_mut() = normalized;
                if let Some(editor) = self.editor.clone() {
                    editor.set_markdown(&incoming, window, cx);
                }
            }
        }
    }

    /// Build the seam editor for this issue if a factory is installed and we
    /// don't have one yet.
    fn ensure_editor(&mut self, issue: &Issue, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.editor_issue.as_deref() == Some(issue.id.as_str()) {
            return;
        }
        let Some(factory) = cx.try_global::<DescriptionEditorFactory>() else {
            return;
        };
        let build = factory.build.clone();

        let issue_id = issue.id.clone();
        let last_saved = self.last_saved_description.clone();
        let initial = issue.description.clone().unwrap_or_default();
        *last_saved.borrow_mut() = initial.trim().to_string();

        let params = DescriptionEditorParams {
            issue_id: issue_id.clone(),
            initial_markdown: initial,
            placeholder: SharedString::from("Add description..."),
            on_save: Rc::new(move |markdown: String, _window, cx: &mut App| {
                let normalized = markdown.trim().to_string();
                if normalized == *last_saved.borrow() {
                    return;
                }
                *last_saved.borrow_mut() = normalized.clone();
                let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                input.description = if normalized.is_empty() {
                    api::Patch::Null
                } else {
                    api::Patch::Set(normalized)
                };
                spawn_issue_update(cx, input);
            }),
        };
        self.editor = Some(build(&params, window, cx));
        self.editor_issue = Some(issue.id.clone());
    }

    // -- mutations --------------------------------------------------------------

    /// Flush a pending (un-blurred) description edit to the server (EXP-68).
    ///
    /// The editor saves on blur, but tab/view switches tear the editor's
    /// element out of the tree without a blur ever firing — the keystrokes
    /// only live in the seam's markdown mirror. Every path that re-points or
    /// hides this view (issue switch, center-tab close / undock, workspace
    /// switch) routes through here first. Same normalize + dedupe as the
    /// editor's `on_save` hook, so a clean editor is a no-op.
    pub(crate) fn flush_description(&self, cx: &mut App) {
        let Some(editor) = &self.editor else {
            return;
        };
        // The edit belongs to the issue the EDITOR was built for — during
        // `set_issue` the view already points at the incoming issue.
        let Some(issue_id) = self.editor_issue.clone() else {
            return;
        };
        let normalized = editor.markdown(cx).trim().to_string();
        if normalized == *self.last_saved_description.borrow() {
            return;
        }
        *self.last_saved_description.borrow_mut() = normalized.clone();
        let mut input = api::issues::IssuesUpdateInput::new(issue_id);
        input.description = if normalized.is_empty() {
            api::Patch::Null
        } else {
            api::Patch::Set(normalized)
        };
        spawn_issue_update(cx, input);
    }

    /// Web `handleTitleBlur`: trimmed, non-empty, changed → `issues.update`.
    fn save_title(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(issue) = self.issue(cx) else {
            return;
        };
        let trimmed = self.title_input.read(cx).value().trim().to_string();
        if trimmed.is_empty() || trimmed == issue.title {
            return;
        }
        self.synced_title = trimmed.clone();
        let mut input = api::issues::IssuesUpdateInput::new(issue.id);
        input.title = Some(trimmed);
        spawn_issue_update(cx, input);
    }

    fn toggle_subscription(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.subscribe_busy {
            return;
        }
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let subscribed = is_subscribed(&issue_id, &account.user_id, cx);
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.subscribe_busy = true;
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct SubscriptionInput<'a> {
                        issue_id: &'a str,
                    }
                    let path = if subscribed {
                        "subscriptions.unsubscribe"
                    } else {
                        "subscriptions.subscribe"
                    };
                    let out: Result<api::labels::TxOutput, api::ApiError> =
                        trpc.mutation(path, &SubscriptionInput { issue_id: &issue_id });
                    out
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.subscribe_busy = false;
                if let Err(err) = result {
                    log::warn!("[ui] subscription toggle failed: {err}");
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- EXP-48 prev/next switcher ------------------------------------------------

    /// Where this issue sits in the ACTIVE issue list's flattened visible
    /// ordering (the sidebar's My Issues board while that tool is active,
    /// the All Issues board otherwise) — same grouping, same EXP-38
    /// comparator, same filters the list applies. `None` (hide the switcher)
    /// when no list scope resolves or the issue isn't in the filtered list.
    fn switcher_state(&self, issue: &Issue, cx: &App) -> Option<SwitcherState> {
        let (query, filters) = {
            let board = self.rail_shared.read(cx).active_issue_board().read(cx);
            (board.query().clone(), board.filters().clone())
        };
        let data = match &query {
            IssueQuery::None => return None,
            IssueQuery::Project { project_id } => {
                queries::project_board(cx, project_id, &filters)
            }
            IssueQuery::MyIssues {
                workspace_id,
                user_id,
            } => queries::my_issues(cx, workspace_id, user_id, &filters),
        };
        let ids = domain::board::flatten_group_issue_ids(&data.groups);
        let position = ids.iter().position(|id| *id == issue.id)?;
        Some(SwitcherState {
            position,
            total: ids.len(),
            prev_id: position.checked_sub(1).map(|ix| ids[ix].clone()),
            next_id: ids.get(position + 1).cloned(),
        })
    }

    /// Swap the displayed issue in place: `+1` = next in list order, `-1` =
    /// previous. No wrap at the ends; a no-op when the current issue isn't
    /// in the filtered list (matching the hidden switcher).
    fn step_issue(&mut self, delta: i32, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(issue) = self.issue(cx) else {
            return;
        };
        let Some(state) = self.switcher_state(&issue, cx) else {
            return;
        };
        let target = if delta < 0 { state.prev_id } else { state.next_id };
        if let Some(issue_id) = target {
            replace_screen(window, cx, Screen::IssueDetail { issue_id });
        }
    }

    /// The "N / total" counter + up/down chevrons for the action header's
    /// right cluster. `None` hides the whole cluster segment.
    fn render_switcher(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> Option<impl IntoElement> {
        let state = self.switcher_state(issue, cx)?;
        Some(
            h_flex()
                .flex_shrink_0()
                .gap_0p5()
                .items_center()
                .child(
                    div()
                        .whitespace_nowrap()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(format!(
                            "{} / {}",
                            state.position + 1,
                            state.total
                        ))),
                )
                .child(
                    Button::new("issue-switch-prev")
                        .ghost()
                        .xsmall()
                        .icon(
                            Icon::new(IconName::ChevronUp)
                                .text_color(cx.theme().muted_foreground),
                        )
                        .disabled(state.prev_id.is_none())
                        .tooltip("Previous issue (K)")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.step_issue(-1, window, cx)
                        })),
                )
                .child(
                    Button::new("issue-switch-next")
                        .ghost()
                        .xsmall()
                        .icon(
                            Icon::new(IconName::ChevronDown)
                                .text_color(cx.theme().muted_foreground),
                        )
                        .disabled(state.next_id.is_none())
                        .tooltip("Next issue (J)")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.step_issue(1, window, cx)
                        })),
                ),
        )
    }

    // -- header pieces -----------------------------------------------------------

    /// The detail's ONE header row (EXP-67 — the former separate tab strip
    /// merged in to save vertical space): the actions right-aligned. The
    /// §4.8 Details · Changes segments are gone (EXP-179 — web dropped its
    /// changes tab in EXP-157; branch diffs live in Source Control). The
    /// breadcrumb trail lives in the TOP BAR (project picker › identifier ›
    /// title) and the center tab already shows the identifier (EXP-65
    /// follow-up: the identifier here was redundant).
    fn render_breadcrumb(
        &mut self,
        issue: &Issue,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let mut row = h_flex()
            .w_full()
            .px_4()
            .py_1p5()
            .gap_1p5()
            .items_center()
            .min_w_0()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .border_b_1()
            .border_color(cx.theme().border);

        row = row.child(div().flex_1().min_w_0());

        // Right cluster: the EXP-48 "N / total" prev/next switcher (hidden
        // when the issue isn't in the active list's filtered ordering), then
        // the Start-coding affordance (§7.1 — play, or "Coding…"+stop while
        // OUR session runs), coding-now pill, subscribe toggle, actions
        // menu. The pill is skipped while a LOCAL session runs — the control
        // already shows the live indicator, and the synced pill would double
        // it as soon as the Electric echo lands.
        row = row.children(self.render_switcher(issue, cx));
        row = row.child(self.start_coding.clone());
        let local_running = LocalSessions::global(cx)
            .read(cx)
            .get(&issue.id)
            .is_some();
        if !local_running {
            if let Some(pill) = coding_now_pill(&issue.id, cx) {
                row = row.child(pill);
            }
        }
        row = row.child(self.render_subscribe_toggle(issue, cx));
        row = row.child(self.render_actions_menu(issue, cx));
        row
    }

    /// Web `SubscribeToggle`: Bell/BellOff + label, live off the
    /// `issue_subscribers` shape.
    fn render_subscribe_toggle(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let account = queries::active_account(cx);
        let subscribed = account
            .as_ref()
            .map(|account| is_subscribed(&issue.id, &account.user_id, cx))
            .unwrap_or(false);
        let (icon, label) = if subscribed {
            (ExpIcon::Bell, "Subscribed")
        } else {
            (ExpIcon::BellOff, "Subscribe")
        };
        Button::new("subscribe-toggle")
            .ghost()
            .xsmall()
            .icon(Icon::from(icon).text_color(cx.theme().muted_foreground))
            .label(label)
            .disabled(self.subscribe_busy || account.is_none())
            .tooltip(if subscribed {
                "Unsubscribe from this issue"
            } else {
                "Subscribe to this issue"
            })
            .on_click(cx.listener(|this, _, window, cx| this.toggle_subscription(window, cx)))
    }

    /// The `…` actions menu (web L361-398): always present (EXP-59) with the
    /// Move-to-project submenu (EXP-57 — hidden without a move target; the
    /// detail tab keys on the stable issue UUID, so no navigation is needed
    /// when the identifier renumbers) and the destructive Delete-issue
    /// confirm submenu — the issue-row context menu's patterns — plus Unmark
    /// duplicate for a duplicate issue (L27 removed the standalone "Mark as
    /// duplicate…" entry; the status control now owns that path via
    /// interception). After the delete fires, the web navigates back to the
    /// board — the tabbed analog is popping the back stack. "Update from
    /// main" (rehomed from the deleted Changes tab, EXP-179) appears while
    /// the issue's worktree exists on disk.
    fn render_actions_menu(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let issue_id = issue.id.clone();
        let project_id = issue.project_id.clone();
        let is_duplicate = issue.duplicate_of_id.is_some();
        let can_move = !crate::issue_list::move_target_projects(cx, &project_id).is_empty();
        // Update-from-main context (EXP-179, ex-Changes-tab §4.9 action):
        // resolved repo + a worktree on disk. Blocked while a session runs —
        // a second `claude` in the same worktree would supersede the session
        // transcript the activity emitter tails.
        let update_ctx = self.start_coding.read(cx).resolved_repo().cloned().and_then(|repo| {
            let settings = CodingHub::global(cx).read(cx).settings.clone();
            let clone = coding::clone_path(&settings.repos_root_path(), &repo.full_name);
            let branch = coding::branch_name(&settings.branch_prefix, &issue.identifier);
            let worktree = coding::worktree_path(&clone, &branch);
            worktree.join(".git").exists().then(|| UpdateFromMainContext {
                settings,
                worktree,
                default_branch: repo.default_branch,
                identifier: issue.identifier.clone(),
            })
        });
        let session_running = session_running(&issue.id, cx);
        Button::new("issue-actions")
            .ghost()
            .xsmall()
            .icon(Icon::new(IconName::Ellipsis).text_color(cx.theme().muted_foreground))
            .dropdown_menu(move |mut menu, window, cx| {
                if let Some(ctx) = update_ctx.clone() {
                    let item = if session_running {
                        PopupMenuItem::new(
                            "Update from main — stop the running session first",
                        )
                        .icon(Icon::from(ExpIcon::Repeat))
                        .disabled(true)
                    } else {
                        PopupMenuItem::new("Update from main")
                            .icon(Icon::from(ExpIcon::Repeat))
                            .on_click(move |_, window, cx| {
                                update_from_main(&ctx, window, cx);
                            })
                    };
                    menu = menu.item(item).separator();
                }
                if is_duplicate {
                    let issue_id = issue_id.clone();
                    menu = menu
                        .item(
                            PopupMenuItem::new("Unmark duplicate")
                                .icon(Icon::new(IconName::Undo2))
                                .on_click(move |_, _, cx| {
                                    set_duplicate_of(issue_id.clone(), None, cx);
                                }),
                        )
                        .separator();
                }
                if can_move {
                    let issue_id = issue_id.clone();
                    let project_id = project_id.clone();
                    menu = menu.submenu_with_icon(
                        Some(Icon::from(ExpIcon::SquareKanban)),
                        "Move to project",
                        window,
                        cx,
                        move |menu, _, cx| {
                            crate::issue_list::move_to_project_menu(
                                menu,
                                &issue_id,
                                &project_id,
                                cx,
                            )
                        },
                    );
                }
                let issue_id = issue_id.clone();
                menu.submenu_with_icon(
                    Some(Icon::new(IconName::Delete)),
                    "Delete issue",
                    window,
                    cx,
                    move |menu, _, _| {
                        let issue_id = issue_id.clone();
                        menu.item(
                            PopupMenuItem::new("Confirm delete")
                                .icon(Icon::new(IconName::Delete))
                                .on_click(move |_, window, cx| {
                                    crate::issue_list::spawn_issue_delete(cx, issue_id.clone());
                                    go_back(window, cx);
                                }),
                        )
                    },
                )
            })
    }

    /// Web `DuplicateOfBanner`: "Duplicate of #IDENT — title" with Unmark.
    fn render_duplicate_banner(
        &mut self,
        duplicate_of_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<impl IntoElement> {
        let canonical = Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(duplicate_of_id)
            .cloned()?;
        let canonical_id = canonical.id.clone();

        Some(
            h_flex()
                .w_full()
                .px_4()
                .py_2()
                .gap_2()
                .items_center()
                .min_w_0()
                .text_sm()
                .bg(cx.theme().accent.opacity(0.3))
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    Icon::from(ExpIcon::Copy)
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(cx.theme().muted_foreground)
                        .child("Duplicate of"),
                )
                .child(
                    Button::new("duplicate-of-link")
                        .outline()
                        .xsmall()
                        .label(SharedString::from(format!("#{}", canonical.identifier)))
                        .on_click(cx.listener(move |_, _, window, cx| {
                            navigate(
                                window,
                                cx,
                                Screen::IssueDetail {
                                    issue_id: canonical_id.clone(),
                                },
                            );
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(canonical.title)),
                )
                .child(
                    Button::new("duplicate-unmark")
                        .ghost()
                        .xsmall()
                        .icon(Icon::new(IconName::Undo2).text_color(cx.theme().muted_foreground))
                        .label("Unmark")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(issue_id) = this.issue_id.clone() {
                                set_duplicate_of(issue_id, None, cx);
                            }
                        })),
                ),
        )
    }

    // -- body pieces --------------------------------------------------------------

    fn render_description(
        &mut self,
        issue: &Issue,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        if let Some(editor) = self.editor.clone() {
            return div().px_4().child(editor.element(window, cx)).into_any_element();
        }
        // Read-only fallback (§4.5 seam not wired yet): rendered GFM.
        let source = issue.description.clone().unwrap_or_default();
        if source.trim().is_empty() {
            return div()
                .px_4()
                .py_2()
                .text_sm()
                .text_color(cx.theme().muted_foreground.opacity(0.6))
                .child("Add description...")
                .into_any_element();
        }
        div()
            .px_4()
            .py_2()
            .text_sm()
            .child(TextView::markdown("issue-description", SharedString::from(source)).selectable(true))
            .into_any_element()
    }

    fn render_left_column(
        &mut self,
        issue: &Issue,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        // Borderless 2xl title (web `titleField`). px_4 = the one shared left
        // edge for the detail body (breadcrumb / tabs / title / description all
        // align on it — §8.3).
        let title = div()
            .px_4()
            .pt_3()
            .pb_1()
            .text_xl()
            .font_weight(FontWeight::SEMIBOLD)
            // Tab jumps from the title into the description editor (web
            // EXP-10 parity, dialog-shell.tsx). Capture runs before the
            // InputState's own Tab handling; Shift+Tab (`OutdentInline`) is a
            // different action, so it keeps its default behavior.
            .capture_action(cx.listener(
                |this, _: &input::IndentInline, window, cx: &mut gpui::Context<Self>| {
                    if let Some(editor) = this.editor.clone() {
                        cx.stop_propagation();
                        editor.focus(window, cx);
                    }
                },
            ))
            .child(Input::new(&self.title_input).appearance(false));

        let mut column = v_flex()
            .child(title)
            .child(self.render_description(issue, window, cx));

        if let Some(rail) = attachments_row::attachments_row(&issue.id, cx) {
            column = column.child(rail);
        }
        // The timeline sits OUTSIDE the centered column: its top border runs
        // full-bleed across the detail body (EXP-67 — one line splitting the
        // description+images section from the comment section); the
        // timeline's own content re-centers to the same column width.
        v_flex()
            .w_full()
            .child(centered_column(column))
            .child(self.timeline.clone())
    }
}

impl Render for IssueDetailView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Terminal-dock pattern: key context + tracked focus + action
        // handlers make the scoped J/K bindings (see [`init`]) land here.
        let base = v_flex()
            .size_full()
            .bg(cx.theme().colors.list)
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &NextIssue, window, cx| {
                this.step_issue(1, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PrevIssue, window, cx| {
                this.step_issue(-1, window, cx)
            }));

        let Some(issue) = self.issue(cx) else {
            let issues_ready = Store::global(cx)
                .collections()
                .issues
                .read(cx)
                .is_ready();
            if !issues_ready {
                // §4.1: never render "not found" off an unsynced snapshot.
                return base
                    .child(
                        v_flex()
                            .p_4()
                            .gap_2()
                            .child(Skeleton::new().h_4().w_48())
                            .child(Skeleton::new().h_4().w_64())
                            .child(Skeleton::new().h_4().w_56()),
                    )
                    .into_any_element();
            }
            return base
                .child(
                    v_flex().flex_1().items_center().justify_center().child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Issue not found in this team."),
                    ),
                )
                .into_any_element();
        };

        // ONE header row (tabs + actions merged, EXP-67) — the standalone
        // tab strip is gone.
        let mut view = base.child(self.render_breadcrumb(&issue, window, cx));
        if let Some(duplicate_of_id) = issue.duplicate_of_id.clone() {
            if let Some(banner) = self.render_duplicate_banner(&duplicate_of_id, cx) {
                view = view.child(banner);
            }
        }

        // The two-pane body: scrolling detail column + properties panel.
        let left = self.render_left_column(&issue, window, cx);
        let body = h_flex()
            .flex_1()
            .min_h_0()
            .items_start()
            .overflow_hidden()
            .child(
                div()
                    .id("issue-detail-scroll")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.body_scroll)
                    .child(left),
            )
            .child(self.properties.clone());
        view.child(body).into_any_element()
    }
}

// ---------------------------------------------------------------------------
// Duplicate-of mutation + picker dialog (issue-picker-dialog.tsx, detail scope)
// ---------------------------------------------------------------------------

/// Link/unlink `duplicate_of_id` (web `issues.update`): the server sets
/// `status='duplicate'` atomically on link and restores the prior status on
/// clear. `pub(crate)` — the row `ContextMenu`'s "Unmark duplicate" item
/// (§4.6) shares this mutation.
pub(crate) fn set_duplicate_of(issue_id: String, canonical_id: Option<String>, cx: &mut App) {
    let mut input = api::issues::IssuesUpdateInput::new(issue_id);
    input.duplicate_of_id = match canonical_id {
        Some(id) => api::Patch::Set(id),
        None => api::Patch::Null,
    };
    spawn_issue_update(cx, input);
}

/// L27 status interception: selecting `duplicate` from ANY status control
/// opens the duplicate picker (the server links `duplicate_of_id` and sets
/// `status='duplicate'` atomically) instead of writing the status directly;
/// every other status flows straight through to `issues.update`. Cancelling
/// the picker writes nothing, so the control reverts to the live status. The
/// single interception point shared by the detail properties panel, the row
/// status dropdown and the row context menu (web `useDuplicateInterception`).
pub(crate) fn apply_status_selection(
    issue_id: String,
    status: domain::IssueStatus,
    window: &mut Window,
    cx: &mut App,
) {
    if status == domain::IssueStatus::Duplicate {
        open_duplicate_picker(issue_id, window, cx);
        return;
    }
    let mut input = api::issues::IssuesUpdateInput::new(issue_id);
    input.status = Some(status);
    spawn_issue_update(cx, input);
}

/// Open the shared duplicate-picker dialog for `issue_id`. `pub(crate)` — the
/// §4.6 shared-`IssuePicker` rule: both the detail actions menu and the row
/// `ContextMenu`'s "Mark as duplicate…" item open this same overlay.
pub(crate) fn open_duplicate_picker(issue_id: String, window: &mut Window, cx: &mut App) {
    let picker = cx.new(|cx| DuplicatePicker::new(issue_id, window, cx));
    window.open_dialog(cx, move |dialog, _, _| {
        let picker = picker.clone();
        dialog
            .title("Mark as duplicate")
            .w(px(480.))
            .button_props(
                gpui_component::dialog::DialogButtonProps::default().show_cancel(false),
            )
            .content(move |content, _, _| content.child(picker.clone()))
    });
}

/// Searchable issue list over the synced `issues` collection, excluding the
/// current issue. Picking commits `duplicate_of_id` and closes the dialog.
struct DuplicatePicker {
    exclude_issue_id: String,
    search: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl DuplicatePicker {
    fn new(exclude_issue_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search the canonical issue…")
        });
        let mut subscriptions = vec![cx.subscribe(
            &search,
            |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        )];
        let collections = Store::global(cx).collections().clone();
        subscriptions.push(cx.observe(&collections.issues, |_, _, cx| cx.notify()));

        Self {
            exclude_issue_id,
            search,
            _subscriptions: subscriptions,
        }
    }

    fn matches(&self, cx: &App) -> Vec<Issue> {
        let query = self.search.read(cx).value().trim().to_lowercase();
        let mut issues: Vec<Issue> = Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .iter()
            .filter(|issue| issue.id != self.exclude_issue_id && issue.archived_at.is_none())
            .filter(|issue| {
                query.is_empty()
                    || issue.identifier.to_lowercase().contains(&query)
                    || issue.title.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        issues.sort_by(|a, b| sync::cmp_identifiers(&a.identifier, &b.identifier));
        issues.truncate(50);
        issues
    }

    fn pick(&self, canonical_id: String, window: &mut Window, cx: &mut App) {
        let mut input = api::issues::IssuesUpdateInput::new(self.exclude_issue_id.clone());
        // The server sets status='duplicate' atomically with the link.
        input.duplicate_of_id = api::Patch::Set(canonical_id);
        spawn_issue_update(cx, input);
        window.close_dialog(cx);
    }
}

impl Render for DuplicatePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let issues = self.matches(cx);

        let mut list = v_flex().w_full().max_h(px(320.)).gap_0p5();
        if issues.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .py_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No matching issues."),
            );
        }
        for issue in issues {
            let issue_id = issue.id.clone();
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("dup-pick-{}", issue.id)))
                    .w_full()
                    .px_2()
                    .py_1p5()
                    .gap_2()
                    .items_center()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|style| style.bg(cx.theme().colors.list_hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pick(issue_id.clone(), window, cx);
                    }))
                    .child(
                        div()
                            .w(px(72.))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(SharedString::from(issue.identifier.clone())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(issue.title.clone())),
                    ),
            );
        }

        v_flex()
            .w_full()
            .gap_2()
            .child(Input::new(&self.search))
            .child(
                div()
                    .id("dup-pick-scroll")
                    .w_full()
                    .max_h(px(320.))
                    .overflow_y_scroll()
                    .child(list),
            )
    }
}

// ---------------------------------------------------------------------------
// Small pieces
// ---------------------------------------------------------------------------

/// Live subscribe state off the `issue_subscribers` shape (web
/// `SubscribeToggle` query: row for (issue, me) and NOT unsubscribed).
fn is_subscribed(issue_id: &str, user_id: &str, cx: &App) -> bool {
    Store::global(cx)
        .collections()
        .issue_subscribers
        .read(cx)
        .iter()
        .any(|subscriber| {
            subscriber.issue_id == issue_id
                && subscriber.user_id.as_deref() == Some(user_id)
                && subscriber.unsubscribed != Some(true)
        })
}

/// A running `coding_sessions` row for the issue (any client — the synced
/// shape). Heartbeat-stale rows count as absent (EXP-153).
fn session_running(issue_id: &str, cx: &App) -> bool {
    let now = chrono::Utc::now().timestamp();
    Store::global(cx)
        .collections()
        .coding_sessions
        .read(cx)
        .iter()
        .any(|session| {
            session.issue_id.as_deref() == Some(issue_id)
                && crate::queries::coding_session_is_live(session, now)
        })
}

/// Everything the actions-menu "Update from main" click needs, resolved at
/// render time (menus must not read `self`).
#[derive(Clone)]
struct UpdateFromMainContext {
    settings: coding::Settings,
    worktree: std::path::PathBuf,
    default_branch: String,
    identifier: String,
}

/// Update from main (v4 §4.9, rehomed from the deleted Changes tab —
/// EXP-179): a Claude task in the worktree — "rebase onto origin/<default>,
/// resolve conflicts, verify the build, then push with --force-with-lease" —
/// opened as a `ClaudeTask` terminal tab.
fn update_from_main(ctx: &UpdateFromMainContext, window: &mut Window, cx: &mut App) {
    let Some(manager) = window_terminal_manager(window, cx) else {
        return;
    };
    let prompt = coding::resolve_pr_prompt(&ctx.default_branch);
    let label = format!("Update from main · {}", ctx.identifier);
    // A worktree prepared by a pre-EXP-98 app version still carries a stale
    // `.mcp.json`, which alone re-raises claude's project-approval dialog.
    coding::remove_stale_legacy_mcp_json(&ctx.worktree);
    let task = coding::claude_task(&ctx.settings, &ctx.worktree, &prompt, &label);
    let _ = manager.update(cx, |manager, cx| {
        // EXP-145: keep the issue identity visible once claude's OSC titles
        // take over the tab.
        manager.open_tab(
            terminal::TabKind::ClaudeTask,
            task.tab_title.clone(),
            Some(ctx.identifier.clone().into()),
            &task.spawn,
            None,
            cx,
        )
    });
}

/// The §4.2 steer presence pill: a "coding now" badge while a
/// `coding_sessions` row is `running` for this issue (the Watch/viewer UI is
/// §08 — another track wires it onto this pill).
fn coding_now_pill(issue_id: &str, cx: &App) -> Option<impl IntoElement> {
    let collections = Store::global(cx).collections();
    let now = chrono::Utc::now().timestamp();
    let session = collections
        .coding_sessions
        .read(cx)
        .iter()
        .find(|session| {
            session.issue_id.as_deref() == Some(issue_id)
                && crate::queries::coding_session_is_live(session, now)
        })
        .cloned()?;

    let who = session
        .user_id
        .as_deref()
        .and_then(|id| collections.users.read(cx).get(id).cloned())
        .map(|user| comments::author_label(Some(&user)));
    let label = match (who, session.device_label.as_deref()) {
        (Some(who), Some(device)) => format!("{who} coding now · {device}"),
        (Some(who), None) => format!("{who} coding now"),
        (None, Some(device)) => format!("Coding now · {device}"),
        (None, None) => "Coding now".to_string(),
    };

    Some(
        h_flex()
            .flex_shrink_0()
            .gap_1p5()
            .px_2()
            .py_0p5()
            .rounded_full()
            .border_1()
            .border_color(theme::tokens::GREEN.to_hsla().opacity(0.4))
            .items_center()
            .text_xs()
            .child(
                div()
                    .size_1p5()
                    .rounded_full()
                    .bg(theme::tokens::GREEN.to_hsla()),
            )
            .child(SharedString::from(label)),
    )
}

