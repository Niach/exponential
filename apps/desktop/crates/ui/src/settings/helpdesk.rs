//! Settings → Helpdesk (EXP-771).
//!
//! Web parity: the SECOND card of `components/team/widget-section.tsx` — the
//! "Enable the helpdesk" toggle and, once it is on, the "Open Support inbox"
//! link. The web keeps it on the widget page because support tickets arrive
//! through the widget; the desktop nav is one section per page, so it gets a
//! row of its own right under Feedback widget.
//!
//! The switch reads the team's SYNCED `helpdesk_enabled` and writes through
//! `teams.update` — no local optimistic flip, the Electric echo moves it (the
//! same rail as every other team-settings toggle). It is the one place two
//! server preconditions can fire, so all three of the web's error branches are
//! mirrored: a plan limit, REV2-10(c)'s mail-transport refusal (whose own
//! wording names the env vars, so it renders verbatim) and everything else.

use gpui::{
    div, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window,
};
use gpui_component::{
    h_flex, switch::Switch, v_flex, ActiveTheme as _, Disableable as _,
};
use sync::Store;

use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::{card_title, error_notice, is_plan_limit, section, upgrade_notice};

/// Web `toggleHelpdesk`'s plan-limit branch, verbatim.
const PLAN_LIMIT_MESSAGE: &str = "The helpdesk is available on the Team plan.";
/// Web `toggleHelpdesk`'s catch-all branch, verbatim.
const GENERIC_FAILURE_MESSAGE: &str = "Could not update the helpdesk setting.";

pub struct HelpdeskPane {
    nav: Entity<Navigation>,
    /// A `teams.update` is in flight — the switch is disabled meanwhile (web
    /// `helpdeskBusy`).
    busy: bool,
    /// Web `helpdeskError`, split by kind: a plan limit takes the §4.9
    /// "Upgrade on the web" notice, everything else the destructive one.
    error: Option<SharedString>,
    limit_notice: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl HelpdeskPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |this, _, cx| {
                // Team switch: the notices belong to the old team.
                this.error = None;
                this.limit_notice = None;
                cx.notify();
            }),
            // The switch renders off the synced team row.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            busy: false,
            error: None,
            limit_notice: None,
            _subscriptions: subscriptions,
        }
    }

    fn set_enabled(&mut self, team_id: String, enabled: bool, cx: &mut gpui::Context<Self>) {
        if self.busy {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.busy = true;
        self.error = None;
        self.limit_notice = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut input = api::teams::TeamsUpdateInput::new(team_id);
                    input.helpdesk_enabled = Some(enabled);
                    api::teams::teams_update(&trpc, &input)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(_) => {}
                    Err(err) if is_plan_limit(&err) => {
                        this.limit_notice = Some(PLAN_LIMIT_MESSAGE.into());
                    }
                    // REV2-10(c): a non-plan PRECONDITION_FAILED is an
                    // actionable SETUP error — the server's message names
                    // AWS_SES_REGION / SMTP_HOST, and this toggle is the only
                    // place it is ever hit, so it reaches the owner verbatim.
                    Err(api::ApiError::Http {
                        status: 412,
                        message,
                    }) => {
                        this.error = Some(message.into());
                    }
                    Err(_) => {
                        this.error = Some(GENERIC_FAILURE_MESSAGE.into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for HelpdeskPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No team selected."),
            );
        };
        let enabled = super::active_team(cx, &self.nav)
            .and_then(|team| team.helpdesk_enabled)
            .unwrap_or(false);

        let mut body = section(cx)
            .child(card_title("Helpdesk"))
            .child(crate::surface::glass_group_rows(vec![
                crate::surface::glass_toggle_row(
                    "Enable the helpdesk",
                    Some(
                        "Give this team a shared support inbox. Support tickets from \
                         the widget land there."
                            .into(),
                    ),
                    Switch::new("team-helpdesk-enabled")
                        .checked(enabled)
                        .disabled(self.busy)
                        .on_click(cx.listener({
                            let team_id = team_id.clone();
                            move |this, checked: &bool, _, cx| {
                                this.set_enabled(team_id.clone(), *checked, cx);
                            }
                        }))
                        .into_any_element(),
                    cx,
                ),
            ]));

        if let Some(notice) = &self.limit_notice {
            body = body.child(upgrade_notice(notice.clone(), cx));
        }
        if let Some(error) = &self.error {
            body = body.child(error_notice(error.clone(), cx));
        }

        // Web: the "Open Support inbox" pill renders only while the helpdesk
        // is on. The rail's Support TOOL WINDOW is this app's inbox (there is
        // no Support screen — `activate_tool` also drops the center tab, so
        // the settings screen closes behind it, which is the navigation the
        // web link performs).
        if enabled {
            body = body.child(
                h_flex().child(
                    crate::surface::glass_pill_button(
                        "helpdesk-open-inbox",
                        crate::surface::PillSize::Sm,
                        cx,
                    )
                    .label("Open Support inbox")
                    .on_click(|_, window, cx| {
                        crate::sidebar::activate_tool(
                            window,
                            cx,
                            crate::sidebar::ToolWindow::Support,
                        );
                    }),
                ),
            );
        }

        v_flex().child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-771: the three branches the web's `toggleHelpdesk` maps, kept
    /// apart. A plan limit is the §4.9 upgrade notice; REV2-10(c)'s
    /// transport refusal is a 412 the server WORDS (it names the env vars),
    /// so it must pass through untouched; anything else is the generic line.
    #[test]
    fn plan_limits_and_setup_errors_are_told_apart() {
        let plan = api::ApiError::Http {
            status: 412,
            message: "Your plan allows 0 helpdesks. Upgrade to enable support."
                .to_string(),
        };
        assert!(is_plan_limit(&plan));

        let transport = api::ApiError::Http {
            status: 412,
            message: "Email sending is not configured on this server, and support \
                      reporters can only reach their conversation through an emailed \
                      link. Set AWS_SES_REGION (Amazon SES) or SMTP_HOST, then enable \
                      support."
                .to_string(),
        };
        assert!(!is_plan_limit(&transport));
        assert!(matches!(
            transport,
            api::ApiError::Http { status: 412, .. }
        ));

        let other = api::ApiError::Http {
            status: 403,
            message: "Only team owners can update a team".to_string(),
        };
        assert!(!is_plan_limit(&other));
        assert!(!matches!(other, api::ApiError::Http { status: 412, .. }));
    }
}
