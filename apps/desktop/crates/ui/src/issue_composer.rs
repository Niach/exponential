//! EXP-771 — the inline "New sub-issue" composer: a card under the parent's
//! description holding the borderless title input (Tab jumps to the
//! description, Enter submits), the §4.5 WYSIWYG editor in staging mode with
//! its attach handler and non-image file rail, the shared
//! [`crate::issue_draft::IssueDraft`] chip row, the error line, the capsule
//! submit and the create pipeline (`IssueDraft::spawn_create`: create with
//! the `draft://` images STRIPPED, upload them, rewrite the URLs, then wait
//! for the created row to become visible in the synced collection).
//!
//! A ghost ✕ in the card's top-right closes it, Escape closes it too, and a
//! successful create CLEARS and stays open (filing sub-issues comes in runs).
//!
//! EXP-1170: the create-issue DIALOG presentation is gone — "New issue" is a
//! page now ([`crate::issue_draft_screen`], backed by an issue DRAFT through
//! [`crate::draft_editor`]). A sub-issue card is never a draft: its images
//! and files stay staged until the create.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use domain::rows::Issue;
use sync::Store;

use crate::attachments_row;
use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::markdown::image_paste::strip_draft_images;
use crate::wysiwyg::WysiwygDescription;

/// What an [`IssueComposer`] tells its host.
pub(crate) enum IssueComposerEvent {
    /// Escape, or the card's ✕ — the host drops the composer.
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

pub(crate) struct IssueComposer {
    /// The board the issue is filed onto — the parent's, and only the
    /// FALLBACK: the parent's board is re-read at create time
    /// ([`Self::target_board_id`]), since the composer can outlive a board
    /// move made anywhere. A sub-issue on another board would be a move, not
    /// a create (web parity).
    board_id: String,
    /// The parent this files a child under. EXP-760 — the relation is
    /// inserted in the create's OWN transaction, so the row appears under
    /// "Sub-issues" with the issue itself rather than one Electric round trip
    /// later.
    parent_id: String,

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
    /// The INLINE sub-issue card under `parent`'s description.
    pub(crate) fn inline(
        parent: &Issue,
        team_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let title = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Sub-issue title")
        });
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
        // EXP-335: the rail's attach button — image picks embed inline via
        // the editor itself; non-image picks land here and queue for the
        // post-create upload (web draftFiles parity).
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
        // Re-render on every editor change so the submit gating tracks the
        // live description.
        subscriptions.push(cx.observe(&description, |_, _, cx| cx.notify()));

        Self {
            board_id: parent.board_id.clone(),
            parent_id: parent.id.clone(),
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

    /// The board the create actually targets: the PARENT's board as of now.
    /// The card is captured once when it opens, and a parent moved to
    /// another board in between (by a teammate, or from this very window)
    /// would otherwise file its child onto the old one. The captured value
    /// stays the fallback for a parent that is not synced.
    fn target_board_id(&self, cx: &App) -> String {
        Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(&self.parent_id)
            .map(|parent| parent.board_id.clone())
            .unwrap_or_else(|| self.board_id.clone())
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
                this.update(cx, |this, cx| {
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

        // The exact web mutation input: create with the staged `draft://`
        // images STRIPPED, upload them post-create, then update the
        // description with the canonical attachment URLs — all of it
        // `issue_draft::spawn_create`.
        let board_id = self.target_board_id(cx);
        let mut input = api::issues::IssuesCreateInput::new(board_id, title);
        self.draft.read(cx).apply_to_create(&mut input);
        input.parent_id = Some(self.parent_id.clone());
        let markdown = self.description.read(cx).markdown(cx);
        let stripped_description = strip_draft_images(&markdown);
        if !stripped_description.is_empty() {
            input.description = Some(stripped_description.clone());
        }
        let staged_images = self.description.read(cx).staged_images(cx);
        // EXP-335: queued non-image files ride the same post-create window
        // (cheap Arc clones — the bytes are shared, not copied).
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
                    // The child is already synced (the create gate waits for
                    // the row), so the host's relations block picks it up on
                    // its next repaint; the composer clears for the next one.
                    Ok(_) => {
                        this.submitting = false;
                        this.clear(window, cx);
                        cx.emit(IssueComposerEvent::Created);
                        cx.notify();
                    }
                    Err(message) => {
                        this.error = Some(message);
                        this.submitting = false;
                        cx.notify();
                    }
                });
            },
        );
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

    /// Dismiss the composer. The host tears the view down on
    /// [`IssueComposerEvent::Cancelled`] and puts focus back on itself there
    /// (EXP-781) — focus is the host's to place, since it owns the handle the
    /// J/K bindings are scoped to.
    fn cancel(&mut self, cx: &mut gpui::Context<Self>) {
        cx.emit(IssueComposerEvent::Cancelled);
    }

    // -- pieces ----------------------------------------------------------------

    /// Web `IssueEditorAttachmentRail` (EXP-586 shape): one chip per queued
    /// non-image file (web `issue-attachment-file-chip-*` parity). Images are
    /// NOT listed — they render inline in the description and are removed
    /// there.
    fn attachment_rail(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let removable = !self.submitting;
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
                        SharedString::from(format!("sub-attachment-file-remove-{}", file.key)),
                        on_click,
                    )
                });
                attachments_row::file_chip(
                    gpui::ElementId::from(("sub-attachment-file-chip", file.key as usize)),
                    file.filename.clone(),
                    Some(file.content_type.as_str()),
                    file.bytes.len() as i64,
                    remove,
                    cx,
                )
            }))
    }

    /// EXP-771: a real capsule primary button (`controls::WebControl`).
    fn submit_button(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let disabled = self.title.read(cx).value().trim().is_empty() || self.submitting;
        let label: &'static str = if self.submitting {
            "Creating..."
        } else {
            "Create"
        };

        Button::new("sub-issue-submit")
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
        // Row scale — the card is already the field's frame.
        let input = glass_input(&self.title, window, cx)
            .appearance(false)
            .text_sm();
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
            // binding would fight every editor on the page.
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
        self.render_inline(window, cx)
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

    /// The submit contract (moved here from `create_issue_dialog` with the
    /// pipeline, EXP-771): the create call
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
