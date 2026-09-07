//! EXP-760 — the inline SUB-ISSUE composer (web parity target:
//! `apps/web/src/components/sub-issue-composer.tsx`).
//!
//! Linear's affordance under the description: an "Add sub-issues" ghost
//! button that opens a small card — title, description, the shared property
//! chips — and files the child with `parentId` in the SAME server
//! transaction, so it lands under the "Sub-issues" heading directly above it
//! without a second `relations.create` round trip.
//!
//! It is deliberately a form and nothing else: the state behind the chips and
//! the create → upload → row-visible sequence are [`crate::issue_draft`]'s,
//! shared with the full create dialog.

use gpui::{
    div, prelude::FluentBuilder as _, px, AppContext as _, ClickEvent, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    ActiveTheme as _, Sizable as _,
};

use domain::rows::Issue;

use crate::controls::glass_input;
use crate::create_issue_dialog::primary_button;
use crate::markdown::image_paste::strip_draft_images;
use crate::wysiwyg::WysiwygDescription;

/// What the composer tells its host.
pub(crate) enum SubIssueComposerEvent {
    /// Escape, or the Cancel button — the host drops the composer.
    Cancelled,
    /// A child issue exists and has synced — the relations block above the
    /// composer already carries it, so the host only has to repaint. The
    /// composer stays open (filing sub-issues comes in runs), cleared and
    /// re-focused.
    Created,
}

pub(crate) struct SubIssueComposer {
    parent_id: String,
    board_id: String,
    title: Entity<InputState>,
    description: Entity<WysiwygDescription>,
    draft: Entity<crate::issue_draft::IssueDraft>,
    submitting: bool,
    error: Option<SharedString>,
    focus_handle: FocusHandle,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SubIssueComposerEvent> for SubIssueComposer {}

impl SubIssueComposer {
    pub(crate) fn new(
        parent: &Issue,
        team_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Sub-issue title"));
        // The §4.5 block editor in STAGING mode (`upload_issue = None`):
        // pasted images stay `draft://` blocks until the child exists.
        let description = crate::description_editor::build_wysiwyg_editor(
            Some(team_id.clone()),
            None,
            "Add description…",
            "",
            None,
            window,
            cx,
        );
        let draft = cx.new(|cx| crate::issue_draft::IssueDraft::new(team_id, window, cx));

        let mut subscriptions = Vec::new();
        // Enter in the single-line title submits, like the create dialog.
        subscriptions.push(cx.subscribe_in(
            &title,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                // Title emptiness drives the Create button's disabled state.
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        ));
        subscriptions.push(cx.observe(&draft, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&description, |_, _, cx| cx.notify()));

        Self {
            parent_id: parent.id.clone(),
            board_id: parent.board_id.clone(),
            title,
            description,
            draft,
            submitting: false,
            error: None,
            focus_handle: cx.focus_handle(),
            focused_once: false,
            _subscriptions: subscriptions,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let title = self.title.read(cx).value().trim().to_string();
        if title.is_empty() || self.submitting {
            return;
        }
        self.error = None;
        self.submitting = true;
        cx.notify();

        // The child lands on the PARENT's board — a sub-issue on another
        // board would be a move, not a create (web parity).
        let mut input = api::issues::IssuesCreateInput::new(self.board_id.clone(), title);
        self.draft.read(cx).apply_to_create(&mut input);
        // EXP-760: the whole point — the `parent` relation is inserted in the
        // create's own transaction, so the row appears under "Sub-issues"
        // with the issue itself rather than one Electric round trip later.
        input.parent_id = Some(self.parent_id.clone());

        let markdown = self.description.read(cx).markdown(cx);
        let staged_images = self.description.read(cx).staged_images(cx);
        let stripped_description = strip_draft_images(&markdown);
        if !stripped_description.is_empty() {
            input.description = Some(stripped_description.clone());
        }

        let view = cx.entity().downgrade();
        crate::issue_draft::spawn_create(
            crate::issue_draft::CreateJob {
                input,
                markdown,
                stripped_description,
                staged_images,
                // The composer has no attach button — non-image files go on
                // the child's own detail view.
                staged_files: Vec::new(),
            },
            window,
            cx,
            move |result, window, cx| {
                let Some(view) = view.upgrade() else {
                    return;
                };
                view.update(cx, |this, cx| {
                    this.submitting = false;
                    match result {
                        Ok(_issue_id) => {
                            this.clear(window, cx);
                            cx.emit(SubIssueComposerEvent::Created);
                        }
                        Err(message) => this.error = Some(message),
                    }
                    cx.notify();
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
    }

    fn cancel(&mut self, cx: &mut gpui::Context<Self>) {
        cx.emit(SubIssueComposerEvent::Cancelled);
    }
}

impl Focusable for SubIssueComposer {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SubIssueComposer {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Web `autoFocus`: the title takes the caret as soon as the card is up.
        if !self.focused_once {
            self.focused_once = true;
            self.title.update(cx, |state, cx| state.focus(window, cx));
        }
        let title_empty = self.title.read(cx).value().trim().is_empty();
        let disabled = title_empty || self.submitting;
        let chips = self.draft.update(cx, |draft, cx| draft.chips("sub", cx));

        crate::surface::glass_card()
            .id("sub-issue-composer")
            .track_focus(&self.focus_handle)
            .w_full()
            .gap_2()
            .px_3()
            .py_2p5()
            // Escape closes without filing anything (the dialog's contract).
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
                h_flex().w_full().items_center().gap_2().child(
                    // Borderless title, the create dialog's recipe at row
                    // scale — the card is already the field's frame.
                    div().flex_1().min_w_0().child(
                        glass_input(&self.title, window, cx)
                            .appearance(false)
                            .text_sm(),
                    ),
                ),
            )
            .child(
                div()
                    .w_full()
                    .min_h(px(48.))
                    .child(self.description.clone()),
            )
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .items_center()
                    .gap_1p5()
                    .children(chips)
                    .child(
                        h_flex()
                            .ml_auto()
                            .flex_shrink_0()
                            .items_center()
                            .gap_1p5()
                            .child(
                                Button::new("sub-issue-cancel")
                                    .ghost()
                                    .cursor_pointer()
                                    .xsmall()
                                    .label("Cancel")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.cancel(cx);
                                    })),
                            )
                            .child(
                                primary_button("sub-issue-create", disabled, cx)
                                    .child(SharedString::from(if self.submitting {
                                        "Creating..."
                                    } else {
                                        "Create"
                                    }))
                                    .when(!disabled, |button| {
                                        button.on_click(cx.listener(|this, _, window, cx| {
                                            this.submit(window, cx)
                                        }))
                                    }),
                            ),
                    ),
            )
            .children(self.error.clone().map(|error| {
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error)
            }))
    }
}

/// The closed affordance: the ghost "Add sub-issues" button the issue detail
/// shows until the composer is open.
pub(crate) fn add_sub_issues_button(cx: &mut gpui::App) -> Button {
    let _ = cx;
    Button::new("add-sub-issues")
        .ghost()
        .cursor_pointer()
        .xsmall()
        .icon(gpui_component::Icon::new(crate::icons::registry::UI_ADD))
        .label("Add sub-issues")
}
