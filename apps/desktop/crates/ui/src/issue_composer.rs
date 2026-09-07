//! EXP-771 — the ONE issue composer, in two presentations.
//!
//! "New issue" (a native window, [`crate::create_issue_dialog`]) and "New
//! sub-issue" (an inline card under the parent's description) used to be two
//! views over one flow, and they drifted: only the dialog had the attach
//! button and the staged-file rail, only the card had a Cancel affordance,
//! and the submit button was a hand-rolled rounded square in both.
//!
//! Everything that is the COMPOSER lives here — the borderless title input
//! (Tab jumps to the description, Enter submits), the §4.5 WYSIWYG editor in
//! staging mode with its attach handler and non-image file rail, the shared
//! [`crate::issue_draft::IssueDraft`] chip row, the error line, the capsule
//! submit and the create pipeline (`IssueDraft::spawn_create`: create with
//! the `draft://` images STRIPPED, upload them, rewrite the URLs, then wait
//! for the created row to become visible in the synced collection).
//!
//! Only the FRAME differs:
//!
//! * [`Presentation::Dialog`] fills the native window — submit reads "Create
//!   issue", the board select rides the window's titlebar strip
//!   (`create_issue_dialog::open`), Escape is the dialog shell's own
//!   `CancelNativeDialog` action, and the window grows with the description
//!   up to its cap (EXP-288).
//! * [`Presentation::Inline`] is a `glass_card` with a fixed parent + board —
//!   submit reads "Create", a ghost ✕ in the card's top-right closes it, and
//!   Escape closes it too. A successful create CLEARS and stays open (filing
//!   sub-issues comes in runs).

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Entity, EventEmitter, FocusHandle,
    Focusable, FontWeight, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use domain::rows::Issue;

use crate::attachments_row;
use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::markdown::image_paste::strip_draft_images;
use crate::wysiwyg::WysiwygDescription;

/// What an [`IssueComposer`] tells its host.
pub(crate) enum IssueComposerEvent {
    /// Escape, or the card's ✕ — the host drops the composer. Never emitted
    /// in [`Presentation::Dialog`]: there the window itself is the composer.
    Cancelled,
    /// A child issue exists and has synced — the relations block above the
    /// composer already carries it, so the host only has to repaint. The
    /// composer stays open, cleared and re-focused.
    Created,
}

/// EXP-335: one NON-image file queued via the toolbar's attach button — web
/// `draftFiles` parity. Bytes are read (and size-capped) at pick time, then
/// uploaded to the created issue right after `issues.create`.
struct StagedDraftFile {
    /// Process-local chip key (element ids + removal).
    key: u64,
    filename: String,
    content_type: String,
    bytes: std::sync::Arc<Vec<u8>>,
}

/// EXP-288 window-grow bookkeeping — dialog presentation only.
struct DialogGrow {
    /// The description scroll container's tracked handle — handed to the
    /// vendored editor for caret-follow, and read in render as the
    /// content-overflow sensor for the grow-while-typing window resize.
    desc_scroll: gpui::ScrollHandle,
    /// The height cap the dialog grows toward (the pre-EXP-288 fixed open
    /// height, computed from the OPENER's viewport at open time).
    max_height: gpui::Pixels,
    /// Grow-only bookkeeping: the last height this dialog requested via
    /// `window.resize`. An observed height meaningfully BELOW it means the
    /// user resized by hand — auto-grow then stops fighting them.
    last_requested_height: Option<gpui::Pixels>,
    user_resized: bool,
}

enum Presentation {
    Dialog(DialogGrow),
    Inline,
}

pub(crate) struct IssueComposer {
    presentation: Presentation,
    /// The board the issue is filed onto. The dialog's titlebar select writes
    /// it live ([`IssueComposer::set_board_id`]); inline it is the parent's
    /// and never moves — a sub-issue on another board would be a move, not a
    /// create (web parity).
    board_id: String,
    /// Inline only: the parent this files a child under. EXP-760 — the
    /// relation is inserted in the create's OWN transaction, so the row
    /// appears under "Sub-issues" with the issue itself rather than one
    /// Electric round trip later.
    parent_id: Option<String>,

