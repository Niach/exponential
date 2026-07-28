//! Full-page issue detail (masterplan-v3 §4.2; web parity target:
//! `apps/web/src/components/issue-detail-view.tsx` at compact density).
//!
//! Layout mirrors the web's desktop branch exactly: a breadcrumb header
//! (board → identifier → title, with the subscribe toggle + `…` actions
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
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, Entity, FocusHandle, Focusable as _, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{self, Input, InputEvent, InputState},
    notification::Notification,
    skeleton::Skeleton,
    text::TextView,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use sync::Store;

use domain::rows::{Attachment, Issue};

use crate::coding_flow::StartCodingControl;
use crate::icons::{registry, ExpIcon};
use crate::issue_files::{
    all_attachment_ids, attachment_label, file_attachments, format_bytes, icon_for_content_type,
    is_inline_image, temp_open_path,
};
use crate::navigation::{navigate, Screen};
use crate::properties_panel::{spawn_issue_update, PropertiesPanel};
use crate::queries;
use crate::timeline::IssueTimeline;
use crate::comments;

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
/// The ONE left edge of the detail body (EXP-282): the title row, the
/// description slot, the activity section and the comment composer all
/// resolve to this inset inside [`centered_column`] — `px_4` on every plain
/// block, and `DETAIL_GUTTER - WYSIWYG_BLOCK_PADDING_X` on the slot that
/// hosts the self-padding WYSIWYG editor.
pub(crate) const DETAIL_GUTTER: f32 = 16.;

/// The vendored WYSIWYG editor's own per-block horizontal padding —
/// shared with the toolbar compensation (EXP-285), see `wysiwyg::mod`.
pub(crate) use crate::wysiwyg::WYSIWYG_BLOCK_PADDING_X;

pub(crate) fn centered_column(column: gpui::Div) -> gpui::Div {
    div()
        .w_full()
        .child(column.w_full().max_w(px(DETAIL_COLUMN_W)).mx_auto())
}

// The EXP-48 bare-letter J/K switcher bindings are GONE (EXP-268): the
// context-negation guard (`!Input && !MarkdownEditor && …`) missed the
// EXP-261 WYSIWYG editor's renamed key context, so typing `k` in a
// description jumped issues and ate the letter. Bare-letter shortcuts are
// one stale negation away from that failure by construction — the switcher
// keeps its clickable header arrows only.

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
    /// EXP-261: whether the user actually edited since the last load/save.
    /// The vendored WYSIWYG serializer NORMALIZES render-equivalent forms, so
    /// a byte diff against the loaded description does NOT imply an edit —
    /// the flush path skips clean editors so a view-only open never rewrites
    /// other clients' content. The `true` default keeps editors without edit
    /// tracking (the classic block-editor revert path) on the old
    /// byte-compare-only behavior.
    fn is_dirty(&self, _cx: &App) -> bool {
        true
    }
    /// EXP-261: record that the current content was just persisted (the
    /// flush path saves outside the editor's own save hook).
    fn mark_clean(&self, _cx: &mut App) {}
    /// EXP-288: hand the editor the HOST's tracked scroll handle so its
    /// caret-follow keeps the caret visible in the host's scroll container
    /// while typing/pasting. Default no-op for editors without one.
    fn set_scroll_handle(&self, _handle: gpui::ScrollHandle, _cx: &mut App) {}
    /// EXP-261: the bytes a PERSIST site must use — [`Self::markdown`] with
    /// still-uploading `draft://` staging images structurally stripped. A
    /// `draft://` URL must never reach the server; every save path (the
    /// editor's own blur save AND the detail view's tab-switch flush) derives
    /// its bytes through this one shared derivation, never raw `markdown()`.
    fn markdown_for_save(&self, cx: &App) -> String {
        crate::markdown::image_paste::markdown_for_save(self.markdown(cx))
    }
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
    /// EXP-335: non-inline-image files picked via the editor toolbar's attach
    /// button — the detail view wires this to the Files-section upload flow.
    pub on_attach_files: Rc<dyn Fn(Vec<PathBuf>, &mut Window, &mut App)>,
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

