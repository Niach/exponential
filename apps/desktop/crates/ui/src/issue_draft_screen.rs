//! EXP-1170 — the NEW ISSUE page: the issue detail in DRAFT mode
//! ([`Screen::IssueDraft`]), ONE view ×4 (contract
//! `fixtures/issue-draft.json`, copy in [`domain::issue_draft`]).
//!
//! There is no create dialog any more. Every "New issue" opener (the rail's
//! button, the `NewIssue` action, a Drafts row) mints or carries a draft id
//! and NAVIGATES here ([`open_new`], [`open_new_from_rail`],
//! [`open_existing`]). The page IS the detail's layout — the floating bar
//! over the scrolling body, the large title row, the property tray, the
//! description, the Files — with only these differences: the collapsed title
//! reads "New issue" over the typed title (or "Untitled draft"), the bar's
//! cluster is a primary Create plus an `×` tooltipped "Discard draft", the
//! tray is the [`IssueDraft`] chip row (no estimate) plus a board chip when
//! the team has another board, and nothing follows the Files (no relations,
//! composer, PR row or timeline).
//!
//! The draft itself — identity, eager uploads, files and the coalesced
//! AUTOSAVE — is [`DraftEditor`]. This view feeds it snapshots: an edit
//! schedules one, a chip or board pick and a blur save at once, and every
//! way off the page ([`IssueDraftView::leave`], called by the screens panel
//! on any navigation and by the view's release) pays the draft out.
//!
//! ONE shared instance per window ([`crate::screens::ScreensPanel`]),
//! re-pointed per draft like the PR diff ([`IssueDraftView::set_draft`]).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Entity, FocusHandle, Focusable,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _, Icon,
};
use sync::Store;

use domain::issue_draft as copy;
use domain::rows::IssueDraftRow;

use crate::controls::WebControl as _;
use crate::draft_editor::{DraftEditor, DraftEditorEvent, LeaveAction};
use crate::drafts::DraftSave;
use crate::icons::registry;
use crate::issue_detail::{centered_column, WYSIWYG_BLOCK_PADDING_X};
use crate::work_header::WORK_GUTTER;
use crate::issue_draft::IssueDraft;
use crate::markdown::image_paste::{markdown_for_save, strip_draft_images};
use crate::navigation::{nav_for_window, resolved_screen, Navigation, Screen};
use crate::wysiwyg::WysiwygDescription;

// ---------------------------------------------------------------------------
// openers
// ---------------------------------------------------------------------------

/// "New issue" on `board_id` (empty = the window's active board): mint the
/// draft id NOW and navigate to the page. Opened from a board, the page keeps
/// that board's rows beside it (the EXP-851 breadcrumb). Nothing is written
/// until the page has content.
pub(crate) fn open_new(
    window: &mut Window,
    cx: &mut App,
    board_id: String,
    status_id: Option<String>,
) {
    crate::navigation::navigate(
        window,
        cx,
        Screen::IssueDraft {
            draft_id: api::issue_drafts::new_draft_id(),
            board_id,
            status_id,
        },
    );
}

/// The rail's "New issue" button: the same page, with no list beside it (the
/// rail is not a list).
pub(crate) fn open_new_from_rail(window: &mut Window, cx: &mut App, board_id: String) {
    crate::navigation::navigate_from_rail(
        window,
        cx,
        Screen::IssueDraft {
            draft_id: api::issue_drafts::new_draft_id(),
            board_id,
            status_id: None,
        },
    );
}

/// A Drafts row: reopen the saved draft on its own board, under its own id —
/// the page seeds from the row and writes back to it.
pub(crate) fn open_existing(window: &mut Window, cx: &mut App, draft: &IssueDraftRow) {
    let Some(board_id) = draft.board_id.clone() else {
        return;
    };
    crate::navigation::navigate(
        window,
        cx,
        Screen::IssueDraft {
            draft_id: draft.id.clone(),
            board_id,
            status_id: None,
        },
    );
}

// ---------------------------------------------------------------------------
// the view
// ---------------------------------------------------------------------------

/// Which draft the page shows, and where it would be filed.
#[derive(Clone, Debug)]
struct DraftTarget {
    draft_id: String,
    board_id: String,
    team_id: String,
}

/// A `set_draft` whose board has not synced yet (a cold start straight onto
/// the page) — retried when the boards land.
#[derive(Clone, Debug)]
struct PendingDraft {
    draft_id: String,
    board_id: String,
    status_id: Option<String>,
}

