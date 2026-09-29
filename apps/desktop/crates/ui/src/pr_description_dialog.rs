//! "Edit pull request" dialog (EXP-1139) — the web `PrDescriptionDialog`
//! 1:1: the PR's title (one line) over its body (a plain monospace textarea,
//! NOT the WYSIWYG — a PR body is authored GFM, and GitHub renders it), one
//! `issues.updatePr` on Save. Opened from the review screen's description
//! card ([`crate::pr_diff`]); the same procedure the MCP `exponential_pr_update`
//! tool uses, so a person and an agent rewrite the same PR the same way.
//!
//! Nothing is synced: the card re-fetches `issues.prDescription` through
//! `on_saved` once the dialog closes, GitHub stays the source of truth. A
//! server refusal (a merged PR, a severed GitHub connection) renders verbatim
//! above the footer and keeps the dialog open.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, size, App, AppContext as _, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _,
};

use crate::controls::{glass_input, WebControl as _};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

/// The `pr_open` limits (mirrored by the server's `prUpdateFields`), so a
/// save can never be refused for a size the open accepted.
pub(crate) const PR_TITLE_MAX_CHARS: usize = 255;

/// Open the dialog seeded with the PR's CURRENT title and body (what the card
/// just fetched). `on_saved` runs in the main window after a successful save
/// — the card re-fetches the description there.
pub(crate) fn open(
    window: &mut Window,
    cx: &mut App,
    issue_id: String,
    pr_number: i64,
    title: String,
    body: String,
    on_saved: Rc<dyn Fn(&mut Window, &mut App)>,
) {
    // Web sm:max-w-3xl; the body is the tall field.
    let height = (window.viewport_size().height * 0.85).min(px(560.));
    let spec = DialogSpec::new(format!("Edit pull request #{pr_number}"), size(px(680.), height))
        .resizable(size(px(520.), px(380.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            PrDescriptionDialogView::new(
                issue_id.clone(),
                title.clone(),
                body.clone(),
                on_saved.clone(),
                window,
                cx,
            )
        });
        let busy = view.clone();
        DialogContent::new(view)
            // The view pins its own footer; the body textarea scrolls.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).submitting)
    });
}

struct PrDescriptionDialogView {
    issue_id: String,
    initial_title: String,
    initial_body: String,
    title: gpui::Entity<InputState>,
    body: gpui::Entity<TextareaState>,
    submitting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    on_saved: Rc<dyn Fn(&mut Window, &mut App)>,
    _subscriptions: Vec<Subscription>,
}

impl PrDescriptionDialogView {
    fn new(
        issue_id: String,
        initial_title: String,
        initial_body: String,
        on_saved: Rc<dyn Fn(&mut Window, &mut App)>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let title = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Title")
                .validate(|text, _| text.chars().count() <= PR_TITLE_MAX_CHARS)
        });
        title.update(cx, |state, cx| {
            state.set_value(initial_title.clone(), window, cx);
        });
        let body = cx.new(|cx| TextareaState::new(window, cx).placeholder("Description (GFM)"));
        body.update(cx, |state, cx| {
            state.set_value(initial_body.clone(), window, cx);
        });

        let mut subscriptions = vec![cx.subscribe_in(
            &title,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            },
        )];
        // A textarea: Enter inserts a newline; Save-gating follows it live.
        subscriptions.push(cx.subscribe_in(
            &body,
            window,
            |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        ));

        Self {
            issue_id,
            initial_title,
            initial_body,
            title,
            body,
            submitting: false,
            error: None,
            focused_once: false,
            on_saved,
            _subscriptions: subscriptions,
        }
    }

    /// The trimmed title and the body as typed; `None` for a field that did
    /// not change (the server keeps it — an omitted field is never sent as
    /// an empty string).
    fn changes(&self, cx: &App) -> (Option<String>, Option<String>) {
        let title = self
            .title
            .read(cx)
            .value()
            .replace(['\r', '\n'], " ")
            .trim()
            .to_string();
        let body = self.body.read(cx).value().to_string();
        (
            (title != self.initial_title).then_some(title),
            (body != self.initial_body).then_some(body),
        )
    }

    fn can_submit(&self, cx: &App) -> bool {
        let title_empty = self.title.read(cx).value().trim().is_empty();
        let (title, body) = self.changes(cx);
        !title_empty && (title.is_some() || body.is_some()) && !self.submitting
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.can_submit(cx) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };
        let (title, body) = self.changes(cx);
        let issue_id = self.issue_id.clone();
        self.submitting = true;
        self.error = None;
        cx.notify();

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    api::issues::update_pr(&trpc, &issue_id, title.as_deref(), body.as_deref())
                        .map(|_| ())
                })
                .await;
            let _ = this.update_in(window, |view, window, cx| match result {
                Ok(()) => {
                    let on_saved = view.on_saved.clone();
                    native_dialog::close_then(window, cx, move |window, cx| on_saved(window, cx));
                }
                Err(err) => {
                    view.submitting = false;
                    view.error = Some(err.user_message().into());
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for PrDescriptionDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.title.update(cx, |state, cx| state.focus(window, cx));
        }
        let disabled = !self.can_submit(cx);
        let danger = cx.theme().danger;

        // EXP-694 grouped fields: the title row over the body, the group's
        // fill and hairlines the only chrome (web `GlassGroup` parity).
        let title_row = crate::surface::glass_row_shell().child(
            div().flex_1().min_w_0().child(
                glass_input(&self.title, window, cx)
                    .appearance(false)
                    .h_auto()
                    .px_0()
                    .py_0(),
            ),
        );
        let form = crate::surface::glass_group()
            .flex_1()
            .min_h_0()
            .child(crate::surface::glass_row_divider(title_row))
            .child(
                Textarea::new(&self.body)
                    .appearance(false)
                    .h_full()
                    .px_4()
                    .py_3()
                    .font_family(theme::terminal::FONT_FAMILY),
            );

        let footer = h_flex()
            .flex_shrink_0()
            .justify_end()
            .gap_2()
            .child(
                Button::new("pr-description-cancel")
                    .ghost()
                    .cursor_pointer()
                    .web_sm()
                    .label("Cancel")
                    .disabled(self.submitting)
                    .on_click(|_, window, cx| native_dialog::close_dialog_window(window, cx)),
            )
            .child(
                Button::new("pr-description-save")
                    .primary()
                    .cursor_pointer()
                    .web_sm()
                    .label(if self.submitting {
                        "Saving…"
                    } else {
                        "Save changes"
                    })
                    .disabled(disabled)
                    .loading(self.submitting)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            );

        v_flex()
            .size_full()
            .gap_3()
            .child(div().flex_1().min_h_0().flex().flex_col().child(form))
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(danger).child(error))
            })
            .child(footer.pt_3().border_t_1().border_color(cx.theme().border))
    }
}