/// One file the user just picked, from the moment it is staged until its
/// synced `attachments` row arrives (EXP-297). The pending row is what makes
/// an upload visible; the SYNCED row replaces it (deduped by `uploaded_id`),
/// so a slow Electric echo never shows the file twice.
struct PendingFileUpload {
    /// Process-local row key (element ids + the completion lookup).
    key: u64,
    filename: String,
    /// The attachment id the server assigned — set once the POST answered.
    uploaded_id: Option<String>,
    /// Set when the read or the upload failed; the row then shows the
    /// message with a dismiss ✕ instead of "Uploading…".
    error: Option<SharedString>,
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
    /// §7.1/§4.2 header affordance: the Start-coding button (play↔stop),
    /// driven by live `repositories.forIssue` + doctor state.
    start_coding: Entity<StartCodingControl>,
    properties: Entity<PropertiesPanel>,
    timeline: Entity<IssueTimeline>,
    /// EXP-297 files rail: in-flight picks (see [`PendingFileUpload`]).
    pending_files: Vec<PendingFileUpload>,
    next_pending_file_key: u64,
    /// Attachment ids with an open/save/delete request in flight — their row
    /// actions are disabled until it settles.
    busy_files: HashSet<String>,
    _subscriptions: Vec<Subscription>,
}