/// The per-draft half of the view, rebuilt for every draft id.
struct DraftParts {
    target: DraftTarget,
    description: Entity<WysiwygDescription>,
    props: Entity<IssueDraft>,
    editor: Entity<DraftEditor>,
    _subscriptions: Vec<Subscription>,
}

pub(crate) struct IssueDraftView {
    nav: Entity<Navigation>,
    focus_handle: FocusHandle,
    /// The body's scroll position (reset per draft — one shared instance).
    body_scroll: gpui::ScrollHandle,
    /// EXP-1162: the title row's measured content bottom / the bar cluster's
    /// measured width (`work_header::scrolling_title_rows`).
    title_bottom: Rc<std::cell::Cell<Option<f32>>>,
    /// EXP-1191: the tray slot's top and height (it pins under the bar).
    tray_top: Rc<std::cell::Cell<Option<f32>>>,
    tray_h: Rc<std::cell::Cell<Option<f32>>>,
    cluster_w: Rc<std::cell::Cell<Option<f32>>>,
    title: Entity<TextareaState>,
    parts: Option<DraftParts>,
    pending: Option<PendingDraft>,
    /// The page is on screen — a leave pays the draft out exactly once.
    active: bool,
    /// A Create is in flight (the button disables; autosave pauses).
    creating: bool,
    /// The title takes focus once per open (web `autoFocus`).
    focused_once: bool,
    /// Draft attachment ids with an Open request in flight.
    busy_files: HashSet<String>,
    /// The last snapshot of every draft this view LEFT with content (its
    /// write may not have synced yet): Back to such a draft seeds from here
    /// rather than opening it blank and overwriting the save.
    written: HashMap<String, DraftSave>,
    _subscriptions: Vec<Subscription>,
}

/// Everything a Create needs, captured when it starts — the view may be
/// re-pointed at another draft before the create returns, and the create
/// must file (and mark) THIS draft, never whatever the page shows by then.
struct CreateCapture {
    target: DraftTarget,
    title: String,
    editor: Entity<DraftEditor>,
    props: Entity<IssueDraft>,
    description: Entity<WysiwygDescription>,
}

/// The full row of a draft — what every write sends.
fn build_snapshot(
    target: &DraftTarget,
    title: &str,
    props: &Entity<IssueDraft>,
    description: &Entity<WysiwygDescription>,
    cx: &App,
) -> DraftSave {
    let props = props.read(cx);
    DraftSave {
        id: target.draft_id.clone(),
        team_id: target.team_id.clone(),
        board_id: target.board_id.clone(),
        title: title.trim().to_string(),
        // Never a `draft://` url on the wire: a staged image is still local
        // bytes.
        description: markdown_for_save(description.read(cx).markdown(cx)),
        status_id: props.status.status_id.clone(),
        priority: props.priority,
        assignee_id: props.assignee_id.clone(),
        label_ids: props.selected_label_ids.clone(),
        due_date: props.due_date,
    }
}