    title: Entity<InputState>,
    /// The §4.5 block editor in STAGING mode: pasted images stay `draft://`
    /// blocks until submit resolves them.
    description: Entity<WysiwygDescription>,
    /// EXP-760: the status/priority/assignee/labels/due state and its chips.
    draft: Entity<crate::issue_draft::IssueDraft>,
    /// EXP-335: non-image files queued for the post-create upload.
    staged_files: Vec<StagedDraftFile>,
    next_staged_file_key: u64,
    submitting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<IssueComposerEvent> for IssueComposer {}

impl IssueComposer {
    /// The create-issue DIALOG's body (a native window fills with this).
    pub(crate) fn dialog(
        board_id: String,
        team_id: String,
        max_height: gpui::Pixels,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let desc_scroll = gpui::ScrollHandle::new();
        let this = Self::new(
            Presentation::Dialog(DialogGrow {
                desc_scroll: desc_scroll.clone(),
                max_height,
                last_requested_height: None,
                user_resized: false,
            }),
            board_id,
            None,
            team_id,
            "Issue title",
            window,
            cx,
        );
        // EXP-288: hand the scroll container's handle to the editor so the
        // caret stays visible while typing/pasting ("we always wanna see
        // what we type").
        this.description.update(cx, |description, cx| {
            description.set_scroll_handle(desc_scroll, cx);
        });
        this
    }

    /// The INLINE sub-issue card under `parent`'s description.
    pub(crate) fn inline(
        parent: &Issue,
        team_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        Self::new(
            Presentation::Inline,
            parent.board_id.clone(),
            Some(parent.id.clone()),
            team_id,
            "Sub-issue title",
            window,
            cx,
        )
    }

    fn new(
        presentation: Presentation,
        board_id: String,
        parent_id: Option<String>,
        team_id: String,
        title_placeholder: &'static str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder(title_placeholder));
        // Shared configured-editor constructor (§4.5): completion + pills
        // scoped to this team, upload staged (`upload_issue = None`).
        let description = crate::description_editor::build_wysiwyg_editor(
            Some(team_id.clone()),
            None,
            "Add description...",
            "",
            None,
            window,
            cx,
        );
        // EXP-335/EXP-771: the rail's attach button, in BOTH presentations —
        // image picks embed inline via the editor itself; non-image picks
        // land here and queue for the post-create upload (web draftFiles
        // parity).
        {
            let composer = cx.entity().downgrade();
            description.update(cx, |description, _| {
                description.set_attach_handler(Rc::new(move |paths, window, cx| {
                    let Some(composer) = composer.upgrade() else {
                        return;
                    };
                    composer.update(cx, |this, cx| this.stage_files(paths, window, cx));
                }));
            });
        }
        let draft = cx.new(|cx| crate::issue_draft::IssueDraft::new(team_id, window, cx));

        let mut subscriptions = Vec::new();
        // The chips live on the draft entity — repaint when a pick lands.
        subscriptions.push(cx.observe(&draft, |_, _, cx| cx.notify()));
        subscriptions.push(cx.subscribe_in(
            &title,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                // Enter in the (single-line) title submits, like the web form.
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                // Title emptiness drives the submit button's disabled state.
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        ));
        // Re-render on every editor change so the submit gating and the
        // grow-with-content sensor track the live description.
        subscriptions.push(cx.observe(&description, |_, _, cx| cx.notify()));

        Self {
            presentation,
            board_id,
            parent_id,
            title,
            description,
            draft,
            staged_files: Vec::new(),
            next_staged_file_key: 0,
            submitting: false,
            error: None,
            focused_once: false,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    fn is_dialog(&self) -> bool {
        matches!(self.presentation, Presentation::Dialog(_))
    }

    /// Element-id namespace — two composers can be on screen at once.
    fn id_prefix(&self) -> &'static str {
        if self.is_dialog() {
            "create"
        } else {
            "sub"
        }
    }

    /// The staged-file chip's id root (a `&'static str`, which is what
    /// `ElementId` takes alongside the per-file index).
    fn file_chip_id(&self) -> &'static str {
        if self.is_dialog() {
            "create-attachment-file-chip"
        } else {
            "sub-attachment-file-chip"
        }
    }

