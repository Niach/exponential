//! Settings → Feedback widget (EXP-771).
//!
//! Web parity: the first card of `components/team/widget-section.tsx` — the
//! "Exponential widget" heading, its description, and one row per
//! `widget_configs` row of the team.
//!
//! READ-ONLY on purpose. Authoring a widget is the web's job: it needs the
//! embed snippet, the domain allowlist, the mode/label pickers and the theme
//! editor, none of which mean anything in an IDE. What the desktop owes an
//! owner is the ANSWER to "is my widget live and is anything arriving", plus
//! one click to the page that changes it — the same read-only + hand-off
//! shape the General pane's billing summary wears.
//!
//! `widget_configs` is server-only (never an Electric shape), so this is a
//! fetch-on-open tRPC read; `widgets.list` is owner-gated, and the nav only
//! shows the section to owners, so a FORBIDDEN here means the role changed
//! under us and the error notice says so.

use gpui::{
    div, App, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{button::Button, h_flex, v_flex, ActiveTheme as _};

use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::{error_notice, open_url, section, section_description};

/// Web copy, verbatim.
const WIDGET_DESCRIPTION: &str =
    "Embed the Exponential widget on your own site: visitors capture a screenshot, \
     describe the problem, and it lands here as an issue, with reporter email and \
     page context attached.";

enum Load {
    Idle,
    Loading,
    Ready(Result<Vec<api::widgets::WidgetConfigSummary>, String>),
}

pub struct WidgetPane {
    nav: Entity<Navigation>,
    load: Load,
    /// The team the loaded list belongs to — a team switch must refetch.
    team_id: Option<String>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl WidgetPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        Self {
            nav,
            load: Load::Idle,
            team_id: None,
            generation: 0,
            _subscriptions: subscriptions,
        }
    }

    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        if self.team_id.as_deref() != Some(team_id) {
            self.team_id = Some(team_id.to_string());
            self.load = Load::Idle;
        }
        if !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let team = team_id.to_string();

        self.load = Load::Loading;
        self.generation += 1;
        let generation = self.generation;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::widgets::list(&trpc, &team).map_err(|err| err.user_message())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.load = Load::Ready(result);
                cx.notify();
            });
        })
        .detach();
    }

    /// One config row: the name, then "{n} submissions" with a "disabled"
    /// pill beside it when the config is off (web `!widget.enabled`).
    fn render_row(
        &self,
        widget: &api::widgets::WidgetConfigSummary,
        cx: &App,
    ) -> gpui::Div {
        let name: SharedString = widget
            .name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Widget".to_string())
            .into();
        let count = widget.submission_count.unwrap_or(0);
        let submissions: SharedString = format!(
            "{count} submission{}",
            if count == 1 { "" } else { "s" }
        )
        .into();
        // Absent reads as enabled — never accuse a live widget of being off.
        let disabled = widget.enabled == Some(false);

        let mut identity = h_flex().gap_2().items_center().child(
            div()
                .min_w_0()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(name),
        );
        if disabled {
            identity = identity.child(
                crate::surface::glass_pill(
                    SharedString::from(format!("widget-disabled-{}", widget.id)),
                    crate::surface::PillSize::Sm,
                    crate::surface::PillMode::Readonly,
                    cx,
                )
                .child("disabled"),
            );
        }

        // EXP-721: a widget config is an OBJECT — the gapped row card every
        // team-settings entity list wears.
        crate::surface::glass_row_card()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap_0p5()
            .px_3()
            .py_2()
            .child(identity)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(submissions),
            )
    }
}

impl Render for WidgetPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No team selected."),
            );
        };
        self.ensure_loaded(&team_id, cx);

        let mut body = section(cx).child(
            v_flex()
                .child(crate::surface::glass_section_header(
                    "Exponential widget",
                    None,
                    cx,
                ))
                .child(section_description(WIDGET_DESCRIPTION, cx)),
        );

        let muted = cx.theme().muted_foreground;
        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("Loading widgets"),
                );
            }
            Load::Ready(Err(message)) => {
                body = body.child(error_notice(SharedString::from(message.clone()), cx));
            }
            Load::Ready(Ok(widgets)) if widgets.is_empty() => {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("No widgets yet. Create one to get an embed snippet."),
                );
            }
            Load::Ready(Ok(widgets)) => {
                let mut list = v_flex().gap_2();
                for widget in widgets {
                    list = list.child(self.render_row(widget, cx));
                }
                body = body.child(list);
            }
        }

        // The web hand-off — the same recipe the General pane's billing
        // summary uses (`team_general.rs`: instance URL + team slug).
        let slug = super::active_team(cx, &self.nav).and_then(|team| team.slug);
        if let (Some(slug), Some(account)) = (slug, queries::active_account(cx)) {
            let url = format!(
                "{}/t/{slug}/settings/widget",
                account.instance_url.trim_end_matches('/')
            );
            body = body.child(
                h_flex().child(
                    Button::new("widget-manage")
                        .outline()
                        .web_sm()
                        .icon(registry::UI_EXTERNAL_LINK)
                        .label("Manage on the web")
                        .on_click(cx.listener(move |_, _, _, cx| {
                            open_url(cx, url.clone());
                        })),
                ),
            );
        }

        v_flex().child(body)
    }
}
