//! Settings → Widget (EXP-771; SLOP-4 renamed the page from "Feedback
//! widget" — the one path: a submission IS an issue).
//!
//! Web parity: `components/team/widget-section.tsx`'s list — the "Widget"
//! band (its glyph, the "New widget" pill) over one row per `widget_configs`
//! row of the team: the name, the board pill, a "disabled" pill, the
//! submission count and the domain pills; trailing the enable switch and
//! ONE ⋯ (Embed snippet, Delete).
//!
//! The editor itself (`widget-config-dialog.tsx`: the domain allowlist, the
//! label pickers, the theme editor) is the web's: "New widget" and a row
//! click open the web page that runs it.
//!
//! `widget_configs` is server-only (never an Electric shape), so this is a
//! fetch-on-open tRPC read; `widgets.list` is owner-gated, and the nav only
//! shows the section to owners, so a FORBIDDEN here means the role changed
//! under us: the empty state + Retry, quiet like every load on entry
//! (FEED-79).

use gpui::{
    div, prelude::FluentBuilder as _, App, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{
    button::ButtonVariant, h_flex, v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};

use crate::controls::PointerMenuButtonExt as _;
use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::{open_url, section, settings_skeleton};

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
    /// EXP-1274: a refetch of a list already on screen keeps its rows and
    /// spins the band (`reload_requested` asks the next render to start it,
    /// `reloading` is the fetch in flight); only the FIRST load is a skeleton.
    reload_requested: bool,
    reloading: bool,
    /// FEED-79: the load in flight is the person's Retry press (its failure
    /// toasts); a load on entry fails quietly into the empty state.
    retry_pressed: bool,
    /// The widget a switch or a delete is writing — disables its row.
    busy: Option<String>,
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
            reload_requested: false,
            reloading: false,
            retry_pressed: false,
            busy: None,
            _subscriptions: subscriptions,
        }
    }

    /// Server read (`widgets.list` is tRPC — `widget_configs` never syncs), so
    /// the first visit's snapshot would otherwise stand forever: a widget
    /// created on the web through "Manage on the web" stayed invisible until a
    /// team switch. Every entry into the section drops the cache; the next
    /// render refetches (EXP-1274: a list already showing keeps its rows
    /// and the band spins; a failed load starts over at the skeleton — the
    /// way back from an error is re-entering the section or the empty
    /// state's Retry, no Refresh pill).
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        match self.load {
            Load::Ready(Ok(_)) => self.reload_requested = true,
            Load::Ready(Err(_)) => self.load = Load::Idle,
            Load::Idle | Load::Loading => {}
        }
        cx.notify();
    }

    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        if self.team_id.as_deref() != Some(team_id) {
            self.team_id = Some(team_id.to_string());
            self.load = Load::Idle;
            self.reload_requested = false;
            self.reloading = false;
        }
        let reload = self.reload_requested && matches!(self.load, Load::Ready(Ok(_)));
        if !matches!(self.load, Load::Idle) && !reload {
            return;
        }
        self.reload_requested = false;
        let Some(trpc) = queries::trpc_client(cx) else {
            // Nothing to fetch and nothing to wait for — Idle renders the
            // loading line, so leaving it here would spin forever.
            self.load = Load::Ready(Err(
                "Sign in to load this team's widgets.".to_string()
            ));
            return;
        };
        let team = team_id.to_string();

        if reload {
            self.reloading = true;
        } else {
            self.load = Load::Loading;
        }
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
                // FEED-79: a failed load = the empty state + Retry; only a
                // Retry press that fails again toasts.
                let attended = std::mem::take(&mut this.retry_pressed);
                if let Err(message) = &result {
                    crate::action_error::failed_load(
                        "the widgets",
                        attended,
                        Some(message.clone()),
                        cx,
                    );
                }
                this.load = Load::Ready(result);
                this.reloading = false;
                cx.notify();
            });
        })
        .detach();
    }

    /// The failed list's Retry: the same load, attended.
    fn retry_load(&mut self, cx: &mut gpui::Context<Self>) {
        self.retry_pressed = true;
        self.mark_stale(cx);
    }

    /// The switch and the ⋯ Delete: one owner write, then a re-list
    /// (web `toggleEnabled` / `deleteWidget`); a failure toasts.
    fn write(
        &mut self,
        widget_id: String,
        verb: &'static str,
        call: impl FnOnce(&api::TrpcClient, &str) -> Result<(), api::ApiError> + Send + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.busy = Some(widget_id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { call(&trpc, &widget_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                if let Err(err) = &result {
                    crate::action_error::failed_action_err(verb, "the widget", err, cx);
                }
                this.mark_stale(cx);
            });
        })
        .detach();
    }

    /// One config row (web `TeamWidgetSection`): the name, the board pill
    /// and a "disabled" pill over "{n} submissions · " and the domain pills;
    /// the switch and the ⋯ trailing. The row opens the web editor.
    fn render_row(
        &self,
        widget: &api::widgets::WidgetConfigSummary,
        manage_url: Option<String>,
        origin: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let name: SharedString = widget
            .name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Widget".to_string())
            .into();
        let count = widget.submission_count.unwrap_or(0);
        let submissions: SharedString = format!(
            "{count} submission{} \u{00b7}",
            if count == 1 { "" } else { "s" }
        )
        .into();
        // Absent reads as enabled — never accuse a live widget of being off.
        let enabled = widget.enabled != Some(false);
        let busy = self.busy.as_deref() == Some(widget.id.as_str());
        let pill = |id: String, text: SharedString, cx: &App| {
            crate::surface::glass_pill(
                SharedString::from(id),
                crate::surface::PillSize::Sm,
                crate::surface::PillMode::Readonly,
                cx,
            )
            .child(text)
        };

        let mut identity = h_flex().flex_wrap().gap_2().items_center().child(
            div()
                .min_w_0()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(name.clone()),
        );
        if let Some(board) = widget.board_name.clone().filter(|board| !board.is_empty()) {
            identity = identity.child(pill(format!("widget-board-{}", widget.id), board.into(), cx));
        }
        if !enabled {
            identity = identity.child(pill(format!("widget-disabled-{}", widget.id), "disabled".into(), cx));
        }

        let mut meta = h_flex()
            .flex_wrap()
            .gap_1()
            .items_center()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(submissions);
        if widget.allowed_domains.is_empty() {
            // Legacy pre-EXP-209 config: an empty allowlist is denied at
            // serve time, so the widget is dead until domains are added.
            meta = meta.child(
                div()
                    .text_color(cx.theme().warning)
                    .child("Widget blocked: no allowed domains"),
            );
        } else {
            for (index, domain) in widget.allowed_domains.iter().enumerate() {
                meta = meta.child(pill(
                    format!("widget-domain-{}-{index}", widget.id),
                    domain.clone().into(),
                    cx,
                ));
            }
        }

        let toggle_id = widget.id.clone();
        let switch = crate::controls::web_switch(SharedString::from(format!("widget-enabled-{}", widget.id)))
            .checked(enabled)
            .disabled(busy)
            .on_click(cx.listener(move |this, on: &bool, _, cx| {
                let next = *on;
                this.write(
                    toggle_id.clone(),
                    if next { "enable" } else { "disable" },
                    move |trpc, id| api::widgets::set_enabled(trpc, id, next),
                    cx,
                );
            }));

        let view = cx.entity().downgrade();
        let (delete_id, delete_name) = (widget.id.clone(), name.to_string());
        let snippet = widget
            .public_key
            .clone()
            .zip(origin)
            .map(|(key, origin)| widget_snippet(&key, &origin));
        let menu = crate::controls::ghost_icon_button(
            SharedString::from(format!("widget-menu-{}", widget.id)),
            gpui_component::Icon::new(registry::UI_MORE),
            cx,
        )
        .tooltip(SharedString::from(format!("{name} actions")))
        .pointer_menu(move |menu, _window, cx| {
            let snippet = snippet.clone();
            let (view, delete_id, delete_name) = (view.clone(), delete_id.clone(), delete_name.clone());
            menu.item(
                crate::controls::pointer_label_item("Embed snippet", snippet.is_none())
                    .icon(gpui_component::Icon::new(registry::UI_COPY))
                    .on_click(move |_, window, cx| {
                        let Some(snippet) = snippet.clone() else { return };
                        let spec = AlertSpec::new(
                            "Embed snippet",
                            "Paste this before the closing </body> tag of your site.",
                            "Copy snippet",
                        )
                        .on_ok(move |_, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(snippet.clone()));
                            true
                        });
                        native_dialog::open_alert(window, cx, spec);
                    }),
            )
            .item(
                crate::controls::danger_menu_item("Delete", gpui_component::Icon::new(registry::UI_DELETE), cx)
                    .on_click(move |_, window, cx| {
                        let (view, delete_id) = (view.clone(), delete_id.clone());
                        let spec = AlertSpec::new(
                            "Delete widget",
                            "Sites that use its key stop working immediately. Its issues are kept.",
                            "Delete",
                        )
                        .question(format!("Delete the widget \"{delete_name}\"?"))
                        .ok_variant(ButtonVariant::Danger)
                        .on_ok(move |_, cx| {
                            let delete_id = delete_id.clone();
                            let _ = view.update(cx, |this, cx| {
                                this.write(delete_id, "delete", |trpc, id| api::widgets::delete(trpc, id), cx)
                            });
                            true
                        });
                        native_dialog::open_alert(window, cx, spec);
                    }),
            )
        });

        // EXP-1076: a widget config is an OBJECT — one rung of the hairline
        // ladder every settings entity list wears (`list_row` + `flat_row`,
        // the web `SETTINGS_LIST_CLASS` twin).
        let hover = cx.theme().list_hover;
        crate::surface::flat_row()
            .id(SharedString::from(format!("widget-row-{}", widget.id)))
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .when_some(manage_url, |row, url| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(move |_, _, cx| open_url(cx, url.clone()))
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(identity)
                    .child(meta),
            )
            .child(
                // The switch and the menu are their own targets.
                // a11y: a click guard, not a control
                h_flex()
                    .id(SharedString::from(format!("widget-controls-{}", widget.id)))
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(switch)
                    .child(menu),
            )
    }
}