    /// The board this composer files onto (the dialog's titlebar select).
    pub(crate) fn board_id(&self) -> &str {
        &self.board_id
    }

    /// EXP-449: the dialog's titlebar board select writes the target live.
    /// Nothing else resets on a pick — status/assignee/label options are
    /// team-scoped and the menu only offers same-team boards.
    pub(crate) fn set_board_id(&mut self, board_id: String, cx: &mut gpui::Context<Self>) {
        self.board_id = board_id;
        cx.notify();
    }

    /// The dialog shell's `can_close` gate — a submit in flight refuses ✕.
    pub(crate) fn submitting(&self) -> bool {
        self.submitting
    }

    // -- window growth (dialog only) -------------------------------------------

    /// EXP-288: grow the dialog window with the description content, up to
    /// the cap — the dialog opens compact and expands while typing/pasting
    /// instead of starting tall. Grow-only (never shrinks on deletion), and a
    /// manual shrink by the user latches auto-grow off. The overflow sensor is
    /// the description scroll container's last-frame layout, so this runs at
    /// render time with a one-frame lag.
    fn grow_with_content(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Presentation::Dialog(grow) = &mut self.presentation else {
            return;
        };
        if grow.user_resized {
            return;
        }
        let current = window.viewport_size();
        if let Some(requested) = grow.last_requested_height {
            if current.height < requested - px(2.) {
                // The user dragged the window smaller than we grew it —
                // stop fighting them for this dialog's lifetime.
                grow.user_resized = true;
                return;
            }
        }
        let overflow = grow.desc_scroll.max_offset().y;
        if overflow <= px(1.) || current.height >= grow.max_height - px(1.) {
            return;
        }
        let target = (current.height + overflow).min(grow.max_height);
        if target <= current.height + px(1.) {
            return;
        }
        grow.last_requested_height = Some(target);
        let new_size = gpui::size(current.width, target);
        // Deferred: resizing mid-render would re-enter the platform path
        // (the window_size.rs EXP-263 precedent).
        window.defer(cx, move |window, _cx| window.resize(new_size));
    }

    // -- staged files ----------------------------------------------------------

