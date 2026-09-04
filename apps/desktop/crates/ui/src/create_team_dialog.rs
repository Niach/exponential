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

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, size, App, AppContext as _, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Disableable as _,
};
use sync::Store;

use crate::controls::{glass_input, WebControl as _};
use crate::native_dialog::{self, DialogContent, DialogSpec};

use crate::actions::CreateTeam;
use crate::icons::registry;
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
    /// EXP-698, embedded only: the wizard's Back-to-the-choice-page hook.
    /// Present ⇒ the footer becomes the web wizard's `justify-between` row
    /// (ghost Back leading, the primary trailing); absent ⇒ the standalone
    /// dialog's Cancel/Create row is unchanged.
    on_back: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    _subscriptions: Vec<Subscription>,
}

impl CreateTeamDialogView {
    pub(crate) fn new(embedded: bool, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Acme Inc"));

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
            on_back: None,
            _subscriptions: subscriptions,
        }
    }

    /// EXP-698 (embedded host only): give the footer a Back button that
    /// returns the onboarding wizard to its choice page.
    pub(crate) fn with_back(mut self, on_back: Rc<dyn Fn(&mut Window, &mut App)>) -> Self {
        self.on_back = Some(on_back);
        self
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

        let submitted_name = name.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::teams::teams_create(&trpc, &name, None) })
                .await;

            match result {
                Ok(output) => {
                    // Seed the row from the mutation response so the switch
                    // is instant (EXP-470) — the Electric echo only lands
                    // after the shape-identity rotation resolves. The seed
                    // also satisfies the await below immediately; the
                    // pipeline restart makes the rotated shapes re-poll now
                    // instead of waiting out their parked long-polls.
                    let team_id = output.team.id.clone();
                    let seeded = domain::rows::Team::seeded(
                        team_id.clone(),
                        output.team.name.clone().unwrap_or(submitted_name),
                        output.team.slug.clone(),
                    );
                    let teams = window
                        .update(|_, cx| {
                            let store = Store::global(cx).clone();
                            store.collections().seed_team(seeded, cx);
                            store.resync_active(cx);
                            store.collections().teams.clone()
                        })
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
                        this.error = Some(err.user_message().into());
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

        // Embedded (the wizard) mirrors the web wizard card: a foreground
        // "Team name" label over an h-9 input. The standalone window keeps
        // the muted sm form every other native dialog draws.
        let label = if self.embedded {
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .child("Team name")
        } else {
            div().text_sm().text_color(cx.theme().muted_foreground).child("Name")
        };
        let input = if self.embedded {
            glass_input(&self.name, window, cx).web_input()
        } else {
            glass_input(&self.name, window, cx).web_input_sm()
        };
        let mut form = v_flex().gap_4().child(
            v_flex()
                .gap_2()
                .child(label)
                .child(input),
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
        // nothing to close; it gets a Back to the wizard's choice page instead
        // (leading, with the primary pushed to the trailing edge).
        let footer = h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .map(|row| {
                if self.on_back.is_some() {
                    row.justify_between()
                } else {
                    row.justify_end()
                }
            })
            .when_some(self.on_back.clone(), |row, on_back| {
                row.child(
                    Button::new("onboarding-team-back")
                        .ghost()
                        .web_sm()
                        .icon(registry::UI_BACK)
                        .label("Back")
                        .disabled(self.submitting)
                        .on_click(move |_, window, cx| on_back(window, cx)),
                )
            })
            .when(!self.embedded, |row| {
                row.child(
                    Button::new("create-team-cancel")
                        .outline().cursor_pointer()
                        .web_sm()
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
                {
                    let submit = Button::new("create-team-submit").primary().cursor_pointer();
                    if self.embedded { submit.web_md().rounded_full() } else { submit.web_sm() }
                }
                    .label(if self.submitting {
                        "Creating…"
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