impl IssueDraftView {
    pub(crate) fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // The detail's title input, verbatim: auto-grow so a long title
        // soft-wraps, one LOGICAL line (`submit_on_enter` turns Enter into an
        // event, the row swallows Shift+Enter).
        let title = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(copy::TITLE_PLACEHOLDER)
                .auto_grow(1, 5)
                .submit_on_enter(true)
        });
        let nav = nav_for_window(window, cx);
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe_in(
            &title,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    if let Some(snapshot) = this.snapshot(cx) {
                        this.with_editor(cx, |editor, cx| editor.schedule_save(snapshot, cx));
                    }
                    // The Create gate and the collapsed title follow it.
                    cx.notify();
                }
                InputEvent::Blur => this.save_now(cx),
                // Enter moves on to the description; Cmd+Enter is the
                // root's Create (the key capture below).
                InputEvent::PressEnter {
                    secondary: false, ..
                } => this.focus_description(window, cx),
                _ => {}
            },
        ));
        // A cold start straight onto the page can beat the boards shape —
        // the pending open retries when it lands.
        if let Some(collections) = Store::try_global(cx).map(|store| store.collections().clone()) {
            let boards = collections.boards.clone();
            subscriptions.push(cx.observe_in(&boards, window, |this, _, window, cx| {
                // Only while the page is still up: a pending open the user
                // already left must not come back to life.
                if let Some(pending) = this.pending.clone().filter(|_| this.active) {
                    this.set_draft(pending.draft_id, pending.board_id, pending.status_id, window, cx);
                }
                cx.notify();
            }));
            // The Files rows and the chips' option lists read these.
            let statuses = collections.issue_statuses.clone();
            subscriptions.push(cx.observe(&statuses, |_, _, cx| cx.notify()));
        }
        // The window closing drops the view without a navigation — the last
        // place every way off the page meets. The leave goes through the
        // editor's own queue (its in-flight write holds it alive until the
        // queue drains), so it can never land before an older write.
        cx.on_release(|this, cx| {
            if !this.active {
                return;
            }
            let Some(snapshot) = this.snapshot(cx) else {
                return;
            };
            if let Some(parts) = &this.parts {
                parts.editor.update(cx, |editor, cx| {
                    editor.leave(snapshot, cx);
                });
            }
        })
        .detach();
        Self {
            nav,
            focus_handle: cx.focus_handle(),
            body_scroll: gpui::ScrollHandle::new(),
            title_bottom: Rc::new(std::cell::Cell::new(None)),
            tray_top: Rc::new(std::cell::Cell::new(None)),
            tray_h: Rc::new(std::cell::Cell::new(None)),
            cluster_w: Rc::new(std::cell::Cell::new(None)),
            title,
            parts: None,
            pending: None,
            active: false,
            creating: false,
            focused_once: false,
            busy_files: HashSet::new(),
            written: HashMap::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Point the page at a draft (the screens panel calls this on every
    /// navigation that lands here). The SAME id resumes the page as it was
    /// left (Back then Forward keeps everything); a different id pays the
    /// previous draft out first, then seeds ONCE — from the synced row when
    /// one exists (a Drafts row), blank otherwise — and never re-seeds from a
    /// later sync echo.
    pub(crate) fn set_draft(
        &mut self,
        draft_id: String,
        board_id: String,
        status_id: Option<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self
            .parts
            .as_ref()
            .is_some_and(|parts| parts.target.draft_id == draft_id)
        {
            if !self.active {
                self.active = true;
                self.focused_once = false;
                cx.notify();
            }
            return;
        }
        self.leave(cx);

        let collections = Store::global(cx).collections().clone();
        // The synced row, else the snapshot this view left the draft with
        // (its write has not synced yet) — either way the draft EXISTS.
        let row = collections
            .issue_drafts
            .read(cx)
            .get(&draft_id)
            .cloned()
            .or_else(|| self.written.get(&draft_id).map(DraftSave::as_row));
        // The board: the row's own, else the screen's, else (the empty
        // sentinel) the window's active one.
        let board_id = row
            .as_ref()
            .and_then(|row| row.board_id.clone())
            .filter(|id| !id.is_empty())
            .or_else(|| (!board_id.is_empty()).then(|| board_id.clone()))
            .or_else(|| crate::navigation::active_board_id(&self.nav, cx));
        let board = board_id
            .as_deref()
            .and_then(|id| collections.boards.read(cx).get(id).cloned());
        let Some(board) = board else {
            if collections.boards.read(cx).is_ready() {
                // The boards have synced and this one is not among them
                // (deleted, or no board in scope at all): there is no page to
                // show — go back rather than leave a dead skeleton up.
                self.pending = None;
                self.parts = None;
                self.active = false;
                let nav = self.nav.clone();
                window.defer(cx, move |window, cx| {
                    if nav.read(cx).can_go_back() {
                        crate::navigation::go_back(window, cx);
                    } else {
                        crate::navigation::set_screen(window, cx, None);
                    }
                });
                return;
            }
            // Not synced yet — the boards observer retries.
            self.pending = Some(PendingDraft {
                draft_id,
                board_id: board_id.unwrap_or_default(),
                status_id,
            });
            self.parts = None;
            self.active = true;
            cx.notify();
            return;
        };
        self.pending = None;
        let seed = row.as_ref().map(|row| crate::drafts::seed_from_row(row, cx));
        let team_id = board.team_id.clone();

        self.title.update(cx, |title, cx| {
            title.set_value(
                seed.as_ref().map(|seed| seed.title.as_str()).unwrap_or(""),
                window,
                cx,
            );
        });
        let view = cx.entity().downgrade();
        // Blur (and the editor's structural commits) save at once. Deferred:
        // the hook runs inside the editor's own update, and the snapshot
        // reads the editor back.
        let on_save: crate::wysiwyg::OnSave = {
            let view = view.clone();
            Rc::new(move |_markdown, window, cx| {
                let view = view.clone();
                window.defer(cx, move |_window, cx| {
                    let _ = view.update(cx, |this, cx| this.save_now(cx));
                });
            })
        };
        let description = crate::description_editor::build_wysiwyg_editor(
            Some(team_id.clone()),
            None,
            copy::DESCRIPTION_PLACEHOLDER,
            seed.as_ref()
                .map(|seed| seed.description.as_str())
                .unwrap_or(""),
            Some(on_save),
            window,
            cx,
        );
        // The editor's paperclip: images embed inline through the editor
        // itself; everything else uploads onto the draft.
        {
            let view = view.clone();
            description.update(cx, |description, _| {
                description.set_attach_handler(Rc::new(move |paths, window, cx| {
                    let _ = view.update(cx, |this, cx| this.attach_paths(paths, window, cx));
                }));
            });
        }
        let props = cx.new(|cx| IssueDraft::new(team_id.clone(), window, cx));
        match &seed {
            Some(seed) => props.update(cx, |props, cx| props.apply_seed(seed, window, cx)),
            None => {
                if let Some(status_id) = status_id.as_deref() {
                    let pick = crate::queries::team_status_options(cx, &team_id)
                        .into_iter()
                        .find(|status| status.row_id.as_deref() == Some(status_id))
                        .map(|status| crate::pickers::StatusPick::from_resolved(&status));
                    if let Some(pick) = pick {
                        props.update(cx, |props, cx| {
                            props.status = pick;
                            cx.notify();
                        });
                    }
                }
            }
        }
        let window_handle = window.window_handle();
        let editor = cx.new(|cx| {
            DraftEditor::new(draft_id.clone(), row.is_some(), Some(window_handle), cx)
        });

        let subscriptions = vec![
            // An edit schedules a save; a freshly pasted image goes up.
            cx.observe(&description, |this, _, cx| this.on_description_change(cx)),
            // A chip pick saves at once.
            cx.observe(&props, |this, _, cx| {
                this.save_now(cx);
                cx.notify();
            }),
            cx.observe(&editor, |_, _, cx| cx.notify()),
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| {
                this.on_editor_event(event, window, cx);
            }),
        ];
        self.parts = Some(DraftParts {
            target: DraftTarget {
                draft_id,
                board_id: board.id.clone(),
                team_id,
            },
            description,
            props,
            editor,
            _subscriptions: subscriptions,
        });
        if let Some(snapshot) = self.snapshot(cx) {
            self.with_editor(cx, |editor, _| editor.prime(snapshot));
        }
        self.active = true;
        self.creating = false;
        self.focused_once = false;
        self.busy_files.clear();
        self.title_bottom.set(None);
        self.tray_top.set(None);
        self.tray_h.set(None);
        self.body_scroll.set_offset(gpui::Point::default());
        cx.notify();
    }

    /// Leaving the page (any navigation away, a team switch): pay the draft
    /// out — the ONE `draft_editor::leave_action` — and remember what was
    /// left behind. Idempotent: only an ACTIVE page leaves.
    pub(crate) fn leave(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.active {
            return;
        }
        self.active = false;
        self.pending = None;
        let Some(snapshot) = self.snapshot(cx) else {
            return;
        };
        let id = snapshot.id.clone();
        let action = self.with_editor(cx, |editor, cx| editor.leave(snapshot.clone(), cx));
        match action {
            Some(LeaveAction::Save) => {
                self.written.insert(id, snapshot);
            }
            Some(LeaveAction::Delete) => {
                self.written.remove(&id);
            }
            Some(LeaveAction::Nothing) | None => {}
        }
    }

    /// The full row as it stands — what every write sends.
    fn snapshot(&self, cx: &App) -> Option<DraftSave> {
        let parts = self.parts.as_ref()?;
        Some(build_snapshot(
            &parts.target,
            &self.title.read(cx).value(),
            &parts.props,
            &parts.description,
            cx,
        ))
    }

    /// Is the page (still) showing `draft_id`?
    fn shows(&self, draft_id: &str) -> bool {
        self.parts
            .as_ref()
            .is_some_and(|parts| parts.target.draft_id == draft_id)
    }

    fn with_editor<R>(
        &self,
        cx: &mut gpui::Context<Self>,
        f: impl FnOnce(&mut DraftEditor, &mut gpui::Context<DraftEditor>) -> R,
    ) -> Option<R> {
        let editor = self.parts.as_ref()?.editor.clone();
        Some(editor.update(cx, f))
    }

    fn save_now(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.active {
            return;
        }
        if let Some(snapshot) = self.snapshot(cx) {
            self.with_editor(cx, |editor, cx| editor.save_now(snapshot, cx));
        }
    }

    /// Every description notify: claim newly pasted images into the eager
    /// upload queue, and schedule a save when the text moved (the editor's
    /// `schedule_save` tells an edit from a repaint).
    fn on_description_change(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.active {
            return;
        }
        let Some(parts) = self.parts.as_ref() else {
            return;
        };
        let staged = parts.description.read(cx).staged_images(cx);
        let Some(snapshot) = self.snapshot(cx) else {
            return;
        };
        for image in staged {
            let snapshot = snapshot.clone();
            self.with_editor(cx, |editor, cx| editor.queue_image(image, snapshot, cx));
        }
        self.with_editor(cx, |editor, cx| editor.schedule_save(snapshot, cx));
        cx.notify();
    }

    fn on_editor_event(
        &mut self,
        event: &DraftEditorEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(description) = self.parts.as_ref().map(|parts| parts.description.clone()) else {
            return;
        };
        match event {
            DraftEditorEvent::ImageUploaded { staged, url } => {
                description.update(cx, |description, cx| {
                    description.adopt_uploaded_image(staged, url, window, cx);
                });
            }
            DraftEditorEvent::ImageFailed { staged, message } => {
                description.update(cx, |description, cx| {
                    description.drop_failed_image(staged, message, window, cx);
                });
            }
            DraftEditorEvent::MediaUploaded { fragment } => {
                description.update(cx, |description, cx| {
                    description.append_paragraph(fragment, window, cx);
                });
                // A programmatic append is no edit the observer can see as
                // one — save the new description now.
                self.save_now(cx);
            }
            DraftEditorEvent::FileUploaded
            | DraftEditorEvent::FilesLoaded
            | DraftEditorEvent::Saved => {}
        }
        cx.notify();
    }

    fn focus_description(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(parts) = &self.parts {
            parts
                .description
                .update(cx, |description, cx| description.focus(window, cx));
        }
    }

    /// Picked paths (the editor's paperclip, the Files section's): read off
    /// the foreground (the 50 MB cap), then upload onto the draft. Never
    /// reads the description synchronously — this can run inside its update.
    fn attach_paths(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        for path in paths {
            cx.spawn_in(window, async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { crate::markdown::read_any_file(&path) })
                    .await;
                let _ = this.update_in(cx, |this, window, cx| match result {
                    Ok((filename, content_type, bytes)) => {
                        let Some(snapshot) = this.snapshot(cx) else {
                            return;
                        };
                        this.with_editor(cx, |editor, cx| {
                            editor.queue_file(filename, content_type, bytes, snapshot, cx)
                        });
                    }
                    Err(error) => crate::toast::error(format!("{error}"), window, cx),
                });
            })
            .detach();
        }
    }

    /// The Files section's paperclip: the native multi-select picker.
    fn pick_files(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| this.attach_paths(paths, window, cx));
        })
        .detach();
    }

    /// "Open": fetch the bytes through the auth-gated transport into a temp
    /// file and hand the path to the OS (the detail's `open_file`).
    fn open_file(
        &mut self,
        attachment_id: String,
        label: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(transport) = crate::queries::attachment_transport(cx) else {
            return;
        };
        self.busy_files.insert(attachment_id.clone());
        cx.notify();
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let fetch_id = attachment_id.clone();
            let fetch_label = label.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    crate::issue_files::fetch_attachment_to_temp(
                        transport.as_ref(),
                        &fetch_id,
                        &fetch_label,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy_files.remove(&attachment_id);
                match result {
                    Ok(path) => cx.open_with_system(&path),
                    Err(error) => {
                        log::warn!("[ui] draft file open failed for {attachment_id}: {error}");
                        let message = format!("Could not open {label}: {error}");
                        let _ = handle.update(cx, |_, window, cx| {
                            crate::toast::error(message, window, cx);
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- create / discard -------------------------------------------------------

    fn can_create(&self, cx: &App) -> bool {
        let Some(parts) = self.parts.as_ref() else {
            return false;
        };
        // Create waits for uploads (web, iOS and Android do the same): a
        // file still going up would land on a draft the create deleted.
        !self.creating
            && !parts.editor.read(cx).uploads_busy()
            && !self.title.read(cx).value().trim().is_empty()
    }

    /// Create: settle the draft (timer dropped, in-flight writes done, does
    /// the row exist?), then file it with `draftId` when it does — the server
    /// reparents the draft's attachments and deletes the row in the create's
    /// own transaction. Everything is CAPTURED here: the page may be on
    /// another draft by the time the create returns, and only THIS draft is
    /// filed and marked. Ok → the page BECOMES the issue (replace-navigation,
    /// the draft purged from history); Err → a toast, the button comes back.
    fn create(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.can_create(cx) {
            return;
        }
        let Some(parts) = self.parts.as_ref() else {
            return;
        };
        let capture = CreateCapture {
            target: parts.target.clone(),
            title: self.title.read(cx).value().trim().to_string(),
            editor: parts.editor.clone(),
            props: parts.props.clone(),
            description: parts.description.clone(),
        };
        self.creating = true;
        cx.notify();
        // The capture holds the editor STRONGLY, so a re-point mid-flush
        // never drops it (and the flush never answers for a dead draft).
        let flush = capture.editor.update(cx, |editor, cx| editor.flush_for_create(cx));
        cx.spawn_in(window, async move |this, cx| {
            let row_exists = flush.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.spawn_create(capture, row_exists, window, cx);
            });
        })
        .detach();
    }

    fn spawn_create(
        &mut self,
        capture: CreateCapture,
        row_exists: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let target = capture.target.clone();
        let mut input =
            api::issues::IssuesCreateInput::new(target.board_id.clone(), capture.title.clone());
        capture.props.read(cx).apply_to_create(&mut input);
        input.draft_id = row_exists.then(|| target.draft_id.clone());
        let markdown = capture.description.read(cx).markdown(cx);
        let stripped_description = strip_draft_images(&markdown);
        if !stripped_description.is_empty() {
            input.description = Some(stripped_description.clone());
        }
        // Every image is already a draft-owned `/api/attachments/{id}` row;
        // anything still staged (pasted during the create) goes up
        // post-create instead, like the sub-issue composer's.
        let staged_images = capture.description.read(cx).staged_images(cx);

        let view = cx.entity().downgrade();
        crate::issue_draft::spawn_create(
            crate::issue_draft::CreateJob {
                input,
                markdown,
                stripped_description,
                staged_images,
                staged_files: Vec::new(),
            },
            window,
            cx,
            move |result, window, cx| match result {
                Ok(issue_id) => match view.upgrade() {
                    Some(view) => view.update(cx, |this, cx| {
                        this.on_created(issue_id, &capture, window, cx);
                    }),
                    None => capture.editor.update(cx, |editor, _| editor.mark_filed()),
                },
                Err(message) => {
                    crate::toast::error(message, window, cx);
                    let current = view
                        .upgrade()
                        .and_then(|view| {
                            view.update(cx, |this, cx| {
                                if !this.shows(&capture.target.draft_id) {
                                    return None;
                                }
                                this.creating = false;
                                cx.notify();
                                this.snapshot(cx)
                            })
                        });
                    // The page's own state when it still shows the draft,
                    // else the state the create was started with.
                    let snapshot = current.unwrap_or_else(|| {
                        build_snapshot(
                            &capture.target,
                            &capture.title,
                            &capture.props,
                            &capture.description,
                            cx,
                        )
                    });
                    capture
                        .editor
                        .update(cx, |editor, cx| editor.create_failed(snapshot, cx));
                }
            },
        );
    }

    /// The draft is an issue now: nothing may write it again, the window
    /// scopes to its board, and — when the page still shows it — the page
    /// becomes the issue's detail without a history entry for the draft.
    fn on_created(
        &mut self,
        issue_id: String,
        capture: &CreateCapture,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let draft_id = capture.target.draft_id.clone();
        capture.editor.update(cx, |editor, _| editor.mark_filed());
        self.written.remove(&draft_id);
        if self.shows(&draft_id) {
            // A page re-entered mid-create runs a NEW editor for the draft —
            // it is filed too.
            self.with_editor(cx, |editor, _| editor.mark_filed());
            self.creating = false;
            self.active = false;
        }
        let still_here = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::IssueDraft { draft_id: id, .. }) if id == draft_id
        );
        if still_here {
            crate::navigation::set_active_board(window, cx, capture.target.board_id.clone());
            crate::navigation::navigate_replace(window, cx, Screen::IssueDetail { issue_id });
        }
        purge_draft(&draft_id, window, cx);
        cx.notify();
    }

    /// "Discard draft": the row goes (no confirm — a draft is unfiled by
    /// definition), then Back — or the board, with nowhere to go back to.
    fn discard(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(parts) = self.parts.as_ref() else {
            return;
        };
        let target = parts.target.clone();
        self.with_editor(cx, |editor, cx| editor.discard(cx));
        self.written.remove(&target.draft_id);
        self.active = false;
        if self.nav.read(cx).can_go_back() {
            crate::navigation::go_back(window, cx);
        } else {
            crate::navigation::navigate(
                window,
                cx,
                Screen::BoardIssues {
                    board_id: target.board_id.clone(),
                },
            );
        }
        purge_draft(&target.draft_id, window, cx);
    }

    // -- render -----------------------------------------------------------------

    /// The bar's right cluster: Create, then an `×` that discards (EXP-1191:
    /// the draft's only action, so no one-item `…` menu; its tooltip says
    /// what it does).
    fn cluster(&self, cx: &mut gpui::Context<Self>) -> Vec<AnyElement> {
        let enabled = self.can_create(cx);
        let create = Button::new("draft-create")
            .primary()
            .web_sm()
            .label(copy::CREATE)
            .disabled(!enabled)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.create(window, cx)));
        let discard = crate::controls::ghost_icon_button(
            "draft-discard",
            Icon::new(registry::UI_CLOSE),
            cx,
        )
        .tooltip(copy::DISCARD)
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.discard(window, cx)));
        // EXP-1191: the `×` glyph, not its hit box, ends on the content edge.
        let discard = div()
            .mr(px(-crate::work_header::GHOST_ICON_HANG))
            .child(discard);
        vec![create.into_any_element(), discard.into_any_element()]
    }

    /// The property tray: the [`IssueDraft`] chips plus the board chip.
    fn tray(&self, parts: &DraftParts, cx: &mut gpui::Context<Self>) -> AnyElement {
        let mut chips = parts.props.update(cx, |props, cx| props.chips("draft", cx));
        let board = Store::global(cx)
            .collections()
            .boards
            .read(cx)
            .get(&parts.target.board_id)
            .cloned();
        if let Some(board) = board {
            let view = cx.entity().downgrade();
            let on_pick: Rc<dyn Fn(String, &mut Window, &mut App)> =
                Rc::new(move |board_id, window, cx| {
                    let draft_id = view
                        .update(cx, |this, cx| {
                            let parts = this.parts.as_mut()?;
                            parts.target.board_id = board_id.clone();
                            let draft_id = parts.target.draft_id.clone();
                            this.save_now(cx);
                            cx.notify();
                            Some(draft_id)
                        })
                        .ok()
                        .flatten();
                    // The screen follows the pick, so `active_board_id` does.
                    if let Some(draft_id) = draft_id {
                        crate::navigation::set_draft_board(window, cx, &draft_id, &board_id);
                    }
                });
            chips.extend(IssueDraft::board_chip("draft", &board, on_pick, cx));
        }
        crate::work_header::property_tray(chips, Vec::new())
    }

    /// The description slot (the detail's insets and textarea floor).
    fn description_slot(&self, parts: &DraftParts) -> AnyElement {
        div()
            .px(px(WORK_GUTTER - WYSIWYG_BLOCK_PADDING_X))
            .min_h(px(96.))
            .flex()
            .flex_col()
            .cursor(gpui::CursorStyle::IBeam)
            .child(parts.description.clone())
            .into_any_element()
    }

    /// The draft's Files: uploaded attachments (Open / Save as / Delete) and
    /// the picks still going up, under the detail's "Files" header with its
    /// paperclip. Nothing to list → nothing rendered (the editor's own
    /// paperclip attaches).
    fn files_section(&self, parts: &DraftParts, cx: &mut gpui::Context<Self>) -> AnyElement {
        let editor = parts.editor.read(cx);
        let files = editor.files().to_vec();
        let pending = editor.pending().to_vec();
        if files.is_empty() && pending.is_empty() {
            return gpui::Empty.into_any_element();
        }
        let team_id = parts.target.team_id.clone();
        let attach = crate::controls::ghost_icon_button(
            "draft-files-attach",
            Icon::from(registry::UI_ATTACH),
            cx,
        )
        .tooltip("Attach file")
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.pick_files(window, cx)));
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
            // EXP-1191: the paperclip GLYPH ends on the content edge.
            .child(
                div()
                    .mr(px(-crate::work_header::GHOST_ICON_HANG))
                    .child(attach),
            );
        let mut section = v_flex()
            .w_full()
            .px(px(WORK_GUTTER))
            .pt_2()
            .gap_1()
            .child(header);
        for file in files {
            let preview = crate::issue_files::is_markdown_attachment(
                file.content_type.as_deref(),
                Some(file.filename.as_str()),
            )
            .then(|| crate::attachment_markdown_preview::MarkdownPreviewTarget {
                attachment_id: file.id.clone(),
                filename: file.filename.clone(),
                size_bytes: file.size_bytes,
                team_id: Some(team_id.clone()),
            });
            let open_view = cx.entity().downgrade();
            let delete_editor = parts.editor.downgrade();
            let (open_id, open_label) = (file.id.clone(), file.filename.clone());
            let delete_id = file.id.clone();
            section = section.child(crate::issue_files::file_row(
                crate::issue_files::FileRow {
                    id: file.id.clone(),
                    label: file.filename.clone(),
                    content_type: file.content_type.clone(),
                    size_bytes: file.size_bytes,
                    busy: self.busy_files.contains(&file.id),
                    preview,
                },
                move |window, cx| {
                    let _ = open_view.update(cx, |this, cx| {
                        this.open_file(open_id.clone(), open_label.clone(), window, cx);
                    });
                },
                // An unfiled draft's file goes without a confirm (it was
                // the dialog's chip ✕ before).
                move |_window, cx| {
                    let _ = delete_editor
                        .update(cx, |editor, cx| editor.remove_file(delete_id.clone(), cx));
                },
                cx,
            ));
        }
        for file in pending {
            let editor = parts.editor.downgrade();
            let key = file.key;
            section = section.child(crate::issue_files::pending_file_row(
                key,
                file.filename,
                file.error,
                move |_window, cx| {
                    let _ = editor.update(cx, |editor, cx| editor.dismiss_pending(key, cx));
                },
                cx,
            ));
        }
        section.into_any_element()
    }
}