/// Web `buildWidgetSnippet` (`lib/widget-snippet.ts`), byte for byte, with
/// the instance as the origin.
fn widget_snippet(public_key: &str, origin: &str) -> String {
    let script_url = format!("{}/widget/v1/loader.js", origin.trim_end_matches('/'));
    format!(
        r#"<script>
  // Exponential feedback widget. Full docs — init options, identify,
  // setCustomData, setTheme (dark/light/auto), setLocale (en/de/auto), setLauncherHidden, labels,
  // headless submit:
  // https://exponential.at/docs/using/widget/
  (function (w, d, u) {{
    if (w.ExponentialWidget) return;
    var q = [], api = {{ q: q }};
    ["init","identify","setCustomData","setTheme","setLocale","setLauncherHidden","open","close","submit"].forEach(function (m) {{
      api[m] = function () {{ q.push([m, [].slice.call(arguments)]); }};
    }});
    w.ExponentialWidget = api;
    var s = d.createElement("script");
    s.async = true; s.src = u;
    d.head.appendChild(s);
  }})(window, document, "{script_url}");
  ExponentialWidget.init({{ key: "{public_key}" }});
  // Optional: attach your signed-in user so reports carry their contact.
  // ExponentialWidget.identify({{ email: "user@example.com", name: "Jane" }});
  // ExponentialWidget.setCustomData({{ plan: "pro" }});
  // Optional: follow your site's own dark/light toggle.
  // ExponentialWidget.setTheme("light");
  // Optional: widget language ("en", "de" or "auto" = the page language).
  // ExponentialWidget.setLocale("de");
  // Optional: hide the launcher while your own UI covers its corner.
  // ExponentialWidget.setLauncherHidden(true);
</script>"#
    )
}