    /// EXP-335: read picked non-image files off the foreground (read_any_file
    /// enforces the 50 MB cap) and queue them for the post-create upload; an
    /// unreadable/oversize pick surfaces in the error slot.
    fn stage_files(
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
                this.update_in(cx, |this, _window, cx| {
                    match result {
                        Ok((filename, content_type, bytes)) => {
                            let key = this.next_staged_file_key;
                            this.next_staged_file_key += 1;
                            this.staged_files.push(StagedDraftFile {
                                key,
                                filename,
                                content_type,
                                bytes: std::sync::Arc::new(bytes),
                            });
                        }
                        Err(error) => {
                            this.error = Some(format!("{error}").into());
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    // -- submit ----------------------------------------------------------------

    pub(crate) fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let title = self.title.read(cx).value().trim().to_string();
        if title.is_empty() || self.submitting {
            return;
        }

        self.error = None;
        self.submitting = true;
        cx.notify();

        // The exact web mutation input (`create-issue-dialog.tsx`
        // handleSubmit): create with the staged `draft://` images STRIPPED,
        // upload them post-create, then update the description with the
        // canonical attachment URLs — all of it `issue_draft::spawn_create`.
        let mut input = api::issues::IssuesCreateInput::new(self.board_id.clone(), title);
        self.draft.read(cx).apply_to_create(&mut input);
        input.parent_id = self.parent_id.clone();
        let markdown = self.description.read(cx).markdown(cx);
        let staged_images = self.description.read(cx).staged_images(cx);
        let stripped_description = strip_draft_images(&markdown);
        if !stripped_description.is_empty() {
            input.description = Some(stripped_description.clone());
        }
        // EXP-335: queued non-image draft files ride the same post-create
        // window (cheap Arc clones — the bytes are shared, not copied).
        let staged_files: Vec<(String, String, std::sync::Arc<Vec<u8>>)> = self
            .staged_files
            .iter()
            .map(|file| {
                (
                    file.filename.clone(),
                    file.content_type.clone(),
                    file.bytes.clone(),
                )
            })
            .collect();

        let view = cx.entity().downgrade();
        crate::issue_draft::spawn_create(
            crate::issue_draft::CreateJob {
                input,
                markdown,
                stripped_description,
                staged_images,
                staged_files,
            },
            window,
            cx,
            move |result, window, cx| {
                let Some(view) = view.upgrade() else {
                    return;
                };
                view.update(cx, |this, cx| match result {
                    Ok(issue_id) => this.on_created(issue_id, window, cx),
                    Err(message) => {
                        this.error = Some(message);
                        this.submitting = false;
                        cx.notify();
                    }
                });
            },
        );
    }

    fn on_created(
        &mut self,
        issue_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match self.presentation {
            // EXP-288/EXP-510: the rail may point anywhere while the dialog
            // is up — land fully scoped on the ISSUE's board (rail tool +
            // active board + tab origin).
            Presentation::Dialog(_) => {
                let board_id = self.board_id.clone();
                crate::native_dialog::close_then(window, cx, move |window, cx| {
                    crate::navigation::open_issue_scoped(window, cx, issue_id, board_id);
                });
            }
            // The child is already synced (the create gate waits for the
            // row), so the host's relations block picks it up on its next
            // repaint; the composer clears for the next one.
            Presentation::Inline => {
                self.submitting = false;
                self.clear(window, cx);
                cx.emit(IssueComposerEvent::Created);
                cx.notify();
            }
        }
    }

    /// Empty the form for the next child, keeping the composer open and
    /// focused. The property picks reset too — a run of sub-issues does not
    /// silently inherit the last one's labels.
    fn clear(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.title.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        self.description
            .update(cx, |editor, cx| editor.set_markdown("", window, cx));
        self.draft.update(cx, |draft, cx| draft.reset(cx));
        self.staged_files.clear();
        self.error = None;
    }

    fn cancel(&mut self, cx: &mut gpui::Context<Self>) {
        cx.emit(IssueComposerEvent::Cancelled);
    }

    // -- shared pieces ---------------------------------------------------------

    /// Web `IssueEditorAttachmentRail` (EXP-586 shape): one chip per queued
    /// non-image draft file (web `issue-attachment-file-chip-*` parity).
    /// Images are NOT listed — they render inline in the description and are
    /// removed there.
    fn attachment_rail(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let removable = !self.submitting;
        let prefix = self.id_prefix();
        let chip_id = self.file_chip_id();

        h_flex()
            .min_w_0()
            .flex_1()
            .gap_1p5()
            .items_center()
            .overflow_hidden()
            .children(self.staged_files.iter().map(|file| {
                let remove: Option<attachments_row::ChipRemove> = removable.then(|| {
                    let view = cx.entity().clone();
                    let key = file.key;
                    let on_click = Box::new(
                        move |_: &gpui::ClickEvent, _window: &mut Window, cx: &mut App| {
                            view.update(cx, |this, cx| {
                                this.staged_files.retain(|staged| staged.key != key);
                                cx.notify();
                            });
                        },
                    )
                        as Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>;
                    (
                        SharedString::from(format!("{prefix}-attachment-file-remove-{}", file.key)),
                        on_click,
                    )
                });
                attachments_row::file_chip(
                    gpui::ElementId::from((chip_id, file.key as usize)),
                    file.filename.clone(),
                    Some(file.content_type.as_str()),
                    file.bytes.len() as i64,
                    remove,
                    cx,
                )
            }))
    }

    /// EXP-771: a real capsule primary button (`controls::WebControl`), not
    /// the hand-rolled rounded square the two composers used to share.
    fn submit_button(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let disabled = self.title.read(cx).value().trim().is_empty() || self.submitting;
        let label: &'static str = match (self.submitting, self.is_dialog()) {
            (true, _) => "Creating...",
            (false, true) => "Create issue",
            (false, false) => "Create",
        };

        Button::new(SharedString::from(format!("{}-issue-submit", self.id_prefix())))
            .primary()
            .web_sm()
            .label(label)
            .disabled(disabled)
            .when(!disabled, |button| {
                button.on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)))
            })
    }