/// Drop every history entry for `draft_id` — a filed or discarded draft must
/// not be re-enterable through Back or Forward.
fn purge_draft(draft_id: &str, window: &Window, cx: &mut App) {
    let draft_id = draft_id.to_string();
    crate::navigation::purge_from_history(window, cx, move |screen| {
        matches!(screen, Screen::IssueDraft { draft_id: id, .. } if *id == draft_id)
    });
}

impl Focusable for IssueDraftView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for IssueDraftView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let base = v_flex()
            .size_full()
            .track_focus(&self.focus_handle)
            // Cmd+Enter = Create, from the title, the description or a chip.
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let keystroke = &event.keystroke;
                if keystroke.key == "enter" && keystroke.modifiers.secondary() {
                    cx.stop_propagation();
                    this.create(window, cx);
                }
            }));
        if self.parts.is_none() {
            // The board has not synced yet (a cold start onto the page).
            return base
                .child(
                    v_flex()
                        .p_4()
                        .gap_2()
                        .child(crate::controls::skeleton().h_4().w_48())
                        .child(crate::controls::skeleton().h_4().w_64()),
                )
                .into_any_element();
        }
        // Web `autoFocus` on the title, once per open.
        if !self.focused_once {
            self.focused_once = true;
            self.title.update(cx, |title, cx| title.focus(window, cx));
        }
        let Some(parts) = self.parts.as_ref() else {
            return base.into_any_element();
        };
        let description = parts.description.clone();
        let tray = self.tray(parts, cx);
        let slot = self.description_slot(parts);
        let files = self.files_section(parts, cx);

        let typed = self.title.read(cx).value().trim().to_string();
        let collapsed =
            crate::work_header::title_collapsed(&self.body_scroll, &self.title_bottom);
        let compact = collapsed.then(|| {
            crate::work_header::collapsed_title(
                Some(SharedString::from(copy::HEADER)),
                if typed.is_empty() {
                    SharedString::from(copy::UNTITLED)
                } else {
                    SharedString::from(typed)
                },
                Some(gpui::ElementId::from("draft-collapsed-title")),
                cx,
            )
        });
        let right = self.cluster(cx);
        let title_row = crate::work_header::title_input_row(&self.title, false, {
            let description = description.clone();
            move |window, cx| {
                description.update(cx, |description, cx| description.focus(window, cx));
                true
            }
        })
        // EXP-1191: a little air above the title (web `pt-4`) — with no
        // parent line or identifier above it, it hugged the pane's top.
        .mt(px(16.))
        .into_any_element();
        let pinned = crate::work_header::tray_pinned(&self.body_scroll, &self.tray_top);
        let (header, rows) = crate::work_header::scrolling_title_rows(
            crate::work_header::TitleChrome {
                body_scroll: &self.body_scroll,
                title_bottom: &self.title_bottom,
                tray_top: &self.tray_top,
                tray_h: &self.tray_h,
                cluster_w: &self.cluster_w,
                entity_id: cx.entity_id(),
            },
            collapsed,
            vec![title_row],
            tray,
            Vec::new(),
            compact,
            right,
            pinned,
        );
        let column = v_flex()
            .child(rows)
            // The detail's breathing room under the tray.
            .child(div().flex_shrink_0().h(px(20.)))
            .child(slot)
            .child(files);
        let center = div()
            .id("issue-draft-scroll")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .w_full()
            .overflow_y_scroll()
            .track_scroll(&self.body_scroll)
            .child(v_flex().w_full().pb_6().child(centered_column(column)));
        base.child(
            div()
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .w_full()
                .child(center)
                .child(crate::surface::edge_fade_bottom())
                .child(header),
        )
        .into_any_element()
    }
}