impl Render for WidgetPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "WidgetPane").entered();
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(crate::controls::empty_state_list(
                gpui_component::Icon::new(crate::icons::registry::UI_TEAM),
                crate::empty_state_copy::NO_TEAM_SELECTED,
                cx,
            ));
        };
        self.ensure_loaded(&team_id, cx);

        // The web page the editor lives on (New widget, a row click).
        let account = queries::active_account(cx);
        let origin = account.as_ref().map(|account| account.instance_url.trim_end_matches('/').to_string());
        let manage_url = super::active_team(cx, &self.nav)
            .and_then(|team| team.slug)
            .zip(origin.clone())
            .map(|(slug, origin)| format!("{origin}/t/{slug}/settings/widget"));
        let new_widget = manage_url.clone().map(|url| {
            crate::surface::glass_pill_button("widget-new", crate::surface::PillSize::Sm, cx)
                .label("New widget")
                .on_click(move |_, _, cx| open_url(cx, url.clone()))
                .into_any_element()
        });

        // No explanatory paragraph under the band (web parity), no Refresh:
        // the list refetches on entry (EXP-1274).
        let mut body = section(cx).child(crate::surface::glass_section_band_loading(
            Some(
                gpui_component::Icon::new(registry::SETTINGS_WIDGET)
                    .xsmall()
                    .text_color(cx.theme().foreground.opacity(0.5))
                    .into_any_element(),
            ),
            "Widget",
            self.reloading,
            new_widget,
            cx,
        ));

        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(settings_skeleton("widgets-skeleton", cx));
            }
            // A failed load (FEED-79): the empty state + Retry, no toast.
            Load::Ready(Err(_)) => {
                body = body.child(crate::action_error::load_failed_state(
                    "widgets-retry",
                    gpui_component::Icon::new(registry::SETTINGS_WIDGET),
                    crate::empty_state_copy::WIDGETS,
                    cx.listener(|this, _, _, cx| this.retry_load(cx)),
                    cx,
                ));
            }
            Load::Ready(Ok(rows)) if rows.is_empty() => {
                body = body.child(crate::controls::empty_state_list(
                    gpui_component::Icon::new(registry::SETTINGS_WIDGET),
                    crate::empty_state_copy::WIDGETS,
                    cx,
                ));
            }
            Load::Ready(Ok(widgets)) => {
                // EXP-1320: flat gapless rows, no hairlines, nothing around
                // them.
                let widgets = widgets.clone();
                let mut list = v_flex().w_full().min_w_0();
                for widget in widgets.iter() {
                    let row = self.render_row(widget, manage_url.clone(), origin.clone(), cx);
                    list = list.child(crate::surface::list_row(row));
                }
                body = body.child(list);
            }
        }

        v_flex().child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::widget_snippet;

    /// The snippet is web `buildWidgetSnippet`'s: the instance's loader and
    /// the widget's key.
    #[test]
    fn snippet_names_the_loader_and_the_key() {
        let snippet = widget_snippet("expw_abc", "https://exponential.example/");
        assert!(snippet.starts_with("<script>\n  // Exponential feedback widget."));
        assert!(snippet.contains(r#"})(window, document, "https://exponential.example/widget/v1/loader.js");"#));
        assert!(snippet.contains(r#"ExponentialWidget.init({ key: "expw_abc" });"#));
        assert!(snippet.ends_with("</script>"));
    }
}