    /// EXP-586: the error line, else the queued non-image files; with neither
    /// there is nothing to render at all.
    fn status_line(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        match &self.error {
            Some(error) => Some(
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error.clone())
                    .into_any_element(),
            ),
            None if !self.staged_files.is_empty() => {
                Some(self.attachment_rail(cx).into_any_element())
            }
            None => None,
        }
    }

    /// The borderless title field. TAB jumps straight into the description
    /// editor (EXP-68) — without this the single-line input propagates the
    /// `tab → IndentInline` binding and Root's fallback `tab → Tab` cycles
    /// focus through the description toolbar's formatting buttons first.
    /// Handling IndentInline here (the bubble reaches this wrapper right
    /// after the input propagates it) consumes the keystroke before Root's
    /// binding runs.
    fn title_field(&self, window: &mut Window, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let input = glass_input(&self.title, window, cx).appearance(false);
        let input = if self.is_dialog() {
            // Web text-lg font-medium.
            input.text_lg().font_weight(FontWeight::MEDIUM)
        } else {
            // Row scale — the card is already the field's frame.
            input.text_sm()
        };
        div()
            .flex_1()
            .min_w_0()
            .on_action(cx.listener(
                |this, _: &gpui_component::input::IndentInline, window, cx| {
                    this.description
                        .update(cx, |editor, cx| editor.focus(window, cx));
                },
            ))
            .child(input)
    }

    // -- presentations ---------------------------------------------------------

    fn render_dialog(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        // EXP-288: expand the window with the description content (up to the
        // cap) before the caret-follow scrolling takes over.
        self.grow_with_content(window, cx);
        let desc_scroll = match &self.presentation {
            Presentation::Dialog(grow) => grow.desc_scroll.clone(),
            Presentation::Inline => gpui::ScrollHandle::new(),
        };

        // Chip row (web px-4 py-2 border-t): status · priority · assignee ·
        // labels · due. EXP-760: the picks and their chips are the shared
        // `IssueDraft` — this row is only the dialog's placement of them.
        let chips = self.draft.update(cx, |draft, cx| draft.chips("create", cx));

        v_flex()
            .size_full()
            .child(
                // EXP-287: the deleted header row's `pb_2` used to supply
                // this gap under the chrome.
                h_flex()
                    .px_3()
                    .pt_2()
                    .child(self.title_field(window, cx)),
            )
            .child(
                // Only this region scrolls; header/chips/footer pinned.
                // The 96px floor is ~3 text rows (EXP-288 — the compact
                // dialog opens with a real textarea, not a single line).
                // `track_scroll` powers the editor's caret-follow AND the
                // grow-while-typing sensor; the `min_h_full` inner column
                // makes the editor view's trailing click-filler stretch to
                // the container bottom even inside the scroll container
                // (children of a scroll area lay out against content height)
                // — clicking anywhere below the last line places the caret at
                // the end (textarea behavior).
                div()
                    .id("create-issue-description")
                    .flex_1()
                    .min_h(px(96.))
                    .px_3()
                    .overflow_y_scroll()
                    .track_scroll(&desc_scroll)
                    .child(
                        // EXP-421: the filler reads as text, so it carries the
                        // text cursor — the caret lands here on click.
                        div()
                            .min_h_full()
                            .flex()
                            .flex_col()
                            .cursor(gpui::CursorStyle::IBeam)
                            .child(self.description.clone()),
                    ),
            )
            .child(
                h_flex()
                    .px_4()
                    .py_2()
                    .gap_1()
                    .items_center()
                    .flex_wrap()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .children(chips)
                    // EXP-586: submit rides the chip row, right-aligned.
                    .child(
                        div()
                            .ml_auto()
                            .flex_shrink_0()
                            .pl_2()
                            .child(self.submit_button(cx)),
                    ),
            )
            .children(self.status_line(cx).map(|content| {
                h_flex()
                    .px_4()
                    .py_3()
                    .items_center()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(div().min_w_0().flex_1().child(content))
            }))
            .into_any_element()
    }

