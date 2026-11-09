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

use std::rc::Rc;

use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window};
use gpui_component::{button::ButtonVariant, ActiveTheme as _};
use serde_json::{json, Value};

use eui_gpui::binding::Binding;
use eui_gpui::view::SurfaceView;

use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::doc::{and, empty_state, failed_state, hex, mount, not, path, skeleton, IdeToasts, Ink, SM, XS};
use super::open_url;

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
    #[cfg(test)]
    shot_team: Option<String>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

impl WidgetPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        // The actions leave the binding for the pane: the switch and Delete
        // write, the snippet and Delete confirm in a native alert, a row or
        // New widget opens the web editor.
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["open", "toggle", "menu", "retry"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    let payload = event.payload.clone().unwrap_or(Value::Null);
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &key, &payload, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let theme = cx.theme();
        let inks = Inks { list_hover: hex(theme.list_hover), warning: hex(theme.warning) };
        let view = mount("settings-widget", document(&Ink::of(cx), &inks), &binding, window, cx);
        Self {
            nav,
            load: Load::Idle,
            team_id: None,
            generation: 0,
            reload_requested: false,
            reloading: false,
            retry_pressed: false,
            busy: None,
            #[cfg(test)]
            shot_team: None,
            binding,
            view,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = crate::navigation::nav_for_window(window, cx);
        Self::new(nav, window, cx)
    }

    /// The pane in a pinned state (the headless store syncs no team).
    #[cfg(test)]
    pub(crate) fn show(&mut self, load: Option<Vec<api::widgets::WidgetConfigSummary>>, cx: &mut gpui::Context<Self>) {
        self.shot_team = Some("team-shots".into());
        self.team_id = self.shot_team.clone();
        self.load = Load::Ready(load.ok_or_else(|| "offline".to_string()));
        cx.notify();
    }

    /// The team the pane shows (the pictures pin one: the headless store
    /// syncs none).
    fn active_team_id(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if let Some(team) = &self.shot_team {
            return Some(team.clone());
        }
        active_team_id(&self.nav, cx)
    }

    /// The web page the editor lives on (New widget, a row click) and the
    /// instance origin the snippet loads from.
    fn urls(&self, cx: &App) -> (Option<String>, Option<String>) {
        let origin = queries::active_account(cx).map(|account| account.instance_url.trim_end_matches('/').to_string());
        let manage = super::active_team(cx, &self.nav).and_then(|team| team.slug).zip(origin.clone()).map(|(slug, origin)| format!("{origin}/t/{slug}/settings/widget"));
        (manage, origin)
    }

    /// A document action: `open` (the web editor), `toggle` (the switch
    /// of `key`), `menu` (the ⋯ pick on `key`), `retry`.
    fn action(&mut self, name: &str, key: &str, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (manage, origin) = self.urls(cx);
        let widget = match &self.load {
            Load::Ready(Ok(rows)) => rows.iter().find(|row| row.id == key).cloned(),
            _ => None,
        };
        match name {
            "open" => {
                if let Some(url) = manage {
                    open_url(cx, url);
                }
            }
            "retry" => self.retry_load(cx),
            "toggle" if self.busy.is_none() => {
                let (Some(widget), Some(next)) = (widget, payload.get("checked").and_then(Value::as_bool)) else { return };
                self.write(widget.id, if next { "enable" } else { "disable" }, move |trpc, id| api::widgets::set_enabled(trpc, id, next), cx);
            }
            "menu" => {
                let Some(widget) = widget else { return };
                match payload.get("value").and_then(Value::as_str) {
                    Some("snippet") => {
                        let Some(snippet) = widget.public_key.clone().zip(origin).map(|(key, origin)| widget_snippet(&key, &origin)) else { return };
                        let spec = AlertSpec::new("Embed snippet", "Paste this before the closing </body> tag of your site.", "Copy snippet").on_ok(move |_, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(snippet.clone()));
                            true
                        });
                        native_dialog::open_alert(window, cx, spec);
                    }
                    Some("delete") => {
                        let view = cx.entity().downgrade();
                        let delete_id = widget.id.clone();
                        let spec = AlertSpec::new("Delete widget", "Sites that use its key stop working immediately. Its issues are kept.", "Delete")
                            .question(format!("Delete the widget \"{}\"?", widget_name(&widget)))
                            .ok_variant(ButtonVariant::Danger)
                            .on_ok(move |_, cx| {
                                let delete_id = delete_id.clone();
                                let _ = view.update(cx, |this, cx| this.write(delete_id, "delete", |trpc, id| api::widgets::delete(trpc, id), cx));
                                true
                            });
                        native_dialog::open_alert(window, cx, spec);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// The pane's state as the document reads it.
    fn project(&self, team: bool, cx: &App) -> Value {
        let (manage, origin) = self.urls(cx);
        let mut out = json!({"noTeam": !team, "loading": false, "failed": false, "ready": false, "empty": false, "reloading": self.reloading, "canManage": manage.is_some(), "rows": []});
        if !team {
            return out;
        }
        match &self.load {
            Load::Idle | Load::Loading => out["loading"] = json!(true),
            Load::Ready(Err(_)) => out["failed"] = json!(true),
            Load::Ready(Ok(rows)) => {
                out["ready"] = json!(true);
                out["empty"] = json!(rows.is_empty());
                out["rows"] = rows
                    .iter()
                    .map(|widget| {
                        let count = widget.submission_count.unwrap_or(0);
                        let name = widget_name(widget);
                        let board = widget.board_name.clone().filter(|board| !board.is_empty());
                        // Absent reads as enabled — never accuse a live widget of being off.
                        let enabled = widget.enabled != Some(false);
                        let snippet = widget.public_key.is_some() && origin.is_some();
                        json!({
                            "key": widget.id,
                            "name": name,
                            "menuLabel": format!("{name} actions"),
                            "hasBoard": board.is_some(),
                            "board": board.unwrap_or_default(),
                            "disabledPill": !enabled,
                            "enabled": enabled,
                            "busy": self.busy.as_deref() == Some(widget.id.as_str()),
                            "submissions": format!("{count} submission{} \u{00b7}", if count == 1 { "" } else { "s" }),
                            // Legacy pre-EXP-209 config: an empty allowlist
                            // is denied at serve time.
                            "blocked": widget.allowed_domains.is_empty(),
                            "domains": widget.allowed_domains.iter().enumerate().map(|(index, domain)| json!({"key": index.to_string(), "domain": domain})).collect::<Vec<_>>(),
                            "items": [
                                {"label": "Embed snippet", "value": "snippet", "icon": "ui-copy", "disabled": !snippet},
                                {"label": "Delete", "value": "delete", "icon": "ui-delete", "destructive": true},
                            ],
                        })
                    })
                    .collect();
            }
        }
        out
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
}

/// A widget's display name (an unnamed one reads "Widget").
fn widget_name(widget: &api::widgets::WidgetConfigSummary) -> String {
    widget.name.clone().filter(|name| !name.trim().is_empty()).unwrap_or_else(|| "Widget".to_string())
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

/// The inks the rows need beyond [`Ink`].
struct Inks {
    list_hover: String,
    warning: String,
}

/// One config row (web `TeamWidgetSection`): the name, the board pill and
/// a "disabled" pill over "{n} submissions · " and the domain pills; the
/// switch and the ⋯ trailing. The row opens the web editor.
fn row(ink: &Ink, inks: &Inks) -> Value {
    let mut name = ink.text("row_name", path("name"), SM, &ink.foreground);
    name["style"]["fontWeight"] = json!(500);
    name["style"]["minWidth"] = json!(0);
    let mut board = ink.pill("row_board", path("board"), false, None);
    board["visible"] = path("hasBoard");
    let mut disabled = ink.pill("row_disabled", json!("disabled"), false, None);
    disabled["visible"] = path("disabledPill");
    let identity = json!({"id": "row_identity", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "gap": ink.r(0.5)}, "children": [name, board, disabled]});
    let mut blocked = ink.text("row_blocked", json!("Widget blocked: no allowed domains"), XS, &inks.warning);
    blocked["visible"] = path("blocked");
    let domain = ink.pill("row_domain", path("domain"), false, None);
    let meta = json!({"id": "row_meta", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "gap": ink.r(0.25)},
        "template": {"component": "row_domain", "path": "domains", "key": "key"},
        "children": [ink.text("row_submissions", path("submissions"), XS, &ink.muted), blocked, domain]});
    let switch = json!({"id": "row_switch", "component": "Switch", "props": {"label": "", "name": "enabled", "checked": path("enabled"), "disabled": path("busy")},
        "on": {"change": {"event": {"name": "toggle", "context": {"key": path("key")}}}}});
    // a11y: named by the caller ("{name} actions", the node's accessibility label).
    let mut more = ink.ghost_icon_button("row_more", "ui-more", path("menuLabel"), Value::Null, &inks.list_hover);
    more.as_object_mut().expect("the trigger").remove("on");
    let menu = json!({"id": "row_menu", "component": "Menu", "props": {"items": path("items"), "align": "start"}, "on": {"select": {"event": {"name": "menu", "context": {"key": path("key")}}}}, "children": [more]});
    json!({"id": "row", "component": "Box", "props": {"pressable": path("/pane/canManage")}, "on": {"press": {"event": {"name": "open"}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md, "cursor": "pointer", ":hover": {"backgroundColor": inks.list_hover}},
        "children": [
            {"id": "row_text", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0, "gap": ink.r(0.25)}, "children": [identity, meta]},
            {"id": "row_controls", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0, "gap": ink.r(0.25)}, "children": [switch, menu]},
        ]})
}

/// The pane's tree: the "Widget" band (its glyph, the reload spinner, New
/// widget) over the flat rows, the skeleton, the failed Retry or the empty
/// state; no team = the no-team state.
fn document(ink: &Ink, inks: &Inks) -> Value {
    let glyph = ink.r(0.75);
    let leading = json!({"id": "band_icon", "component": "Icon", "props": {"name": "settings-widget"}, "style": {"width": glyph, "height": glyph, "color": ink.foreground, "opacity": 0.5, "flexShrink": 0}});
    let [new_widget, _] = ink.pill_button("new", None, json!("New widget"), json!(false), json!({"name": "open"}));
    let new_widget = json!({"id": "new_slot", "component": "Box", "visible": path("/pane/canManage"), "style": {"display": "flex", "flexShrink": 0}, "children": [new_widget]});
    let band = ink.band_with("band", Some(leading), "Widget", Some(path("/pane/reloading")), Some(new_widget));
    let rows = json!({"id": "rows", "component": "List", "visible": and(path("/pane/ready"), not(path("/pane/empty"))), "props": {"gap": "none"},
        "template": {"component": "row_wrap", "path": "/pane/rows", "key": "key"},
        "children": [{"id": "row_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [row(ink, inks)]}]});
    let section = json!({"id": "section", "component": "Box", "visible": not(path("/pane/noTeam")), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            band,
            skeleton("skeleton", path("/pane/loading")),
            failed_state(ink, "failed", "settings-widget", crate::empty_state_copy::WIDGETS, path("/pane/failed")),
            empty_state(ink, "empty", "settings-widget", crate::empty_state_copy::WIDGETS, and(path("/pane/ready"), path("/pane/empty"))),
            rows,
        ]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [section, empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam"))]})
}

impl Render for WidgetPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "WidgetPane").entered();
        let team = self.active_team_id(cx);
        if let Some(team_id) = &team {
            self.ensure_loaded(team_id, cx);
        }
        let projection = self.project(team.is_some(), cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
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
