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
    div, px, App, AppContext as _, Entity, Focusable as _, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    skeleton::Skeleton,
    text::TextView,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
};
use serde::Serialize;
use sync::Store;

use domain::rows::{Issue, Project};

use crate::coding_flow::{LocalSessions, StartCodingControl};
use crate::icons::ExpIcon;
use crate::issue_changes::IssueChanges;
use crate::navigation::{navigate, Screen};
use crate::properties_panel::{parse_hex_color, spawn_issue_update, PropertiesPanel};
use crate::queries;
use crate::timeline::IssueTimeline;
use crate::{attachments_row, comments};

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
    /// The element to mount in the description slot.
    fn element(&self, window: &mut Window, cx: &mut App) -> gpui::AnyElement;
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

/// The segmented header's two panes (§4.8): the issue body vs. the single
/// worktree/PR diff surface.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Details,
    Changes,
}

pub struct IssueDetailView {
    issue_id: Option<String>,
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
    /// driven by live `repositories.forIssue` + doctor state (EXP-4).
    start_coding: Entity<StartCodingControl>,
    properties: Entity<PropertiesPanel>,
    timeline: Entity<IssueTimeline>,
    /// §4.8 segmented header: Details (the body) vs. Changes (the diff tab).
    tab: DetailTab,
    changes: Entity<IssueChanges>,
    _subscriptions: Vec<Subscription>,
}

