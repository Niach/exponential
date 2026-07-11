//! Create-release dialog (EXP-56 — the Releases tool window's "+"): a name
//! `Input` plus an optional multi-line description, mirroring the
//! create-workspace dialog's shape. Submit → `releases.create`; the close is
//! gated on the new release appearing in the synced collection (§4.1) so the
//! Releases list shows it the moment the dialog closes. Target date and the
//! PR fields stay server/web-managed — the desktop v1 dialog is deliberately
//! name + description only.

use gpui::{
    div, px, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
};
use sync::Store;

use crate::queries;

/// Open the dialog for `workspace_id` (the active workspace — the Releases
/// tool window resolves it before calling).
pub fn open(window: &mut Window, cx: &mut App, workspace_id: String) {
    let view = cx.new(|cx| CreateReleaseDialogView::new(workspace_id, window, cx));
    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).submitting;
        dialog
            .w(px(416.))
            .title("New release")
            .overlay_closable(!busy)
            .keyboard(!busy)
            .on_ok({
                let view = view.clone();
                move |_, window, cx| {
                    view.update(cx, |view, cx| view.submit(window, cx));
                    false
                }
            })
            .child(view.clone())
    });
}

pub struct CreateReleaseDialogView {
    workspace_id: String,
    name: Entity<InputState>,
    description: Entity<InputState>,
    submitting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl CreateReleaseDialogView {
    fn new(workspace_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. v1.0"));
        let description = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(2, 6)
                .placeholder("Optional description…")
        });

        let subscriptions = vec![cx.subscribe_in(
            &name,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )];

        Self {
            workspace_id,
            name,
            description,
            submitting: false,
            error: None,
            focused_once: false,
            _subscriptions: subscriptions,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() || self.submitting {
            return;
        }
        let description = self.description.read(cx).value().trim().to_string();
        let description = (!description.is_empty()).then_some(description);
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };

        self.error = None;
        self.submitting = true;
        cx.notify();

        let workspace_id = self.workspace_id.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    api::releases::create(&trpc, &workspace_id, &name, description.as_deref())
                })
                .await;

            match result {
                Ok(output) => {
                    // Gate on the Electric echo so the Releases list shows the
                    // new row the moment the dialog closes (§4.1).
                    let release_id = output.release.id.clone();
                    let releases = window
                        .update(|_, cx| Store::global(cx).collections().releases.clone())
                        .ok();
                    if let Some(releases) = releases {
                        queries::await_row_visible(&releases, &release_id, window).await;
                    }
                    let _ = this.update_in(window, |_, window, cx| {
                        window.close_dialog(cx);
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _window, cx| {
                        // Keep the dialog open and show the message verbatim
                        // (web parity).
                        this.error = Some(format!("{err}").into());
                        this.submitting = false;
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
}

impl Render for CreateReleaseDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.name.update(cx, |state, cx| state.focus(window, cx));
        }

        let disabled = self.name.read(cx).value().trim().is_empty() || self.submitting;
        let closable = !self.submitting;

        let mut form = v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Name"),
                    )
                    .child(Input::new(&self.name).small()),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Description"),
                    )
                    .child(Input::new(&self.description).small()),
            );

        if let Some(error) = &self.error {
            form = form.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }

        form.child(
            h_flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("create-release-cancel")
                        .outline()
                        .small()
                        .label("Cancel")
                        .disabled(!closable)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.submitting {
                                return;
                            }
                            window.close_dialog(cx);
                        })),
                )
                .child(
                    Button::new("create-release-submit")
                        .primary()
                        .small()
                        .label(if self.submitting {
                            "Creating..."
                        } else {
                            "Create release"
                        })
                        .disabled(disabled)
                        .loading(self.submitting)
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                ),
        )
    }
}