    fn render_inline(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let chips = self.draft.update(cx, |draft, cx| draft.chips("sub", cx));

        crate::surface::glass_card()
            .id("sub-issue-composer")
            .track_focus(&self.focus_handle)
            .w_full()
            .gap_2()
            .px_3()
            .py_2p5()
            // Escape closes without filing anything, from the card itself,
            // the title input and the chips — every focus target inside here
            // that does not BIND escape. It deliberately does not reach in
            // from the description editor: `gpui-markdown-editor` binds
            // escape to `DismissTransientUi` in its own key context, gpui
            // dispatches a matched binding BEFORE any key-down listener and a
            // bubble-phase action handler stops propagation by default, so
            // the keystroke is consumed there — which is the behaviour we
            // want (escape first walks the editor's popups and format rail
            // back). The ✕ beside the title is the dismissal that works from
            // anywhere, including mid-paragraph.
            //
            // A raw key handler rather than an action: the composer is inline
            // chrome with no key context of its own, and an app-wide `escape`
            // binding would fight every editor on the page. (The dialog
            // presentation has no handler here — its window answers
            // `CancelNativeDialog`.)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.cancel(cx);
                }
            }))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(self.title_field(window, cx))
                    // EXP-771: a ghost ✕ in the card's top-right instead of a
                    // Cancel button beside Create — the dismissal every other
                    // card-shaped surface wears.
                    .child(
                        Button::new("sub-issue-close")
                            .ghost()
                            .web_icon_xs()
                            .icon(
                                Icon::new(registry::UI_CLOSE)
                                    .small()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .tooltip("Close")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.cancel(cx);
                            })),
                    ),
            )
            .child(div().w_full().min_h(px(48.)).child(self.description.clone()))
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .items_center()
                    .gap_1p5()
                    .children(chips)
                    .child(
                        div()
                            .ml_auto()
                            .flex_shrink_0()
                            .pl_2()
                            .child(self.submit_button(cx)),
                    ),
            )
            .children(self.status_line(cx))
            .into_any_element()
    }
}

impl Focusable for IssueComposer {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for IssueComposer {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Web `autoFocus` on the title input (once, after the composer mounts).
        if !self.focused_once {
            self.focused_once = true;
            self.title.update(cx, |state, cx| state.focus(window, cx));
        }
        if self.is_dialog() {
            self.render_dialog(window, cx)
        } else {
            self.render_inline(window, cx)
        }
    }
}

/// The closed affordance: the ghost "Add sub-issues" button the issue detail
/// shows until the inline composer is open.
pub(crate) fn add_sub_issues_button(cx: &mut gpui::App) -> Button {
    let _ = cx;
    Button::new("add-sub-issues")
        .ghost()
        .cursor_pointer()
        .xsmall()
        .icon(Icon::new(registry::UI_ADD))
        .label("Add sub-issues")
}


#[cfg(test)]
mod tests {
    use super::*;

    /// The submit contract both presentations share (moved here from
    /// `create_issue_dialog` with the pipeline, EXP-771): the create call
    /// carries the description with every STAGED image removed — a
    /// `draft://` URL must never reach the server — and nothing else.
    #[test]
    fn submit_strips_only_staged_images_from_the_description() {
        let markdown =
            "Intro text\n\n![shot](draft://abc-1)\n\n![kept](/api/attachments/xyz)\n\nOutro";
        assert_eq!(
            strip_draft_images(markdown),
            "Intro text\n\n![kept](/api/attachments/xyz)\n\nOutro"
        );
        assert_eq!(strip_draft_images("![only](draft://a)"), "");
        assert_eq!(strip_draft_images(""), "");
    }
}
