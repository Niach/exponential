//! Create-team dialog (masterplan-v3 §4.2 — mirror of
//! `apps/web/src/components/create-team-dialog.tsx`).
//!
//! A single name `Input` + Cancel/Create footer. Submit →
//! `teams.create`; the close is gated on the new team appearing in
//! the synced collection (§4.1), then the window switches to it (the desktop
//! analog of the web's navigate-to-new-slug). Errors render verbatim in the
//! dialog — `teams.create` is open to every authed user (EXP-188); a
//! FORBIDDEN here is the cloud free-tier owned-teams cap (or a real
//! server-side denial) and renders verbatim.
//!
//! Opened from the team picker's "Create team…" item
//! via the [`CreateTeam`] action; [`init`] owns the handler.

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

use crate::actions::CreateTeam;
use crate::navigation::switch_team;
use crate::queries;

/// Register the App-global [`CreateTeam`] handler (call once from
/// `ui::init`).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &CreateTeam, cx| {
        crate::navigation::on_active_window(cx, |window, cx| open(window, cx));
    });
}

/// Open the dialog.
pub fn open(window: &mut Window, cx: &mut App) {
    let view = cx.new(|cx| CreateTeamDialogView::new(window, cx));
    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).submitting;
        dialog
            .w(px(416.)) // web sm:max-w-[26rem]
            .title("Create team")
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

pub struct CreateTeamDialogView {
    name: Entity<InputState>,
    submitting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl CreateTeamDialogView {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Side Boards"));

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
                .spawn(async move { api::teams::teams_create(&trpc, &name, None) })
                .await;

            match result {
                Ok(output) => {
                    // Gate on the Electric echo, then switch the window to
                    // the new team (web navigates to the new slug).
                    let team_id = output.team.id.clone();
                    let teams = window
                        .update(|_, cx| Store::global(cx).collections().teams.clone())
                        .ok();
                    if let Some(teams) = teams {
                        queries::await_row_visible(&teams, &team_id, window).await;
                    }
                    let _ = this.update_in(window, |_, window, cx| {
                        window.close_dialog(cx);
                        switch_team(window, cx, team_id);
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _window, cx| {
                        // Web keeps the dialog open and shows the message —
                        // including the free-tier owned-teams-cap FORBIDDEN
                        // (the server's message says "upgrade" itself).
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

impl Render for CreateTeamDialogView {
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
                    Button::new("create-team-cancel")
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
                    Button::new("create-team-submit")
                        .primary()
                        .small()
                        .label(if self.submitting {
                            "Creating..."
                        } else {
                            "Create team"
                        })
                        .disabled(disabled)
                        .loading(self.submitting)
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                ),
        )
    }
}
