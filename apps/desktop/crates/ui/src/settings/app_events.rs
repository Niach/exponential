//! #EXP-1850 (apps.json `eventsTab`, web `app-events-tab.tsx`): an app's
//! EVENTS tab, rendered by the app screen ([`crate::app_screen`]) after
//! Data: ONE scrolling column of three groups, existing pieces only:
//! 1. THE trigger list of the app's agent ([`super::agent_triggers`], every
//!    kind), editable by the app's managers (its creator, team owners).
//! 2. THE Webhooks group ([`super::app_mcp::webhooks_group`], moved here
//!    from the Settings tab) when the agent has a webhook trigger.
//! 3. RECENT RUNS: tRPC `apps.events {id}`, one row per audit row
//!    (`event_run_row`: the title over the muted detail, trailing the
//!    relative time and the small status Pill); a `woke` row opens the Chat
//!    tab ([`OpenChat`]); none = the muted `noRuns` row. Re-read while the
//!    tab shows (every 15 s) and whenever the agent's triggers change.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use eui_gpui::{binding::Binding, host::HostPlugin, view::SurfaceView};
use gpui::{div, App, AppContext as _, Entity, EventEmitter, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window};
use serde_json::{json, Value};
use sync::Store;

use domain::apps::{event_run_row, EventBadge, APPS, APPS_EVENTS_TAB};
use domain::rows::AppRow;

use super::agent_doc::Chrome;
use super::agent_triggers::{self, AgentTriggers};
use super::app_detail::{app_row, two_lines, text_line, Tints};
use super::doc::{not, path, IdeToasts, Ink, SM, XS};
use crate::apps_view::AppEntry;

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";
/// How often Recent runs re-reads while the tab shows (web `REFRESH_MS`).
const REFRESH: Duration = Duration::from_secs(15);

/// The pane's own actions (the trigger list's ride along).
const ACTIONS: [&str; 3] = ["hook", "copy", "openChat"];

/// A `woke` run was pressed: the screen shows its Chat tab.
pub(crate) struct OpenChat;