impl IssueDetailView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // Auto-grow so a long title soft-wraps across rows instead of
        // clipping at the right edge (EXP-230). Titles stay one LOGICAL
        // line: `submit_on_enter` turns plain Enter into a commit instead
        // of a newline, the render-side capture swallows Shift+Enter, and
        // `save_title` collapses pasted newlines.
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Issue title")
                .auto_grow(1, 5)
                .submit_on_enter(true)
        });
        let start_coding = cx.new(StartCodingControl::new);
        let properties = cx.new(|cx| PropertiesPanel::new(start_coding.clone(), window, cx));
        let timeline = cx.new(|cx| IssueTimeline::new(window, cx));

        let mut subscriptions = Vec::new();
        // Title saves on blur when changed (web `handleTitleBlur`); Enter
        // commits too — in the auto-grow input it emits PressEnter instead
        // of inserting a newline.
        subscriptions.push(cx.subscribe_in(
            &title_input,
            window,
            |this, _, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
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
        // Body affordances (PR section, attachments, coding pill) read these
        // directly. (The former header cluster's subscribers/rail observers
        // moved to the properties panel with the cluster — EXP-277.)
        subscriptions.push(cx.observe(&collections.boards, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.users, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.attachments, |_, _, cx| cx.notify()));

        Self {
            issue_id: None,
            focus_handle: cx.focus_handle(),
            body_scroll: gpui::ScrollHandle::new(),
            title_input,
            synced_title: String::new(),
            editor: None,
            editor_issue: None,
            last_saved_description: Rc::new(RefCell::new(String::new())),
            start_coding,
            properties,
            timeline,
            pending_files: Vec::new(),
            next_pending_file_key: 1,
            busy_files: HashSet::new(),
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
        // The files rail's transient state belongs to the OUTGOING issue —
        // a pending upload row or a busy marker must never leak onto the
        // incoming one (the in-flight requests themselves keep running and
        // land through the synced collection).
        self.pending_files.clear();
        self.busy_files.clear();
        *self.last_saved_description.borrow_mut() = String::new();
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
            on_attach_files: {
                let view = cx.entity().downgrade();
                let issue_id = issue.id.clone();
                Rc::new(move |paths: Vec<PathBuf>, window, cx: &mut App| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };
                    view.update(cx, |view, cx| {
                        for path in paths {
                            view.start_file_upload(issue_id.clone(), path, window, cx);
                        }
                    });
                })
            },
        };
        let editor = build(&params, window, cx);
        // EXP-288: the detail body's scroll container follows the caret
        // while typing/pasting at the bottom of a long description.
        editor.set_scroll_handle(self.body_scroll.clone(), cx);
        self.editor = Some(editor);
        self.editor_issue = Some(issue.id.clone());
    }

    // -- mutations --------------------------------------------------------------

    /// Flush a pending (un-blurred) description edit to the server (EXP-68).
    ///
    /// The editor saves on blur, but tab/view switches tear the editor's
    /// element out of the tree without a blur ever firing — the keystrokes
    /// only live in the seam's markdown mirror. Every path that re-points or
    /// hides this view (issue switch, center-tab close / undock, team
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
        // EXP-261: no user edit → nothing to flush. The serializer
        // normalizes render-equivalent markdown, so the byte diff below is
        // only trustworthy AFTER a real edit happened.
        if !editor.is_dirty(cx) {
            return;
        }
        // EXP-261: `markdown_for_save`, never raw `markdown()` — a paste
        // still uploading when the user switches tabs must not persist its
        // `draft://` placeholder (the editor is dropped with the view, so
        // the upload's healing rewrite would never run).
        let normalized = editor.markdown_for_save(cx).trim().to_string();
        if normalized == *self.last_saved_description.borrow() {
            editor.mark_clean(cx);
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
        editor.mark_clean(cx);
    }

    /// Web `handleTitleBlur`: trimmed, non-empty, changed → `issues.update`.
    fn save_title(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(issue) = self.issue(cx) else {
            return;
        };
        // Pasted text can carry newlines (the input is auto-grow
        // multi-line); a title is one logical line, so collapse them.
        let trimmed = self
            .title_input
            .read(cx)
            .value()
            .replace(['\r', '\n'], " ")
            .trim()
            .to_string();
        if trimmed.is_empty() || trimmed == issue.title {
            return;
        }
        self.synced_title = trimmed.clone();
        let mut input = api::issues::IssuesUpdateInput::new(issue.id);
        input.title = Some(trimmed);
        spawn_issue_update(cx, input);
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
                        .icon(Icon::new(registry::UI_UNDO).text_color(cx.theme().muted_foreground))
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
            // The 96px floor restores the pre-EXP-261 classic-editor height
            // (EXP-268): the embedded WYSIWYG editor contributes no height of
            // its own, so an empty description otherwise collapses to a
            // one-line input instead of reading as a textarea.
            //
            // EXP-282 alignment: the vendored editor pads every block by its
            // own `block_padding_x` (12px), so a `px_4` slot pushed the
            // description text 28px in — 12px past the title/timeline edge.
            // The slot contributes the REMAINDER of the shared 16px gutter.
            // EXP-285: a flex column so the editor view's trailing filler
            // strip absorbs the min-height leftover — clicking the empty
            // area below a short description places the caret at the end
            // (textarea behavior) instead of dying on a bare div.
            return div()
                .px(px(DETAIL_GUTTER - WYSIWYG_BLOCK_PADDING_X))
                .min_h(px(96.))
                .flex()
                .flex_col()
                .child(editor.element(window, cx))
                .into_any_element();
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
            .child(
                // EXP-282: glass code blocks instead of the component's
                // opaque `muted` panel.
                TextView::markdown("issue-description", SharedString::from(source))
                    .style(crate::surface::markdown_style())
                    .selectable(true),
            )
            .into_any_element()
    }

    // -- EXP-297 files rail -----------------------------------------------------

    /// The "Files" section: every NON-inline-image attachment of the issue
    /// (`issue_files::file_attachments` — pdf/zip/video/… plus the image
    /// types markdown never embeds) with Open / Save as / Delete, the
    /// in-flight picks on top of them, and the "Attach file" picker.
    ///
    /// Inline images are deliberately absent: they live in the description
    /// markdown and the editor renders them. The section re-renders off the
    /// view's existing `attachments` collection observer, so an upload,
    /// a delete or another client's change lands without any refetch.
    fn render_files_section(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let attachments = file_attachments(&issue.id, cx);
        // A pending row disappears the moment its synced row shows up — that
        // is the one dedupe rule between the two sources. The set spans ALL
        // synced rows (inline images included), so a row can never spin
        // forever just because its content type classified out of the rail.
        let synced_ids = all_attachment_ids(&issue.id, cx);
        let pending: Vec<(u64, String, Option<SharedString>)> = self
            .pending_files
            .iter()
            .filter(|pending| {
                pending
                    .uploaded_id
                    .as_deref()
                    .is_none_or(|id| !synced_ids.contains(id))
            })
            .map(|pending| (pending.key, pending.filename.clone(), pending.error.clone()))
            .collect();

        // EXP-335: with nothing to list the section renders NOTHING — the
        // attach entry point moved into the description toolbar's paperclip
        // (web parity; the old lone right-aligned button confused everyone).
        let has_rows = !attachments.is_empty() || !pending.is_empty();
        if !has_rows {
            return gpui::Empty.into_any_element();
        }

        let issue_id = issue.id.clone();
        // EXP-316: icon-only attach button (tooltip carries the wording).
        let attach_button = Button::new("issue-files-attach")
            .ghost()
            .xsmall()
            .icon(Icon::from(ExpIcon::Paperclip).xsmall())
            .tooltip("Attach file")
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick_files(issue_id.clone(), window, cx);
            }));
        let header = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().muted_foreground)
                    .child("Files"),
            )
            .child(attach_button);

        let mut section = v_flex()
            .w_full()
            .px(px(DETAIL_GUTTER))
            .pt_2()
            .gap_1()
            .child(header);

        for attachment in &attachments {
            section = section.child(self.render_file_row(attachment, cx));
        }
        for (key, filename, error) in pending {
            section = section.child(self.render_pending_file_row(key, filename, error, cx));
        }
        section.into_any_element()
    }

    /// One synced file row: type glyph · filename · size · Open / Save as /
    /// Delete.
    fn render_file_row(
        &self,
        attachment: &Attachment,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let id = attachment.id.clone();
        let label = attachment_label(attachment);
        let busy = self.busy_files.contains(&id);
        let glyph = icon_for_content_type(attachment.content_type.as_deref());

        h_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .items_center()
            .bg(theme::tokens::glass::FILL_CARD.to_hsla())
            .child(
                Icon::from(glyph)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(label.clone())),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(format_bytes(
                        attachment.size_bytes.unwrap_or_default(),
                    ))),
            )
            .child({
                let (id, label) = (id.clone(), label.clone());
                Button::new(SharedString::from(format!("issue-file-open-{id}")))
                    .ghost()
                    .xsmall()
                    .disabled(busy)
                    .icon(Icon::from(ExpIcon::ExternalLink).xsmall())
                    .tooltip("Open")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_file(id.clone(), label.clone(), window, cx);
                    }))
            })
            .child({
                let (id, label) = (id.clone(), label.clone());
                Button::new(SharedString::from(format!("issue-file-save-{id}")))
                    .ghost()
                    .xsmall()
                    .disabled(busy)
                    .icon(Icon::from(ExpIcon::Download).xsmall())
                    .tooltip("Save as…")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.save_file_as(id.clone(), label.clone(), window, cx);
                    }))
            })
            .child({
                let (id, label) = (id.clone(), label.clone());
                Button::new(SharedString::from(format!("issue-file-delete-{id}")))
                    .ghost()
                    .xsmall()
                    .disabled(busy)
                    .icon(
                        Icon::from(ExpIcon::Trash2)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .tooltip("Delete")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_delete_file(id.clone(), label.clone(), window, cx);
                    }))
            })
            .into_any_element()
    }

    /// A staged pick: "Uploading…" until the server answers, or the failure
    /// message with a dismiss ✕.
    fn render_pending_file_row(
        &self,
        key: u64,
        filename: String,
        error: Option<SharedString>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let failed = error.is_some();
        let status = error.unwrap_or_else(|| SharedString::from("Uploading…"));
        h_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .items_center()
            .bg(theme::tokens::glass::FILL_CARD.to_hsla())
            .child(
                Icon::from(ExpIcon::File)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(filename)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(if failed {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(status),
            )
            .when(failed, |row| {
                row.child(
                    Button::new(SharedString::from(format!("issue-file-dismiss-{key}")))
                        .ghost()
                        .xsmall()
                        .icon(Icon::new(registry::UI_CLOSE).xsmall())
                        .tooltip("Dismiss")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pending_files.retain(|pending| pending.key != key);
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    /// "Attach file" — the native multi-select picker (same shape as the
    /// editor's image picker); every pick becomes a pending row immediately.
    fn pick_files(&mut self, issue_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            // Receiver error = dialog dismissed/unsupported; None = cancelled.
            let Ok(Ok(paths)) = receiver.await else {
                return;
            };
            let Some(paths) = paths else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                for path in paths {
                    this.start_file_upload(issue_id.clone(), path, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Stage one picked path and upload it in the background. The 50 MB read
    /// happens off the foreground too (a big file would otherwise freeze the
    /// window before the row even appears). Inline-image picks route to the
    /// image upload endpoint and land INLINE at the bottom of the description
    /// (EXP-316) — they never live in the Files rail on any client.
    fn start_file_upload(
        &mut self,
        issue_id: String,
        path: PathBuf,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = self.next_pending_file_key;
        self.next_pending_file_key += 1;
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file")
            .to_string();
        self.pending_files.push(PendingFileUpload {
            key,
            filename,
            uploaded_id: None,
            error: None,
        });
        cx.notify();

        let Some(transport) = queries::attachment_transport(cx) else {
            self.fail_pending_file(key, "Not signed in".into(), cx);
            return;
        };
        let upload_issue = issue_id.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let (filename, content_type, bytes) =
                        crate::markdown::read_any_file(&path)?;
                    if is_inline_image(Some(content_type.as_str())) {
                        // The /images endpoint enforces the inline-image
                        // contract (type + 10 MB cap) atomically.
                        transport
                            .upload(&upload_issue, &filename, &content_type, &bytes)
                            .map(|uploaded| (uploaded, true))
                    } else {
                        transport
                            .upload_file(&upload_issue, &filename, &content_type, &bytes)
                            .map(|uploaded| (uploaded, false))
                    }
                })
                .await;
            this.update_in(cx, |this, window, cx| match result {
                Ok((uploaded, true)) => {
                    // The image is part of the description now — the pending
                    // Files row has nothing to wait for.
                    this.pending_files.retain(|pending| pending.key != key);
                    this.append_image_to_description(issue_id, uploaded, window, cx);
                    cx.notify();
                }
                Ok((uploaded, false)) => {
                    if let Some(pending) = this
                        .pending_files
                        .iter_mut()
                        .find(|pending| pending.key == key)
                    {
                        pending.uploaded_id = Some(uploaded.id);
                    }
                    cx.notify();
                }
                Err(error) => {
                    log::warn!("[ui] file upload failed: {error}");
                    this.fail_pending_file(key, SharedString::from(error.to_string()), cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// EXP-316: append a just-uploaded inline image to the BOTTOM of the
    /// description and persist. Any pending user edit is flushed first so the
    /// append builds on the flushed text and `set_markdown` replaces a clean
    /// buffer (the file dialog already took focus off the editor).
    fn append_image_to_description(
        &mut self,
        issue_id: String,
        uploaded: crate::markdown::UploadedImage,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.flush_description(cx);
        let current = self.last_saved_description.borrow().clone();
        let alt = uploaded
            .filename
            .clone()
            .unwrap_or_else(|| "image".to_string());
        let image_ref = format!("![{alt}]({})", uploaded.url);
        let next = if current.is_empty() {
            image_ref
        } else {
            format!("{current}\n\n{image_ref}")
        };
        if let Some(editor) = self.editor.clone() {
            editor.set_markdown(&next, window, cx);
            editor.mark_clean(cx);
        }
        *self.last_saved_description.borrow_mut() = next.clone();
        let mut input = api::issues::IssuesUpdateInput::new(issue_id);
        input.description = api::Patch::Set(next);
        spawn_issue_update(cx, input);
    }

    fn fail_pending_file(&mut self, key: u64, message: SharedString, cx: &mut gpui::Context<Self>) {
        if let Some(pending) = self
            .pending_files
            .iter_mut()
            .find(|pending| pending.key == key)
        {
            pending.error = Some(message);
        }
        cx.notify();
    }

    /// "Open": fetch the bytes through the auth-gated transport into a
    /// per-attachment temp file and hand the path to the OS. The desktop app
    /// ships no viewers of its own (EXP-297 decision: no video/PDF players).
    fn open_file(
        &mut self,
        attachment_id: String,
        label: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(transport) = queries::attachment_transport(cx) else {
            return;
        };
        self.busy_files.insert(attachment_id.clone());
        cx.notify();
        let url = format!("/api/attachments/{attachment_id}");
        let path = temp_open_path(&attachment_id, &label);
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let write_path = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let bytes = transport.fetch(&url)?;
                    if let Some(parent) = write_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&write_path, bytes)?;
                    anyhow::Ok(())
                })
                .await;
            this.update(cx, |this, cx| {
                this.busy_files.remove(&attachment_id);
                match result {
                    Ok(()) => cx.open_with_system(&path),
                    Err(error) => {
                        log::warn!("[ui] attachment open failed for {attachment_id}: {error}");
                        let note = Notification::error(SharedString::from(format!(
                            "Could not open {label}: {error}"
                        )));
                        let _ = handle.update(cx, |_, window, cx| {
                            window.push_notification(note, cx);
                        });
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// "Save as…" — the native save dialog + a background fetch/write, the
    /// exact shape of the description editor's image download.
    fn save_file_as(
        &mut self,
        attachment_id: String,
        label: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(transport) = queries::attachment_transport(cx) else {
            return;
        };
        let directory = dirs::download_dir().unwrap_or_else(|| PathBuf::from("."));
        let receiver = cx.prompt_for_new_path(&directory, Some(&label));
        let url = format!("/api/attachments/{attachment_id}");
        let handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            // Receiver error = dialog dismissed/unsupported; None = cancelled.
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let write_path = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let bytes = transport.fetch(&url)?;
                    std::fs::write(&write_path, bytes)?;
                    anyhow::Ok(())
                })
                .await;
            let note = match result {
                Ok(()) => Notification::info(SharedString::from(format!(
                    "Saved to {}",
                    path.display()
                ))),
                Err(error) => {
                    log::warn!("[ui] attachment download failed for {attachment_id}: {error}");
                    Notification::error(SharedString::from(format!("Download failed: {error}")))
                }
            };
            let _ = handle.update(cx, |_, window, cx| {
                window.push_notification(note, cx);
            });
        })
        .detach();
    }

    /// The delete confirm (member-level, like every client): the server also
    /// rewrites any markdown still referencing the attachment, so the copy
    /// names that consequence.
    fn confirm_delete_file(
        &mut self,
        attachment_id: String,
        label: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        // The OPENER's window — the alert closes on confirm, so a failure
        // notification has to land back on the detail window.
        let handle = window.window_handle();
        let spec = crate::native_dialog::AlertSpec::new(
            format!("Delete \"{label}\"?"),
            "The file is removed for everyone and its storage is reclaimed. \
             Any description or comment still embedding it keeps a plain-text \
             placeholder.",
            "Delete file",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            let Some(trpc) = queries::trpc_client(cx) else {
                return true;
            };
            let _ = view.update(cx, |this, cx| {
                this.busy_files.insert(attachment_id.clone());
                cx.notify();
            });
            let view = view.clone();
            let attachment_id = attachment_id.clone();
            let label = label.clone();
            cx.spawn(async move |cx| {
                let deleted_id = attachment_id.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        api::attachments::attachments_delete(&trpc, &deleted_id)
                    })
                    .await;
                let _ = view.update(cx, |this, cx| {
                    this.busy_files.remove(&attachment_id);
                    cx.notify();
                });
                if let Err(error) = result {
                    // The row stays — the delete simply did not happen; the
                    // Electric echo is what removes a row that DID.
                    log::warn!("[ui] attachments.delete({attachment_id}) failed: {error}");
                    let note = Notification::error(SharedString::from(format!(
                        "Could not delete {label}: {error}"
                    )));
                    let _ = handle.update(cx, |_, window, cx| {
                        window.push_notification(note, cx);
                    });
                }
            })
            .detach();
            true
        });
        crate::native_dialog::open_alert(window, cx, spec);
    }

    fn render_left_column(
        &mut self,
        issue: &Issue,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        // Borderless 2xl title (web `titleField`). [`DETAIL_GUTTER`] = the one
        // shared left edge for the detail body (title / description / activity
        // / composer all align on it — §8.3, EXP-282).
        let title = div()
            .px(px(DETAIL_GUTTER))
            .pt_3()
            .pb_1()
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
            // Shift+Enter would insert a newline in the auto-grow input
            // (`submit_on_enter` only intercepts plain Enter) — swallow it
            // so no keyboard path can put a newline in a title (EXP-230).
            .capture_action(cx.listener(
                |_, action: &input::Enter, _window, cx: &mut gpui::Context<Self>| {
                    if action.shift {
                        cx.stop_propagation();
                    }
                },
            ))
            // Style the INPUT itself (EXP-181): the widget's own
            // `input_text_size`/`input_px` (text_sm, 12px padding) override
            // wrapper styles, so a size set on the wrapper never reached the
            // text and the extra padding misaligned it against the
            // description's px_4 edge. `refine_style` runs last, so these
            // win; the explicit line height lifts the widget's fixed
            // 1.25rem, which would clip 2xl glyphs, and h_auto releases the
            // fixed h_8 box.
            .child(
                Input::new(&self.title_input)
                    .appearance(false)
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(gpui::rems(2.))
                    .px_0()
                    .h_auto(),
            );

        let column = v_flex()
            .child(title)
            .child(self.render_description(issue, window, cx))
            // EXP-297: the files rail sits under the description and above
            // the timeline — inline images stay in the description itself.
            .child(self.render_files_section(issue, cx));

        // The timeline sits OUTSIDE the centered column and re-centers its own
        // content to the same column width — which is what lets it carry a
        // full-bleed top border spanning the whole pane (EXP-67, dropped by
        // EXP-282, restored by EXP-327 so the activity section reads as its
        // own region on every client).
        v_flex()
            .w_full()
            .child(centered_column(column))
            .child(self.timeline.clone())
    }
}

impl Render for IssueDetailView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Terminal-dock pattern: key context + tracked focus. (The bare-letter
        // J/K switcher bindings are gone — EXP-268; the header arrows remain.)
        // EXP-282: no base fill — the view sits directly on the window's page
        // gradient (`colors.list` is transparent since EXP-269, so the old
        // `.bg()` was a no-op that only obscured the intent).
        let base = v_flex()
            .size_full()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle);

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

        // EXP-277: the header row is gone — the switcher / copy-link /
        // subscribe / actions cluster lives in the properties panel's
        // toolbar row now; the duplicate banner is the first row.
        let mut view = base;
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

/// L27 status interception: selecting a `duplicate`-category status from ANY
/// status control
/// opens the duplicate picker (the server links `duplicate_of_id` and sets
/// `status='duplicate'` atomically) instead of writing the status directly;
/// every other status flows straight through to `issues.update`. Cancelling
/// the picker writes nothing, so the control reverts to the live status. The
/// single interception point shared by the detail properties panel, the row
/// status dropdown and the row context menu (web `useDuplicateInterception`).
pub(crate) fn apply_status_selection(
    issue_id: String,
    pick: crate::pickers::StatusPick,
    window: &mut Window,
    cx: &mut App,
) {
    // EXP-314: the interception now keys on the resolved CATEGORY (only the
    // locked builtin Duplicate row can carry it), not on the enum value.
    if pick.category == domain::statuses::IssueStatusCategory::Duplicate {
        open_duplicate_picker(issue_id, window, cx);
        return;
    }
    let mut input = api::issues::IssuesUpdateInput::new(issue_id);
    pick.apply_to_update(&mut input);
    spawn_issue_update(cx, input);
}

/// Open the shared duplicate-picker dialog for `issue_id`. `pub(crate)` — the
/// §4.6 shared-`IssuePicker` rule: both the detail actions menu and the row
/// `ContextMenu`'s "Mark as duplicate…" item open this same overlay.
pub(crate) fn open_duplicate_picker(issue_id: String, window: &mut Window, cx: &mut App) {
    // EXP-285: trimmed 480 → 420.
    let spec = crate::native_dialog::DialogSpec::new(
        "Mark as duplicate",
        gpui::size(px(480.), px(420.)),
    );
    crate::native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let picker = cx.new(|cx| DuplicatePicker::new(issue_id, window, cx));
        crate::native_dialog::DialogContent::new(picker)
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
            .filter(|issue| issue.id != self.exclude_issue_id)
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
        crate::native_dialog::close_dialog_window(window, cx);
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
/// `pub(crate)` — the properties panel's subscribe toggle reads it (EXP-277).
pub(crate) fn is_subscribed(issue_id: &str, user_id: &str, cx: &App) -> bool {
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

/// The issue's full web URL — `{instance}/t/{team}/boards/{board}/issues/{id}`
/// (the web copy-link button's exact shape). `None` while signed out or before
/// the board/team rows (or their slugs) have synced. `pub(crate)` — the
/// properties panel's copy-link button builds it (EXP-277).
pub(crate) fn issue_web_url(issue: &Issue, cx: &App) -> Option<String> {
    let account = queries::active_account(cx)?;
    let collections = Store::global(cx).collections();
    let board = collections.boards.read(cx).get(&issue.board_id).cloned()?;
    let board_slug = board.slug?;
    let team_slug = collections.teams.read(cx).get(&board.team_id).cloned()?.slug?;
    Some(format!(
        "{}/t/{team_slug}/boards/{board_slug}/issues/{}",
        account.instance_url.trim_end_matches('/'),
        issue.identifier
    ))
}

/// The §4.2 steer presence pill: a "coding now" badge while a
/// `coding_sessions` row is `running` for this issue (the Watch/viewer UI is
/// §08 — another track wires it onto this pill). The parked states render the
/// same pill with a different tone/verb (EXP-194/EXP-214): review GREEN
/// "ready for review" (the in_review issue-status tint), done BLUE once the
/// PR merges, needs-input YELLOW while the agent waits on a plan-approval /
/// question picker.
pub(crate) fn coding_now_pill(issue_id: &str, cx: &App) -> Option<impl IntoElement> {
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

    let pr_state = collections
        .issues
        .read(cx)
        .get(issue_id)
        .and_then(|issue| issue.pr_state.clone());
    let (verb, tone) =
        match crate::queries::coding_session_display(&session, pr_state.as_deref()) {
            crate::queries::CodingSessionDisplay::NeedsInput => {
                ("needs input", theme::tokens::YELLOW)
            }
            crate::queries::CodingSessionDisplay::Review => {
                ("ready for review", theme::tokens::GREEN)
            }
            crate::queries::CodingSessionDisplay::Done => ("done", theme::tokens::BLUE),
            crate::queries::CodingSessionDisplay::Running => ("coding now", theme::tokens::GREEN),
        };

    let who = session
        .user_id
        .as_deref()
        .and_then(|id| collections.users.read(cx).get(id).cloned())
        .map(|user| comments::author_label(Some(&user)));
    let label = match (who, session.device_label.as_deref()) {
        (Some(who), Some(device)) => format!("{who} {verb} · {device}"),
        (Some(who), None) => format!("{who} {verb}"),
        (None, Some(device)) => {
            let mut capitalized = capitalize_first(verb);
            capitalized.push_str(&format!(" · {device}"));
            capitalized
        }
        (None, None) => capitalize_first(verb),
    };

    // EXP-309: the pill is a full-width sidebar row like every other EXP-282
    // control, and its label ellipsizes. A content-sized `flex_shrink_0` pill
    // spilled past the 220px sidebar as soon as the label carried a name AND a
    // device ("Danny Strähhuber needs input · MacBook Pro"). Truncation needs
    // the whole width chain definite: `w_full` + `min_w_0` on the row, then
    // `flex_1 min_w_0 overflow_hidden` on the text div itself.
    Some(
        h_flex()
            .w_full()
            .min_w_0()
            .gap_1p5()
            .px_2()
            .py_0p5()
            .rounded_full()
            .border_1()
            .border_color(tone.to_hsla().opacity(0.4))
            .items_center()
            .text_xs()
            .child(
                div()
                    .flex_shrink_0()
                    .size_1p5()
                    .rounded_full()
                    .bg(tone.to_hsla()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(label)),
            ),
    )
}

/// "ready for review" → "Ready for review" (the who-less pill variants).
fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}


#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;

    /// A minimal seam editor whose raw buffer still holds mid-upload
    /// `draft://` placeholders.
    struct StubEditor(&'static str);

    impl DescriptionEditor for StubEditor {
        fn set_markdown(&self, _markdown: &str, _window: &mut Window, _cx: &mut App) {}
        fn markdown(&self, _cx: &App) -> String {
            self.0.to_string()
        }
        fn is_focused(&self, _window: &Window, _cx: &App) -> bool {
            false
        }
        fn element(&self, _window: &mut Window, _cx: &mut App) -> gpui::AnyElement {
            unreachable!("not rendered in this test")
        }
        fn focus(&self, _window: &mut Window, _cx: &mut App) {}
    }

    // EXP-261 regression (BUG 1): `flush_description` persists
    // `markdown_for_save`, never raw `markdown()`. The trait's shared default
    // derivation strips a mid-upload draft (inline AND standalone) for every
    // seam editor, so a tab/issue switch can never write `draft://` bytes to
    // the server even though the raw buffer still holds them.
    #[gpui::test]
    async fn flush_derivation_strips_drafts_raw_markdown_still_has(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let editor = StubEditor("before\n\n- ![img](draft://u1) item\n\n![shot](draft://u2)");
            assert!(editor.markdown(cx).contains("draft://"));
            let for_save = editor.markdown_for_save(cx);
            assert!(!for_save.contains("draft://"));
            assert_eq!(for_save, "before\n\n-  item");

            // Draft-free content (tables included) passes through untouched.
            let clean = StubEditor("| a | b |\n| --- | --- |\n| 1 | 2 |");
            assert_eq!(clean.markdown_for_save(cx), clean.markdown(cx));
        });
    }
}
