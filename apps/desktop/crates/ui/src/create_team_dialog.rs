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

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, size, App, AppContext as _, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};
use sync::Store;

use crate::native_dialog::{self, DialogContent, DialogSpec};

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

/// Open the dialog (a native window since EXP-284).
pub fn open(window: &mut Window, cx: &mut App) {
    // Web sm:max-w-[26rem] width; the height hugs the one-field form, with
    // room for a server error under it (EXP-369: 224 → 248, since the footer
    // is pinned and the field/error area is what has to scroll).
    let spec = DialogSpec::new("Create team", size(px(416.), px(248.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| CreateTeamDialogView::new(false, window, cx));
        let busy = view.clone();
        let submit = view.clone();
        DialogContent::new(view)
            // EXP-369: the view pins its own footer and scrolls only the form.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).submitting)
            .on_enter(move |window, cx| {
                submit.update(cx, |view, cx| view.submit(window, cx));
            })
    });
}

/// Emitted (embedded host only, EXP-367) once the created team is VISIBLE in
/// the synced collection — the onboarding wizard advances on it.
pub(crate) struct TeamCreated;
impl gpui::EventEmitter<TeamCreated> for CreateTeamDialogView {}

pub struct CreateTeamDialogView {
    name: Entity<InputState>,
    /// EXP-367: hosted inside the onboarding wizard instead of a native
    /// dialog window — no Cancel button, success EMITS [`TeamCreated`]
    /// instead of closing a dialog + switching the team.
    embedded: bool,
    submitting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    /// EXP-369: the scrolling form pane, so the footer can stay pinned.
    /// Unused in `embedded` mode (the wizard column scrolls instead).
    body_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl CreateTeamDialogView {
    pub(crate) fn new(embedded: bool, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
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
            embedded,
            submitting: false,
            error: None,
            focused_once: false,
            body_scroll: ScrollHandle::new(),
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
                    let _ = this.update_in(window, |view, window, cx| {
                        if view.embedded {
                            // The wizard host advances on the event; the
                            // window stays put (there is no dialog to close).
                            view.submitting = false;
                            switch_team(window, cx, team_id.clone());
                            cx.emit(TeamCreated);
                            cx.notify();
                        } else {
                            native_dialog::close_then(window, cx, move |window, cx| {
                                switch_team(window, cx, team_id);
                            });
                        }
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

        // Cancel exists only in the dialog host — the embedded wizard step has
        // nothing to close.
        let footer = h_flex()
            .flex_shrink_0()
            .justify_end()
            .gap_2()
            .when(!self.embedded, |row| {
                row.child(
                    Button::new("create-team-cancel")
                        .outline()
                        .small()
                        .label("Cancel")
                        .disabled(!closable)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.submitting {
                                return;
                            }
                            native_dialog::close_dialog_window(window, cx);
                        })),
                )
            })
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
            );

        // Embedded (onboarding wizard): the host column owns the scrolling and
        // has no definite height for `size_full` to resolve against — keep the
        // buttons in flow there.
        if self.embedded {
            return form.child(footer).into_any_element();
        }

        // EXP-369: the form scrolls, the buttons stay pinned at the bottom
        // edge (see [`DialogContent::self_scrolling`] in [`open`]).
        let body_scroll = self.body_scroll.clone();
        v_flex()
            .size_full()
            .gap_3()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .id("create-team-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&body_scroll)
                            .child(form),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(Scrollbar::new(&body_scroll).axis(ScrollbarAxis::Vertical)),
                    ),
            )
            .child(footer.pt_3().border_t_1().border_color(cx.theme().border))
            .into_any_element()
    }
}
