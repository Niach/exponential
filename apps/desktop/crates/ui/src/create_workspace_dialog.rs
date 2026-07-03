//! Create-workspace dialog (masterplan-v3 §4.2 — mirror of
//! `apps/web/src/components/create-workspace-dialog.tsx`).
//!
//! A single name `Input` + Cancel/Create footer. Submit →
//! `workspaces.create`; the close is gated on the new workspace appearing in
//! the synced collection (§4.1), then the window switches to it (the desktop
//! analog of the web's navigate-to-new-slug). Plan-cap FORBIDDEN → the
//! neutral "Upgrade on the web" notification (§4.9).
//!
//! Opened from the workspace picker's "Create workspace…" item (EXP-1 #1)
//! via the [`CreateWorkspace`] action; [`init`] owns the handler.

use gpui::{
    div, px, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    notification::Notification,
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
};
use sync::Store;

use crate::actions::CreateWorkspace;
use crate::create_project_dialog::is_plan_limit;
use crate::navigation::switch_workspace;
use crate::queries;

/// Register the App-global [`CreateWorkspace`] handler (call once from
/// `ui::init`).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &CreateWorkspace, cx| {
        crate::navigation::on_active_window(cx, |window, cx| open(window, cx));
    });
}

/// Open the dialog.
pub fn open(window: &mut Window, cx: &mut App) {
    let view = cx.new(|cx| CreateWorkspaceDialogView::new(window, cx));
    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).submitting;
        dialog
            .w(px(416.)) // web sm:max-w-[26rem]
            .title("Create workspace")
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

pub struct CreateWorkspaceDialogView {
    name: Entity<InputState>,
    submitting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl CreateWorkspaceDialogView {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Side Projects"));

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
            name,
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
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };

        self.error = None;
        self.submitting = true;
        cx.notify();

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::workspaces::workspaces_create(&trpc, &name, None) })
                .await;

            match result {
                Ok(output) => {
                    // Gate on the Electric echo, then switch the window to
                    // the new workspace (web navigates to the new slug).
                    let workspace_id = output.workspace.id.clone();
                    let workspaces = window
                        .update(|_, cx| Store::global(cx).collections().workspaces.clone())
                        .ok();
                    if let Some(workspaces) = workspaces {
                        queries::await_row_visible(&workspaces, &workspace_id, window).await;
                    }
                    let _ = this.update_in(window, |_, window, cx| {
                        window.close_dialog(cx);
                        switch_workspace(window, cx, workspace_id);
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, window, cx| {
                        if is_plan_limit(&err) {
                            window.close_dialog(cx);
                            window.push_notification(
                                Notification::warning(
                                    "Workspace limit reached — upgrade on the web to create more.",
                                ),
                                cx,
                            );
                            return;
                        }
                        // Web keeps the dialog open and shows the message.
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

impl Render for CreateWorkspaceDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.name.update(cx, |state, cx| state.focus(window, cx));
        }

        let disabled = self.name.read(cx).value().trim().is_empty() || self.submitting;
        let closable = !self.submitting;

        let mut form = v_flex().gap_4().child(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Name"),
                )
                .child(Input::new(&self.name).small()),
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
                    Button::new("create-workspace-cancel")
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
                    Button::new("create-workspace-submit")
                        .primary()
                        .small()
                        .label(if self.submitting {
                            "Creating..."
                        } else {
                            "Create workspace"
                        })
                        .disabled(disabled)
                        .loading(self.submitting)
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                ),
        )
    }
}