pub struct AppEventsPane {
    app_id: String,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// THE trigger list's state (a whole-array write in flight).
    triggers: AgentTriggers,
    /// #VAPP-238: `appAccess.appTools` (its hooks) for the (app, head
    /// version) it was read for; the hook whose secret is being written; the
    /// secrets this pane minted: SHOWN ONCE, in memory only.
    mcp: Option<api::app_access::AppToolsOut>,
    mcp_for: Option<(String, i64)>,
    hook_busy: Option<String>,
    hook_secrets: HashMap<String, String>,
    /// Recent runs (`None` while the first read is in flight), the triggers
    /// they were read against and when.
    runs: Option<Vec<api::apps::AppEventRun>>,
    runs_key: Option<String>,
    runs_at: Option<Instant>,
    runs_generation: u64,
    _poll: gpui::Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<OpenChat> for AppEventsPane {}

impl AppEventsPane {
    pub fn new(app_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&collections.apps, |_, _, cx| cx.notify()),
            cx.observe(&collections.agents, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
        ];
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ACTIONS.into_iter().chain(agent_triggers::ACTIONS) {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let context = event.context.clone();
                    let payload = event.payload.clone().unwrap_or(Value::Null);
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &context, &payload, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let plugin: Rc<dyn HostPlugin> = Rc::new(PaneHost { ide: super::doc::ide_plugin() });
        let view = super::doc::mount_with("app-events", document(&Ink::of(cx), &Tints::of(cx), &Chrome::of(cx)), &binding, plugin, true, window, cx);
        // The re-read's clock: a repaint every REFRESH; only a SHOWN tab
        // renders, so only a shown tab re-reads (`render`).
        let poll = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(REFRESH).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        });
        Self {
            app_id,
            binding,
            view,
            triggers: AgentTriggers::default(),
            mcp: None,
            mcp_for: None,
            hook_busy: None,
            hook_secrets: HashMap::new(),
            runs: None,
            runs_key: None,
            runs_at: None,
            runs_generation: 0,
            _poll: poll,
            _subscriptions: subscriptions,
        }
    }

    /// The app's managers (its creator, team owners) edit its agent's
    /// triggers; everyone else reads them.
    fn can_manage(row: &AppRow, cx: &App) -> bool {
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        let owner = row.team_id.as_deref().is_some_and(|team| super::is_owner(cx, team));
        crate::apps_view::can_manage(&AppEntry::from_row(row), me.as_deref(), owner)
    }

    /// The app agent's stored triggers (what a run's title reads).
    fn agent_triggers(row: &AppRow, cx: &App) -> Vec<Value> {
        row.agent_id
            .as_deref()
            .and_then(|id| crate::session_data::agent_def(id, cx))
            .map(|agent| agent.trigger_list())
            .unwrap_or_default()
    }

    /// (Re)read `appAccess.appTools` when the app or its head version
    /// changes; a failure hides the Webhooks group.
    fn load_mcp(&mut self, row: &AppRow, cx: &mut gpui::Context<Self>) {
        let key = (row.id.clone(), row.head_version.unwrap_or(0));
        if self.mcp_for.as_ref() == Some(&key) {
            return;
        }
        if self.mcp_for.as_ref().map(|(id, _)| id) != Some(&key.0) {
            self.mcp = None;
            self.hook_secrets.clear();
        }
        self.mcp_for = Some(key.clone());
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        cx.spawn(async move |this, cx| {
            let app_id = key.0.clone();
            let result = cx.background_executor().spawn(async move { api::app_access::app_tools(&trpc, &app_id) }).await;
            let _ = this.update(cx, |this, cx| {
                if this.mcp_for.as_ref() != Some(&key) {
                    return; // superseded
                }
                match result {
                    Ok(data) => this.mcp = Some(data),
                    Err(err) => {
                        log::warn!("[ui] appAccess.appTools failed: {err}");
                        this.mcp = None;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-read Recent runs when the agent's triggers changed or the last
    /// read is older than [`REFRESH`]. A failure keeps the rows (none on the
    /// first read).
    fn load_runs(&mut self, row: &AppRow, triggers_key: String, cx: &mut gpui::Context<Self>) {
        let fresh = self.runs_at.is_some_and(|at| at.elapsed() < REFRESH);
        if fresh && self.runs_key.as_deref() == Some(triggers_key.as_str()) {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        self.runs_key = Some(triggers_key);
        self.runs_at = Some(Instant::now());
        self.runs_generation += 1;
        let generation = self.runs_generation;
        let app_id = row.id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { api::apps::events(&trpc, &app_id) }).await;
            let _ = this.update(cx, |this, cx| {
                if this.runs_generation != generation {
                    return; // superseded
                }
                match result {
                    Ok(out) => this.runs = Some(out.runs),
                    Err(err) => {
                        log::warn!("[ui] apps.events failed: {err}");
                        this.runs.get_or_insert_with(Vec::new);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// #VAPP-238: a webhook row's owner pill: Create secret mints at once,
    /// Rotate secret asks first (prompts.json `rotate-webhook-secret`).
    fn hook_action(&mut self, name: String, action: domain::apps::WebhookAction, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match action {
            domain::apps::WebhookAction::Mint => self.rotate_hook(name, "create", cx),
            domain::apps::WebhookAction::Rotate => {
                let view = cx.entity().downgrade();
                let prompt = domain::prompts::rotate_webhook_secret(&name);
                let spec = crate::native_dialog::AlertSpec::from_prompt("Rotate secret", &prompt).on_ok(move |_, cx| {
                    let name = name.clone();
                    let _ = view.update(cx, |this, cx| this.rotate_hook(name, "rotate", cx));
                    true
                });
                // dialog stays native: the Rotate secret confirm is the OS alert (prompts.json `rotate-webhook-secret`).
                crate::native_dialog::open_alert(window, cx, spec);
            }
        }
    }

    /// #VAPP-238: `appAccess.rotateHook` (owners): the answer's secret is
    /// kept for the shown-once row and the hook reads Ready; a failure =
    /// the failed-action toast.
    fn rotate_hook(&mut self, name: String, verb: &'static str, cx: &mut gpui::Context<Self>) {
        let Some((app_id, _)) = self.mcp_for.clone() else { return };
        if self.hook_busy.is_some() {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action(verb, "the secret", None, cx);
            return;
        };
        self.hook_busy = Some(name.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let for_app = app_id.clone();
            let result = cx.background_executor().spawn(async move { api::app_access::rotate_hook(&trpc, &app_id, &name) }).await;
            let _ = this.update(cx, |this, cx| {
                this.hook_busy = None;
                match result {
                    // Another app on show by now: drop the answer.
                    Ok(_) if this.mcp_for.as_ref().map(|(id, _)| id) != Some(&for_app) => {}
                    Ok(out) => {
                        if let Some(data) = this.mcp.as_mut() {
                            for hook in data.hooks.iter_mut().filter(|h| h.name == out.name) {
                                hook.has_secret = true;
                            }
                        }
                        this.hook_secrets.insert(out.name, out.secret);
                    }
                    Err(err) => crate::action_error::failed_action_err(verb, "the secret", &err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn action(&mut self, name: &str, context: &Value, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(row) = app_row(&self.app_id, cx) else { return };
        if agent_triggers::ACTIONS.contains(&name) {
            let Some(agent_id) = row.agent_id.clone() else { return };
            let editor = Self::can_manage(&row, cx);
            agent_triggers::handle(self, |pane| &mut pane.triggers, name, agent_triggers::ListTarget::Agent(agent_id), editor, context, payload, window, cx);
            return;
        }
        let key = context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
        match name {
            "hook" => {
                let action = match context.get("action").and_then(Value::as_str) {
                    Some(a) if a == domain::apps::WebhookAction::Rotate.as_str() => domain::apps::WebhookAction::Rotate,
                    Some(a) if a == domain::apps::WebhookAction::Mint.as_str() => domain::apps::WebhookAction::Mint,
                    _ => return,
                };
                self.hook_action(key, action, window, cx);
            }
            // The copy pills: the clipboard, then the success toast.
            "copy" => {
                let text = context.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                crate::toast::show(crate::toast::Toast::success(APPS.copy.webhooks.copied.clone()), window, cx);
            }
            "openChat" => cx.emit(OpenChat),
            _ => {}
        }
    }

    /// The state as the document binds it.
    fn project(&self, row: &AppRow, triggers: &[Value], cx: &App) -> Value {
        let editor = Self::can_manage(row, cx);
        let trigger_list = match row.agent_id.as_deref() {
            Some(agent_id) => agent_triggers::project(&self.triggers, agent_id, editor, cx),
            None => agent_triggers::project_list(&[], false, false),
        };
        let empty = api::app_access::AppToolsOut::default();
        let data = self.mcp.as_ref().unwrap_or(&empty);
        json!({
            "hasAgent": row.agent_id.is_some(),
            "triggerList": trigger_list,
            "hooks": super::app_mcp::hooks_rows(data, &self.hook_secrets, self.hook_busy.as_deref()),
            "runs": runs_projection(self.runs.as_deref(), triggers, now_epoch()),
        })
    }
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Recent runs as the document binds it: `none` = the muted `noRuns` row
/// (nothing while the first read is in flight).
fn runs_projection(runs: Option<&[api::apps::AppEventRun]>, triggers: &[Value], now: i64) -> Value {
    let rows: Vec<Value> = runs
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(ix, run)| {
            let row = event_run_row(run.trigger_id.as_deref(), &run.status, run.error.as_deref(), &run.tool, triggers);
            json!({
                "key": format!("{}-{ix}", run.at),
                "divided": ix > 0,
                "title": row.title,
                "hasDetail": row.detail.is_some(),
                "detail": row.detail.unwrap_or_default(),
                "time": crate::comments::relative_time(&run.at, now),
                "badge": row.badge.label(),
                "woke": row.badge == EventBadge::Woke,
            })
        })
        .collect();
    json!({"none": runs.is_some_and(<[_]>::is_empty), "hasRows": !rows.is_empty(), "rows": rows})
}

impl Render for AppEventsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AppEventsPane").entered();
        let Some(row) = app_row(&self.app_id, cx) else {
            return div().into_any_element();
        };
        let triggers = Self::agent_triggers(&row, cx);
        self.load_mcp(&row, cx);
        self.load_runs(&row, Value::Array(triggers.clone()).to_string(), cx);
        let projection = self.project(&row, &triggers, cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone()).into_any_element()
    }
}

/// The pane's host: the IDE's plugin (the document has no text fields).
struct PaneHost {
    ide: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for PaneHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.ide.icon(name)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.ide.on_unknown(node)
    }
}

// ---------------------------------------------------------------------------
// The document
// ---------------------------------------------------------------------------

/// One Recent runs row: the title over the muted detail, trailing the
/// relative time and the badge Pill; drawn twice, a `woke` row pressable
/// (it opens the Chat tab).
fn run_row(ink: &Ink, chrome: &Chrome, woke: bool) -> Value {
    let id = if woke { "run_woke" } else { "run" };
    let mut detail = text_line(ink, &format!("{id}_detail"), path("detail"), XS, &ink.muted, false);
    detail["visible"] = path("hasDetail");
    let mut time = ink.text(&format!("{id}_time"), path("time"), XS, &ink.muted);
    time["style"]["flexShrink"] = json!(0);
    let mut row = ink.shell(&format!("{id}_row"), false, if woke { path("woke") } else { not(path("woke")) }, vec![
        two_lines(ink, id, text_line(ink, &format!("{id}_title"), path("title"), SM, &ink.foreground, false), detail, None),
        time,
        ink.pill(&format!("{id}_badge"), path("badge"), false, None),
    ]);
    if woke {
        row["props"] = json!({"pressable": true});
        row["on"] = json!({"press": {"event": {"name": "openChat"}}});
        row["style"]["cursor"] = json!("pointer");
        row["style"][":hover"] = json!({"backgroundColor": chrome.list_hover});
    }
    row
}

/// The tab's tree (`gap_4`, at most `editorTabs.settingsMaxWidth`): the
/// agent's Triggers, Webhooks, Recent runs.
fn document(ink: &Ink, tints: &Tints, chrome: &Chrome) -> Value {
    let apps = &*APPS;
    let copy = &APPS_EVENTS_TAB.copy;
    let triggers = json!({"id": "events_triggers", "component": "Box", "visible": path("/pane/hasAgent"), "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [agent_triggers::section(ink, chrome)]});
    let item = json!({"id": "run_item", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [ink.divider("run_divider", path("divided")), run_row(ink, chrome, true), run_row(ink, chrome, false)]});
    let list = json!({"id": apps.test_ids.run_row, "component": "List", "visible": path("/pane/runs/hasRows"), "props": {"gap": "none"},
        "template": {"component": "run_item", "path": "/pane/runs/rows", "key": "key"}, "children": [item]});
    let none = ink.shell("runs_none", false, path("/pane/runs/none"), vec![ink.text("runs_none_text", json!(copy.no_runs), SM, &ink.muted)]);
    let runs = json!({"id": apps.test_ids.recent_runs, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [ink.band("runs_band", &copy.recent_runs, None), ink.group("runs_group", vec![none, list])]});
    json!({"id": apps.test_ids.events_tab, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "maxWidth": apps.editor_tabs.settings_max_width, "gap": ink.r(1.)},
        "children": [triggers, super::app_mcp::webhooks_group(ink, tints), runs]})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tab's document raises no issue a debug build's mount check
    /// refuses (#EXP-1767's class).
    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let tree = document(&super::super::readiness::tests::test_ink(), &Tints::test(), &Chrome::test());
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("app-events", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn recent_runs_read_the_fixture_rows() {
        let triggers = vec![json!({"id": "t1", "kind": "webhook", "name": "stripe", "enabled": false})];
        let run = |status: &str, error: Option<&str>, tool: &str| api::apps::AppEventRun {
            trigger_id: Some("t1".into()),
            at: "2026-11-14T10:00:00Z".into(),
            status: status.into(),
            error: error.map(str::to_string),
            tool: tool.into(),
        };
        let runs = [run("ok", None, "agent:ops"), run("denied", Some("badSignature"), "webhook:stripe")];
        let projection = runs_projection(Some(&runs), &triggers, 0);
        assert_eq!(projection["rows"][0]["title"], json!("When webhook stripe is called"));
        assert_eq!(projection["rows"][0]["badge"], json!("Woke agent"));
        assert_eq!(projection["rows"][0]["woke"], json!(true));
        assert_eq!(projection["rows"][1]["badge"], json!("Refused"));
        assert_eq!(projection["rows"][1]["detail"], json!("The signature did not match"));
        assert_eq!(projection["none"], json!(false));
        assert_eq!(runs_projection(Some(&[]), &triggers, 0)["none"], json!(true));
        assert_eq!(runs_projection(None, &triggers, 0)["none"], json!(false));
    }
}