impl IssueDetailView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let title_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Issue title"));
        let start_coding = cx.new(StartCodingControl::new);
        let properties = cx.new(|cx| PropertiesPanel::new(window, cx));
        let timeline = cx.new(|cx| IssueTimeline::new(window, cx));
        let changes = cx.new(|cx| IssueChanges::new(window, cx));

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

        Self {
            issue_id: None,
            title_input,
            synced_title: String::new(),
            editor: None,
            editor_issue: None,
            last_saved_description: Rc::new(RefCell::new(String::new())),
            subscribe_busy: false,
            start_coding,
            properties,
            timeline,
            tab: DetailTab::Details,
            changes,
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
        self.issue_id = Some(issue_id.clone());
        self.editor = None;
        self.editor_issue = None;
        self.synced_title = String::new();
        *self.last_saved_description.borrow_mut() = String::new();
        self.subscribe_busy = false;

        self.start_coding.update(cx, |control, cx| {
            control.set_issue(Some(issue_id.clone()), cx)
        });
        self.properties.update(cx, |panel, cx| {
            panel.set_issue(Some(issue_id.clone()), window, cx)
        });
        self.timeline
            .update(cx, |timeline, cx| {
                timeline.set_issue(Some(issue_id.clone()), window, cx)
            });
        // §4.8: navigating resets to the Details pane; the Changes tab (hidden)
        // repoints and won't fetch until it becomes visible again.
        self.tab = DetailTab::Details;
        self.changes.update(cx, |changes, cx| {
            changes.set_visible(false, cx);
            changes.set_issue(Some(issue_id), cx);
        });

        self.sync_from_issue(window, cx);
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

    fn project(&self, issue: &Issue, cx: &App) -> Option<Project> {
        Store::global(cx)
            .collections()
            .projects
            .read(cx)
            .get(&issue.project_id)
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
        // echoes.
        self.ensure_editor(&issue, window, cx);
        let incoming = issue.description.clone().unwrap_or_default();
        let normalized = incoming.trim().to_string();
        if normalized != *self.last_saved_description.borrow() {
            *self.last_saved_description.borrow_mut() = normalized;
            if let Some(editor) = self.editor.clone() {
                editor.set_markdown(&incoming, window, cx);
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

    // -- header pieces -----------------------------------------------------------

    /// Web breadcrumb strip: project dot+name → identifier → live title, with
    /// subscribe + actions on the right.
    fn render_breadcrumb(
        &mut self,
        issue: &Issue,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let project = self.project(issue, cx);
        let title = self.title_input.read(cx).value().to_string();

        let mut row = h_flex()
            .w_full()
            .px_4()
            .py_2()
            .gap_1p5()
            .items_center()
            .min_w_0()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .border_b_1()
            .border_color(cx.theme().border);

        if let Some(project) = project {
            let project_id = project.id.clone();
            let dot_color = project
                .color
                .as_deref()
                .and_then(parse_hex_color)
                .unwrap_or(cx.theme().muted_foreground);
            row = row
                .child(
                    h_flex()
                        .id("breadcrumb-project")
                        .gap_1p5()
                        .items_center()
                        .cursor_pointer()
                        .hover(|style| style.text_color(cx.theme().foreground))
                        .on_click(cx.listener(move |_, _, window, cx| {
                            navigate(
                                window,
                                cx,
                                Screen::Board {
                                    project_id: project_id.clone(),
                                },
                            );
                        }))
                        .child(div().size_2p5().rounded_full().bg(dot_color))
                        .child(SharedString::from(project.name.clone())),
                )
                .child(Icon::new(IconName::ChevronRight).xsmall());
        }

        row = row
            .child(
                div()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .whitespace_nowrap()
                    .child(SharedString::from(issue.identifier.clone())),
            )
            .child(Icon::new(IconName::ChevronRight).xsmall())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(title)),
            );

        // Right cluster: Start-coding affordance (§7.1 — play, or
        // "Coding…"+stop while OUR session runs), coding-now pill, subscribe
        // toggle, actions menu. The pill is skipped while a LOCAL session
        // runs — the control already shows the live indicator, and the synced
        // pill would double it as soon as the Electric echo lands.
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
        row = row.children(self.render_actions_menu(issue, cx));
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

    /// The `…` actions menu (web L361-398): Unmark duplicate only, and — like
    /// the web — the whole menu is present only for a duplicate issue (L27
    /// removed the standalone "Mark as duplicate…" entry; the status control
    /// now owns that path via interception).
    fn render_actions_menu(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> Option<impl IntoElement> {
        if issue.duplicate_of_id.is_none() {
            return None;
        }
        let issue_id = issue.id.clone();
        Some(
            Button::new("issue-actions")
                .ghost()
                .xsmall()
                .icon(Icon::new(IconName::Ellipsis).text_color(cx.theme().muted_foreground))
                .dropdown_menu(move |menu, _, _| {
                    let issue_id = issue_id.clone();
                    menu.item(
                        PopupMenuItem::new("Unmark duplicate")
                            .icon(Icon::new(IconName::Undo2))
                            .on_click(move |_, _, cx| {
                                set_duplicate_of(issue_id.clone(), None, cx);
                            }),
                    )
                }),
        )
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
        // align on it — EXP-8 §8.3).
        let title = div()
            .px_4()
            .pt_3()
            .pb_1()
            .text_xl()
            .font_weight(FontWeight::SEMIBOLD)
            .child(Input::new(&self.title_input).appearance(false));

        let mut column = v_flex()
            .w_full()
            .max_w(px(768.))
            .mx_auto()
            .child(title)
            .child(self.render_description(issue, window, cx));

        if let Some(rail) = attachments_row::attachments_row(&issue.id, cx) {
            column = column.child(rail);
        }
        column.child(self.timeline.clone())
    }

    /// §4.8 segmented header — Details (the body) · Changes (the diff tab).
    /// Selecting Changes makes the tab visible (it fetches on focus); selecting
    /// Details hides it (stops its poll).
    fn render_tabs(&mut self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let tab_button = |this: &Self, tab: DetailTab, label: &'static str, cx: &App| {
            let active = this.tab == tab;
            Button::new(match tab {
                DetailTab::Details => "issue-tab-details",
                DetailTab::Changes => "issue-tab-changes",
            })
            .ghost()
            .xsmall()
            .label(label)
            .text_color(if active {
                cx.theme().foreground
            } else {
                cx.theme().muted_foreground
            })
        };

        h_flex()
            .w_full()
            .px_4()
            .py_1()
            .gap_1()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                tab_button(self, DetailTab::Details, "Details", cx).on_click(cx.listener(
                    |this, _, _, cx| this.select_tab(DetailTab::Details, cx),
                )),
            )
            .child(
                tab_button(self, DetailTab::Changes, "Changes", cx).on_click(cx.listener(
                    |this, _, _, cx| this.select_tab(DetailTab::Changes, cx),
                )),
            )
    }

    fn select_tab(&mut self, tab: DetailTab, cx: &mut gpui::Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.changes
            .update(cx, |changes, cx| changes.set_visible(tab == DetailTab::Changes, cx));
        cx.notify();
    }
}

impl Render for IssueDetailView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let base = v_flex().size_full().bg(cx.theme().colors.list);

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
                            .child("Issue not found in this workspace."),
                    ),
                )
                .into_any_element();
        };

        let mut view = base.child(self.render_breadcrumb(&issue, window, cx));
        if let Some(duplicate_of_id) = issue.duplicate_of_id.clone() {
            if let Some(banner) = self.render_duplicate_banner(&duplicate_of_id, cx) {
                view = view.child(banner);
            }
        }
        view = view.child(self.render_tabs(cx));

        // §4.8: Changes is a full-width single diff surface (no properties
        // panel); Details keeps the two-pane body + properties panel.
        let body = match self.tab {
            DetailTab::Changes => div()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(self.changes.clone())
                .into_any_element(),
            DetailTab::Details => {
                let left = self.render_left_column(&issue, window, cx);
                h_flex()
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
                            .child(left),
                    )
                    .child(self.properties.clone())
                    .into_any_element()
            }
        };
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

/// The §4.2 steer presence pill: a "coding now" badge while a
/// `coding_sessions` row is `running` for this issue (the Watch/viewer UI is
/// §08 — another track wires it onto this pill).
fn coding_now_pill(issue_id: &str, cx: &App) -> Option<impl IntoElement> {
    let collections = Store::global(cx).collections();
    let session = collections
        .coding_sessions
        .read(cx)
        .iter()
        .find(|session| {
            session.issue_id.as_deref() == Some(issue_id)
                && session.status.as_deref() == Some("running")
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

