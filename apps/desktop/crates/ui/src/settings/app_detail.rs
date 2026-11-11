//! VAPP-110 / VAPP-177: one app's SETTINGS TAB (apps.json THE SETTINGS TAB,
//! web `app-settings-tab.tsx`), rendered by the app editor
//! ([`crate::app_screen::AppScreenView`]) on its Settings tab: the board
//! form's identity row (icon + colour + name: icon and colour save at once,
//! the name on blur / Enter), the description card (saved on blur), the
//! records line, the Tools group, then Export (VAPP-121, once there is a
//! version) and "Delete app" (the creator or an owner; it leaves for the
//! Apps list), at most `editorTabs.settingsMaxWidth` px wide. Everyone else
//! reads the tab. VAPP-120: under the records line, the Tools group: one row per
//! typed action the head package declares, its title, the function name
//! muted and a switch = the app's grant (`apps.granted_tools`). VAPP-176:
//! no create mode (New app = a draft and its editor).
//!
//! #EXP-1629: the tab is a document on the native binding (projection at
//! `/pane`): the identity row (genUI IconPicker, ColorPicker and a `bare`
//! Input), the description (a `row` Textarea), the records line, Record
//! access, Tools, then `app_mcp`'s groups and the Export / Delete buttons.
//! The share rows' ⋯ are genUI Menus; the Export dialog and every confirm
//! stay native.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use eui_gpui::{binding::Binding, host::HostPlugin, view::SurfaceView};
use gpui::{div, AnyElement, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, WeakEntity, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};
use sync::Store;

use domain::app_actions::{action_grantable, action_title, can_grant_actions, declared_actions, flat_action, APP_ACTIONS_COPY};
use domain::apps::{APPS, APPS_CAPS};
use domain::rows::AppRow;

use super::doc::{hex, not, path, IdeToasts, Ink, SM, XS};
use super::SettingsSection;
use crate::apps_view::AppEntry;
use crate::navigation::{navigate, Screen};
use crate::sidebar::select_settings_section;

/// The web form's defaults (an app saved without an icon or a colour).
const DEFAULT_ICON: &str = "puzzle";
const DEFAULT_COLOR: &str = "#6366f1";
/// VAPP-183: the Connect wait (the MCP servers page's cadence).
const CONNECT_POLL: std::time::Duration = std::time::Duration::from_secs(3);
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// The synced app row, trashed rows excluded.
pub(crate) fn app_row(app_id: &str, cx: &App) -> Option<AppRow> {
    Store::try_global(cx)?
        .collections()
        .apps
        .read(cx)
        .get(app_id)
        .filter(|row| row.deleted_at.is_none())
        .cloned()
}

/// `apps.update` with the row's whole identity, `patch` applied.
fn update_app(cx: &mut App, verb: &'static str, row: &AppRow, patch: impl FnOnce(&mut api::apps::AppInput)) {
    let mut input = api::apps::AppInput {
        id: Some(row.id.clone()),
        name: row.name.clone(),
        icon: row.icon.clone().unwrap_or_else(|| DEFAULT_ICON.to_string()),
        color: row.color.clone().unwrap_or_else(|| DEFAULT_COLOR.to_string()),
        description: row.description.clone(),
        ..Default::default()
    };
    patch(&mut input);
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action(verb, "the app", None, cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::apps::update(&trpc, &input) })
            .await;
        if let Err(err) = result {
            let _ = cx.update(|cx| crate::action_error::failed_action_err(verb, "the app", &err, cx));
        }
    })
    .detach();
}

/// VAPP-120: the typed actions a head package's `functions` declares
/// (patterns expanded, the contract's order); no package = none.
pub(crate) fn declared_tools(package: Option<&serde_json::Value>) -> Vec<String> {
    let functions: Vec<String> = package
        .and_then(|p| p.get("functions"))
        .and_then(serde_json::Value::as_array)
        .map(|list| list.iter().filter_map(|f| f.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    declared_actions(&functions)
}

/// VAPP-120: the whole grant list after one switch (the web's `toggleTool`):
/// on = appended once, off = removed. Round 9 §14: a grant stored dotted
/// goes back under its flat name.
pub(crate) fn toggled_grants(granted: &[String], name: &str, on: bool) -> Vec<String> {
    let mut next: Vec<String> = granted.iter().map(|g| flat_action(g)).filter(|g| g != name).collect();
    if on {
        next.push(name.to_string());
    }
    next
}

/// What one Public links write changes: `setShareLink`'s `action`, or a link's expiry (VAPP-247).
enum ShareChange {
    Action(&'static str),
    Expiry(Option<u32>),
}

pub struct AppDetailPane {
    /// The `apps` row id this tab edits.
    app_id: String,
    /// The Name and description fields' live text while they differ from
    /// the row's (their edits); `None` = untouched since the app loaded.
    name_draft: Option<String>,
    description_draft: Option<String>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// What the document's host draws itself (the rows' leaves).
    host: Rc<RefCell<HostState>>,
    /// The app id the fields were seeded for.
    loaded: Option<String>,
    /// VAPP-122: the team's records quota (`None` = unlimited, or the plan
    /// still loading) and the team it was read for.
    records_max: Option<u64>,
    records_plan_team: Option<String>,
    /// VAPP-120: the tool whose grant is being written (its switch waits).
    tool_busy: Option<String>,
    /// VAPP-183: `appAccess.appTools` for the app on show, the (app, head
    /// version) it was read for, the tool or server being written, and the
    /// live Connect wait (a newer one retires it).
    mcp: Option<api::app_access::AppToolsOut>,
    mcp_for: Option<(String, i64)>,
    mcp_busy: Option<String>,
    connect_generation: u64,
    /// #VAPP-254: the upstream whose server is being written (every Select
    /// waits).
    upstream_busy: Option<String>,
    /// #VAPP-238: the hook whose secret is being minted or rotated, and the
    /// secrets this pane minted (hook name → secret): SHOWN ONCE, held in
    /// memory only, never refetched, gone with the pane or another app.
    hook_busy: Option<String>,
    hook_secrets: std::collections::HashMap<String, String>,
    /// VAPP-241: the public link being written (every switch waits).
    share_busy: Option<String>,
    /// The bench's state (headless has no app row, team or server).
    #[cfg(test)]
    shot: Option<AppShot>,
    _subscriptions: Vec<Subscription>,
}

/// A state the pictures show (`settings_bench`): the headless store syncs
/// no app, so the row, the viewer's powers and the server's answers are
/// pinned.
#[cfg(test)]
pub(crate) struct AppShot {
    pub row: AppRow,
    pub manage: bool,
    pub grant: bool,
    pub records: u64,
    pub records_max: Option<u64>,
    pub mcp: Option<api::app_access::AppToolsOut>,
    pub tool_busy: Option<String>,
    pub mcp_busy: Option<String>,
    pub upstream_busy: Option<String>,
    pub hook_busy: Option<String>,
    pub share_busy: Option<String>,
    pub hook_secrets: Vec<(String, String)>,
}

impl AppDetailPane {
    pub fn new(app_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&collections.apps, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.app_records, |_, _, cx| cx.notify()),
        ];
        // Every action leaves the binding for the pane (the dialogs, the
        // navigation and the toasts need its window), deferred: the binding
        // is mid-update and the pane writes it.
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for &name in ACTIONS {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let context = event.context.clone();
                    let payload = event.payload.clone().unwrap_or(Value::Null);
                    let pane = pane.clone();
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
        let host = Rc::new(RefCell::new(HostState { pane: Some(pane.clone()), ..Default::default() }));
        let plugin: Rc<dyn HostPlugin> = Rc::new(PaneHost { ide: super::doc::ide_plugin(), state: host.clone() });
        let view = super::doc::mount_with("settings-app", document(&Ink::of(cx), &Tints::of(cx)), &binding, plugin, true, window, cx);
        Self {
            app_id,
            name_draft: None,
            description_draft: None,
            binding,
            view,
            host,
            loaded: None,
            records_max: None,
            records_plan_team: None,
            tool_busy: None,
            mcp: None,
            mcp_for: None,
            mcp_busy: None,
            connect_generation: 0,
            upstream_busy: None,
            hook_busy: None,
            hook_secrets: std::collections::HashMap::new(),
            share_busy: None,
            #[cfg(test)]
            shot: None,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new("app-shots".into(), window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: AppShot, cx: &mut gpui::Context<Self>) {
        self.app_id = shot.row.id.clone();
        self.records_max = shot.records_max;
        self.mcp = shot.mcp.clone();
        self.tool_busy = shot.tool_busy.clone();
        self.mcp_busy = shot.mcp_busy.clone();
        self.upstream_busy = shot.upstream_busy.clone();
        self.hook_busy = shot.hook_busy.clone();
        self.share_busy = shot.share_busy.clone();
        self.hook_secrets = shot.hook_secrets.iter().cloned().collect();
        self.shot = Some(shot);
        cx.notify();
    }

    /// The bench's press on a document node (a headless click never
    /// reaches an eUI popup), its popup's enter fade run out.
    #[cfg(test)]
    pub(crate) fn press_for_shots(&mut self, id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.view.update(cx, |view, cx| {
            let index = view.index_of(id).expect("the pressed node");
            view.press(index, window, cx);
            view.advance_clock(std::time::Duration::from_millis(500), cx);
        });
    }

    /// Runs an open popup's fade out after the press's frames.
    #[cfg(test)]
    pub(crate) fn press_for_shots_settle(&mut self, cx: &mut gpui::Context<Self>) {
        self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
    }

    /// The pane's app row (the bench's, under test).
    fn row(&self, id: &str, cx: &App) -> Option<AppRow> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return Some(shot.row.clone());
        }
        app_row(id, cx)
    }

    /// The app's live records (the bench's count, under test).
    fn records(&self, row: &AppRow, cx: &App) -> u64 {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.records;
        }
        Store::global(cx).collections().app_records.read(cx).iter().filter(|record| record.app_id == row.id).count() as u64
    }

    /// The app id on show.
    fn page(&self) -> Option<String> {
        Some(self.app_id.clone())
    }

    fn loaded_row(&self, cx: &App) -> Option<AppRow> {
        self.loaded.as_deref().and_then(|id| self.row(id, cx))
    }

    /// Re-seed the inputs when the page changes (another app), flushing the
    /// previous app's pending edits first.
    fn sync_page(&mut self, page: &str, row: &AppRow, cx: &mut gpui::Context<Self>) {
        if self.loaded.as_deref() == Some(page) {
            return;
        }
        if self.loaded.is_some() {
            self.commit_rename(cx);
            self.commit_description(cx);
        }
        self.loaded = Some(page.to_string());
        self.name_draft = None;
        self.description_draft = None;
        if let Some(team_id) = row.team_id.clone() {
            self.load_records_max(team_id, cx);
        }
    }

    /// VAPP-122: the team's `appRecords` quota off `billing.teamPlan` (the
    /// web's; a failure keeps the count without a max).
    fn load_records_max(&mut self, team_id: String, cx: &mut gpui::Context<Self>) {
        if self.records_plan_team.as_deref() == Some(team_id.as_str()) {
            return;
        }
        #[cfg(test)]
        if self.shot.is_some() {
            return;
        }
        self.records_plan_team = Some(team_id.clone());
        self.records_max = None;
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        cx.spawn(async move |this, cx| {
            let plan_team = team_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::billing::billing_team_plan(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.records_plan_team.as_deref() != Some(plan_team.as_str()) {
                    return; // superseded
                }
                match result {
                    Ok(plan) => {
                        this.records_max = plan.limits.app_records.filter(|max| max.is_finite()).map(|max| max as u64);
                        cx.notify();
                    }
                    Err(err) => log::warn!("[ui] billing.teamPlan failed: {err}"),
                }
            });
        })
        .detach();
    }

    fn can_manage(&self, row: &AppRow, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.manage;
        }
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        let owner = row.team_id.as_deref().is_some_and(|team| super::is_owner(cx, team));
        crate::apps_view::can_manage(&AppEntry::from_row(row), me.as_deref(), owner)
    }

    fn commit_rename(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(row) = self.loaded_row(cx) else { return };
        if !self.can_manage(&row, cx) {
            return;
        }
        let Some(drafted) = self.name_draft.as_deref().map(str::trim).map(str::to_string) else { return };
        // A blank name keeps the saved one (the web restores it).
        if drafted.is_empty() || drafted == row.name {
            return;
        }
        let drafted: String = drafted.chars().take(APPS_CAPS.name_max).collect();
        update_app(cx, "save", &row, |input| input.name = drafted);
    }

    fn commit_description(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(row) = self.loaded_row(cx) else { return };
        if !self.can_manage(&row, cx) {
            return;
        }
        let Some(drafted) = self.description_draft.as_deref() else { return };
        let drafted: String = drafted.chars().take(APPS_CAPS.description_max).collect();
        if drafted == row.description.clone().unwrap_or_default() {
            return;
        }
        update_app(cx, "save", &row, |input| input.description = Some(drafted));
    }

    /// VAPP-120 (the 2026-10-24 ruling): only a team OWNER sets the app's
    /// grants; the creator gets no grant power from creating it.
    fn can_grant(&self, row: &AppRow, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.grant;
        }
        row.team_id
            .as_deref()
            .is_some_and(|team| can_grant_actions(super::is_owner(cx, team).then_some(domain::app_actions::GRANT_ROLE)))
    }

    /// VAPP-120: one switch of the Tools group: `apps.update` with the whole
    /// new `granted_tools` list (owners only, never a destructive action) and
    /// the head version on screen (VAPP-156: a newer head = CONFLICT); a
    /// refusal toasts.
    fn toggle_tool(&mut self, name: String, on: bool, cx: &mut gpui::Context<Self>) {
        let Some(row) = self.loaded_row(cx) else { return };
        if !self.can_grant(&row, cx) || !action_grantable(&name) || self.tool_busy.is_some() {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("save", "the app", None, cx);
            return;
        };
        let input = api::apps::AppInput {
            id: Some(row.id.clone()),
            name: row.name.clone(),
            icon: row.icon.clone().unwrap_or_else(|| DEFAULT_ICON.to_string()),
            color: row.color.clone().unwrap_or_else(|| DEFAULT_COLOR.to_string()),
            description: row.description.clone(),
            granted_tools: Some(toggled_grants(&row.granted_tools, &name, on)),
            // eApps V1 (#VAPP-210): the field carries `grantVersion`.
            head_version: Some(domain::apps::grant_version(domain::apps::AppVersions {
                head: row.head_version.unwrap_or(0),
                published: row.published_version.unwrap_or(0),
            })),
            ..Default::default()
        };
        self.tool_busy = Some(name);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::apps::update(&trpc, &input) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.tool_busy = None;
                if let Err(err) = result {
                    crate::action_error::failed_action_err("save", "the app", &err, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// VAPP-183: (re)read `appAccess.appTools` when the app or its head
    /// version changes (the web's effect deps); a failure hides both sections.
    fn load_mcp(&mut self, row: &AppRow, cx: &mut gpui::Context<Self>) {
        let key = (row.id.clone(), row.head_version.unwrap_or(0));
        if self.mcp_for.as_ref() == Some(&key) {
            return;
        }
        #[cfg(test)]
        if self.shot.is_some() {
            return;
        }
        if self.mcp_for.as_ref().map(|(id, _)| id) != Some(&key.0) {
            self.mcp = None;
            self.hook_secrets.clear();
        }
        self.mcp_for = Some(key.clone());
        self.fetch_mcp(key, cx);
    }

    fn fetch_mcp(&mut self, key: (String, i64), cx: &mut gpui::Context<Self>) {
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        cx.spawn(async move |this, cx| {
            let app_id = key.0.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::app_access::app_tools(&trpc, &app_id) })
                .await;
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

    /// VAPP-183: one MCP tools switch: optimistic, `appAccess.setToolEnabled`
    /// (owners), a refusal restores the row and toasts.
    pub(crate) fn toggle_mcp_tool(&mut self, tool: String, enabled: bool, cx: &mut gpui::Context<Self>) {
        let (Some(data), Some((app_id, _))) = (self.mcp.clone(), self.mcp_for.clone()) else { return };
        if !data.can_toggle || self.mcp_busy.is_some() {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("save", "the tool", None, cx);
            return;
        };
        self.mcp = Some(super::app_mcp::with_tool_enabled(&data, &tool, enabled));
        self.mcp_busy = Some(tool.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::app_access::set_tool_enabled(&trpc, &app_id, &tool, enabled) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.mcp_busy = None;
                if let Err(err) = result {
                    this.mcp = Some(data);
                    crate::action_error::failed_action_err("save", "the tool", &err, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// #VAPP-238: a webhook row's owner pill: Create secret mints at once,
    /// Rotate secret asks first (prompts.json `rotate-webhook-secret`).
    pub(crate) fn hook_action(&mut self, name: String, action: domain::apps::WebhookAction, window: &mut Window, cx: &mut gpui::Context<Self>) {
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
            let result = cx
                .background_executor()
                .spawn(async move { api::app_access::rotate_hook(&trpc, &app_id, &name) })
                .await;
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

    /// VAPP-241: `appAccess.setShareLink` (owners; `action` = `enable` |
    /// `rotate` | `revoke`), then the rows re-read (the state and URL are the
    /// server's); a failure = the failed-action toast.
    pub(crate) fn set_share(&mut self, name: String, action: &'static str, cx: &mut gpui::Context<Self>) {
        self.change_share(name, ShareChange::Action(action), cx);
    }

    /// VAPP-247: the row's expiry item (`days` None = never), the same way.
    pub(crate) fn set_share_expiry(&mut self, name: String, days: Option<u32>, cx: &mut gpui::Context<Self>) {
        self.change_share(name, ShareChange::Expiry(days), cx);
    }

    fn change_share(&mut self, name: String, change: ShareChange, cx: &mut gpui::Context<Self>) {
        let Some((app_id, _)) = self.mcp_for.clone() else { return };
        if self.share_busy.is_some() {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("save", "the link", None, cx);
            return;
        };
        self.share_busy = Some(name.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let for_app = app_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    match change {
                        ShareChange::Action(action) => api::app_access::set_share_link(&trpc, &app_id, &name, action),
                        ShareChange::Expiry(days) => api::app_access::set_share_expiry(&trpc, &app_id, &name, days),
                    }
                    .and_then(|()| api::app_access::app_tools(&trpc, &app_id))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.share_busy = None;
                match result {
                    // Another app on show by now: drop the answer.
                    Ok(_) if this.mcp_for.as_ref().map(|(id, _)| id) != Some(&for_app) => {}
                    Ok(data) => this.mcp = Some(data),
                    Err(err) => crate::action_error::failed_action_err("save", "the link", &err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// #VAPP-254 (`upstreamPickerCases`): an owner's pick on an Accounts row
    /// saves at once (`none` = null), then the rows re-read (the resolved
    /// server and the viewer's connection are the server's); a failure = the
    /// failed-action toast and the old value.
    pub(crate) fn pick_upstream_server(&mut self, upstream: String, value: String, cx: &mut gpui::Context<Self>) {
        let (Some(data), Some((app_id, _))) = (self.mcp.as_ref(), self.mcp_for.clone()) else { return };
        if self.upstream_busy.is_some() {
            return;
        }
        let server_id = Some(value).filter(|value| value != domain::apps::UPSTREAM_PICKER_NONE);
        let current = data.upstreams.iter().find(|row| row.name == upstream).and_then(|row| row.server_id.as_ref());
        if server_id.as_ref() == current {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("save", "the server", None, cx);
            return;
        };
        self.upstream_busy = Some(upstream.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let for_app = app_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::app_access::set_upstream_mapping(&trpc, &app_id, &upstream, server_id.as_deref())
                        .and_then(|()| api::app_access::app_tools(&trpc, &app_id))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.upstream_busy = None;
                match result {
                    // Another app on show by now: drop the answer.
                    Ok(_) if this.mcp_for.as_ref().map(|(id, _)| id) != Some(&for_app) => {}
                    Ok(data) => this.mcp = Some(data),
                    Err(err) => crate::action_error::failed_action_err("save", "the server", &err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// VAPP-183: Connect / Reconnect an account. An OAuth server = the MCP
    /// servers page's flow (`mcp_servers.rs` `connect`): the web settings
    /// deep link runs `mcpServers.connect` in the SIGNED-IN browser, and this
    /// side re-reads the sections until the row reads connected. Any other
    /// server opens Settings → MCP servers.
    /// #VAPP-243: an owner's Add server for an upstream the team lacks:
    /// Settings › MCP with the add form pre-filled with its name.
    pub(crate) fn add_upstream_server(&mut self, name: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        super::mcp_servers::request_add(name, cx);
        // This pane lives in the editor, not on the Settings screen.
        crate::navigation::navigate(window, cx, crate::navigation::Screen::Settings);
        select_settings_section(window, cx, SettingsSection::Mcp);
    }

    pub(crate) fn connect_upstream(
        &mut self,
        upstream: &api::app_access::AppUpstream,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(server_id) = upstream.server_id.clone() else { return };
        if upstream.auth.as_deref() != Some("oauth") {
            crate::navigation::navigate(window, cx, crate::navigation::Screen::Settings);
            select_settings_section(window, cx, SettingsSection::Mcp);
            return;
        }
        let slug = self
            .loaded_row(cx)
            .and_then(|row| row.team_id)
            .and_then(|team| Store::global(cx).collections().teams.read(cx).get(&team).and_then(|t| t.slug.clone()));
        let page = match (slug, crate::queries::active_account(cx)) {
            (Some(slug), Some(account)) => api::mcp_servers::connect_page_url(&account.instance_url, &slug, &server_id),
            _ => None,
        };
        let Some(page) = page else {
            crate::action_error::failed_action("connect", "the account", None, cx);
            return;
        };
        super::open_url(cx, page);
        let Some(key) = self.mcp_for.clone() else { return };
        self.connect_generation += 1;
        let generation = self.connect_generation;
        self.mcp_busy = Some(server_id.clone());
        cx.notify();
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        let trpc = std::sync::Arc::new(trpc);
        cx.spawn(async move |this, cx| {
            let deadline = std::time::Instant::now() + CONNECT_TIMEOUT;
            loop {
                cx.background_executor().timer(CONNECT_POLL).await;
                let live = this
                    .update(cx, |this, _| this.connect_generation == generation && this.mcp_for.as_ref() == Some(&key))
                    .unwrap_or(false);
                if !live {
                    return;
                }
                let (trpc, app_id) = (trpc.clone(), key.0.clone());
                let result = cx
                    .background_executor()
                    .spawn(async move { api::app_access::app_tools(&trpc, &app_id) })
                    .await;
                let connected = result.as_ref().is_ok_and(|data| {
                    data.upstreams.iter().any(|u| u.server_id.as_deref() == Some(server_id.as_str()) && u.state.as_deref() == Some("connected"))
                });
                let done = connected || std::time::Instant::now() >= deadline;
                let _ = this.update(cx, |this, cx| {
                    if let Ok(data) = result {
                        this.mcp = Some(data);
                    }
                    if done {
                        this.mcp_busy = None;
                    }
                    cx.notify();
                });
                if done {
                    return;
                }
            }
        })
        .detach();
    }

    /// A text field's edit (the host's): the live text, and on commit (blur;
    /// Enter is the `rename` action) the save.
    fn edited(&mut self, name: &str, value: String, commit: bool, cx: &mut gpui::Context<Self>) {
        match name {
            "name" => {
                self.name_draft = Some(value);
                if commit {
                    self.commit_rename(cx);
                }
            }
            "description" => {
                self.description_draft = Some(value);
                if commit {
                    self.commit_description(cx);
                }
            }
            _ => {}
        }
    }

    /// A document action.
    fn action(&mut self, name: &str, context: &Value, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let key = context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
        let checked = payload.get("checked").and_then(Value::as_bool);
        let value = context.get("value").or_else(|| payload.get("value")).and_then(Value::as_str).unwrap_or_default().to_string();
        match name {
            "tool" => {
                if let Some(on) = checked {
                    self.toggle_tool(key, on, cx);
                }
            }
            "mcpTool" => {
                if let Some(on) = checked {
                    self.toggle_mcp_tool(key, on, cx);
                }
            }
            "share" => {
                if let Some(on) = checked {
                    self.set_share(key, if on { "enable" } else { "revoke" }, cx);
                }
            }
            // A live share row's ⋯ pick (`app_mcp::share_menu_items`); the
            // trigger is inert while a share change is in flight.
            "menu" if self.share_busy.is_none() => {
                let url = self.mcp.as_ref().and_then(|data| data.shares.iter().find(|share| share.name == key)).and_then(|share| share.url.clone());
                match value.as_str() {
                    "copy" => {
                        let Some(url) = url else { return };
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(url));
                        crate::toast::show(crate::toast::Toast::success(APPS.copy.webhooks.copied.clone()), window, cx);
                    }
                    "rotate" => self.set_share(key, "rotate", cx),
                    "expiry:never" => self.set_share_expiry(key, None, cx),
                    other => {
                        if let Some(days) = other.strip_prefix("expiry:").and_then(|days| days.parse().ok()) {
                            self.set_share_expiry(key, Some(days), cx);
                        }
                    }
                }
            }
            "addServer" => self.add_upstream_server(key, window, cx),
            // An Accounts row's server Select.
            "upstream" => self.pick_upstream_server(key, value, cx),
            "connect" => {
                let upstream = self.mcp.as_ref().and_then(|data| data.upstreams.iter().find(|u| u.name == key)).cloned();
                if let Some(upstream) = upstream {
                    self.connect_upstream(&upstream, window, cx);
                }
            }
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
            // The identity row: icon and colour save at once (an empty pick
            // keeps the saved one).
            "icon" | "color" => {
                let Some(row) = self.loaded_row(cx) else { return };
                if value.is_empty() || !self.can_manage(&row, cx) {
                    return;
                }
                if name == "icon" {
                    update_app(cx, "save", &row, |input| input.icon = value);
                } else {
                    update_app(cx, "save", &row, |input| input.color = value);
                }
            }
            "rename" => self.commit_rename(cx),
            "export" => {
                let Some(row) = self.loaded_row(cx) else { return };
                // dialog stays native: the Export dialog is its own window.
                crate::app_export_dialog::open(window, cx, row.id, false);
            }
            "delete" => {
                let Some(row) = self.loaded_row(cx) else { return };
                if !self.can_manage(&row, cx) {
                    return;
                }
                // dialog stays native: a destructive confirm is the OS alert (prompts fixture).
                crate::apps_view::confirm_delete(
                    row.id,
                    row.name,
                    // VAPP-242: the delete leaves for the team's home (no
                    // Apps list page).
                    |window, cx| navigate(window, cx, Screen::Home),
                    window,
                    cx,
                );
            }
            _ => {}
        }
    }

    /// The state as the document binds it (the host's leaves refilled).
    fn project(&self, row: &AppRow, cx: &App) -> Value {
        let manage = self.can_manage(row, cx);
        let grant = self.can_grant(row, cx);
        // eApps V1: the RUNNING package declares the kinds and the grantable
        // tools (the published one, else the head's).
        let package = row.package.as_ref().filter(|p| p.is_object()).or(row.draft_package.as_ref());
        // #VAPP-243 (apps.json `policyRowCases`): one read-only row per
        // declared kind, its effective policy as the caption.
        let records = package.and_then(|p| p.get("records"));
        let policies: Vec<Value> = domain::apps::policy_kinds(records)
            .into_iter()
            .enumerate()
            .map(|(ix, kind)| {
                let policy = eui_core::host::record_policy(records.and_then(|r| r.get(&kind)));
                let caption = domain::apps::policy_caption([policy.read, policy.create, policy.update, policy.delete]);
                json!({"key": kind, "divided": ix > 0, "kind": kind, "caption": caption})
            })
            .collect();
        // VAPP-120: one row per declared action; the ruling: owners grant, a
        // destructive or owner-only action is never grantable ("Always asks").
        let tools: Vec<Value> = declared_tools(package)
            .into_iter()
            .enumerate()
            .map(|(ix, name)| {
                let grantable = action_grantable(&name);
                json!({
                    "key": name,
                    "divided": ix > 0,
                    "title": action_title(&name),
                    "name": name,
                    "switch": grantable,
                    "always": !grantable,
                    "checked": row.granted_tools.iter().any(|g| flat_action(g) == name),
                    "disabled": !grant || self.tool_busy.as_deref() == Some(name.as_str()),
                })
            })
            .collect();
        let empty = api::app_access::AppToolsOut::default();
        let data = self.mcp.as_ref().unwrap_or(&empty);
        let busy = self.mcp_busy.as_deref();
        let projection = json!({
            "manage": manage,
            "name": row.name,
            "description": row.description.clone().unwrap_or_default(),
            "icon": row.icon.clone().unwrap_or_else(|| DEFAULT_ICON.to_string()),
            "color": row.color.clone().unwrap_or_else(|| DEFAULT_COLOR.to_string()),
            // VAPP-122: the app's live records against the quota.
            "records": domain::app_records::records_count_label(self.records(row, cx), self.records_max),
            "policies": {"show": !policies.is_empty(), "rows": policies},
            "tools": {"hasRows": !tools.is_empty(), "none": tools.is_empty(), "rows": tools},
            // VAPP-241: the published package's public links first (the web's order).
            "shares": super::app_mcp::shares_rows(data, self.share_busy.as_deref()),
            "accounts": super::app_mcp::accounts_rows(data, busy, self.upstream_busy.as_deref()),
            "mcpTools": super::app_mcp::tools_rows(data, busy),
            // #VAPP-220: the published package's automations, read-only.
            "automations": super::app_mcp::automations_rows(data),
            // #VAPP-238: the webhooks those automations listen on.
            "hooks": super::app_mcp::hooks_rows(data, &self.hook_secrets, self.hook_busy.as_deref()),
            // VAPP-121: any member exports an app that has a version.
            "export": AppEntry::from_row(row).head_version > 0,
            "delete": manage,
        });
        projection
    }
}

impl Render for AppDetailPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AppDetailPane").entered();
        let Some(page) = self.page() else {
            return div().into_any_element();
        };
        // The nav clamps a stale App(id) selection away before this pane
        // shows, so a missing row is only a transient frame.
        let Some(row) = self.row(&page, cx) else {
            return div().into_any_element();
        };
        self.sync_page(&page, &row, cx);
        self.load_mcp(&row, cx);
        let projection = self.project(&row, cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone()).into_any_element()
    }
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// The document's actions (all the pane's).
const ACTIONS: &[&str] = &["tool", "mcpTool", "share", "menu", "addServer", "upstream", "connect", "hook", "copy", "icon", "color", "rename", "export", "delete"];

/// The pane the host's text edits go to.
#[derive(Default)]
struct HostState {
    pane: Option<WeakEntity<AppDetailPane>>,
}

/// The pane's host: the IDE's plugin and the text fields' edits for the
/// pane.
struct PaneHost {
    ide: crate::exponential_ui_host::IdePlugin,
    state: Rc<RefCell<HostState>>,
}

impl HostPlugin for PaneHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.ide.icon(name)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.ide.on_unknown(node)
    }

    fn on_input(&self, event: &eui_gpui::host::InputEvent, cx: &mut App) {
        let (name, commit) = (event.name.clone(), event.kind == eui_gpui::host::InputKind::Commit);
        let value = event.value.as_str().unwrap_or_default().to_string();
        let Some(pane) = self.state.borrow().pane.clone() else { return };
        cx.defer(move |cx| {
            let _ = pane.update(cx, |pane, cx| pane.edited(&name, value, commit, cx));
        });
    }
}

// ---------------------------------------------------------------------------
// The document
// ---------------------------------------------------------------------------

/// The colours the document needs beyond [`Ink`]: gpui-component's outline
/// and danger `Button`s.
pub(super) struct Tints {
    pub(super) radius: f32,
    outline_fill: String,
    outline_stroke: String,
    outline_fg: String,
    outline_hover: String,
    pub(super) outline_fill_dim: String,
    pub(super) outline_stroke_dim: String,
    outline_fg_dim: String,
    danger: String,
    danger_fg: String,
    danger_hover: String,
    /// The ghost ⋯'s hover wash.
    pub(super) list_hover: String,
}

impl Tints {
    fn of(cx: &App) -> Self {
        use super::api_keys::premultiplied_hex;
        use gpui_component::Colorize as _;
        let theme = cx.theme();
        Self {
            radius: f32::from(theme.radius),
            outline_fill: hex(theme.input_background()),
            outline_stroke: hex(theme.input),
            outline_fg: hex(theme.button_foreground),
            outline_hover: premultiplied_hex(theme.input.mix_oklab(theme.transparent, 0.5)),
            outline_fill_dim: premultiplied_hex(theme.input_background().opacity(0.5)),
            outline_stroke_dim: hex(theme.input.opacity(0.5)),
            outline_fg_dim: hex(theme.muted_foreground.opacity(0.5)),
            danger: hex(theme.button_danger),
            danger_fg: hex(theme.button_danger_foreground),
            danger_hover: hex(theme.button_danger_hover),
            list_hover: hex(theme.list_hover),
        }
    }
}

/// How a [`button`] paints: gpui-component's outline or danger `Button`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Paint {
    Outline,
    Danger,
}

/// A trailing row Switch (`web_switch`): the genUI Switch's box keeps its
/// label gap (8 px) on its leading side even with no label, which the IDE's
/// bare track has not.
pub(super) fn switch(id: &str, name: &str, event: &str) -> Value {
    json!({"id": id, "component": "Switch", "props": {"label": "", "name": name, "checked": path("checked"), "disabled": path("disabled")},
        "style": {"marginLeft": -8}, "on": {"change": {"event": {"name": event, "context": {"key": path("key")}}}}})
}

/// `node` shown where `visible`.
pub(super) fn visible(mut node: Value, visible: Value) -> Value {
    node["visible"] = visible;
    node
}

/// A `web_sm` `Button` (32 px, `px 12`, the theme radius): the optional
/// `size_3p5` glyph `gap_1` before the `text_sm` label (line height 1,
/// which gpui centres half a point lower: `paddingTop: 1`). Drawn twice,
/// `disabled` picks (the half wash and stroke, the faded muted label; a
/// `loading` Button without a glyph looks the same).
#[allow(clippy::too_many_arguments)]
pub(super) fn button(ink: &Ink, tints: &Tints, id: &str, paint: Paint, icon: Option<&str>, label: Value, disabled: Value, event: Value) -> Value {
    let size = ink.r(SM);
    let glyph = ink.r(0.875);
    let face = |dim: bool| {
        let id = if dim { format!("{id}_dim") } else { id.to_string() };
        let (fill, stroke, fg, hover) = match (paint, dim) {
            (Paint::Outline, false) => (&tints.outline_fill, Some(&tints.outline_stroke), &tints.outline_fg, &tints.outline_hover),
            (Paint::Outline, true) => (&tints.outline_fill_dim, Some(&tints.outline_stroke_dim), &tints.outline_fg_dim, &tints.outline_hover),
            (Paint::Danger, _) => (&tints.danger, None, &tints.danger_fg, &tints.danger_hover),
        };
        let mut children: Vec<Value> = icon.map(|icon| json!({"id": format!("{id}_icon"), "component": "Icon", "props": {"name": icon}, "style": {"width": glyph, "height": glyph, "color": fg, "flexShrink": 0}})).into_iter().collect();
        let mut text = json!({"id": format!("{id}_text"), "component": "Text", "props": {"text": label.clone(), "lines": 1}, "style": {"fontSize": size, "lineHeight": size * 1.25, "color": fg, "flexShrink": 0}});
        if paint == Paint::Outline && dim {
            // The half-muted ink as the node's opacity (a `#rrggbbaa` byte
            // can't say 0.5).
            text["style"]["color"] = json!(ink.muted);
            text["style"]["opacity"] = json!(0.5);
        }
        children.push(text);
        let mut button = json!({"id": id, "component": "Box", "visible": if dim { disabled.clone() } else { not(disabled.clone()) },
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": 32, "paddingHorizontal": 12, "borderRadius": tints.radius, "backgroundColor": fill},
            "children": children});
        if let Some(stroke) = stroke {
            button["style"]["borderWidth"] = json!(1);
            button["style"]["borderColor"] = json!(stroke);
        }
        if !dim {
            button["props"] = json!({"pressable": true});
            button["accessibility"] = json!({"label": label.clone()});
            button["on"] = json!({"press": {"event": event.clone()}});
            button["style"]["cursor"] = json!("pointer");
            button["style"][":hover"] = json!({"backgroundColor": hover});
        }
        button
    };
    json!({"id": format!("{id}_slot"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexShrink": 0}, "children": [face(false), face(true)]})
}

/// A run cut to one line (`truncate`), in the theme's mono family where
/// `mono`.
pub(super) fn text_line(ink: &Ink, id: &str, text: Value, rems: f32, color: &str, mono: bool) -> Value {
    let mut t = ink.text(id, text, rems, color);
    t["props"]["lines"] = json!(1);
    t["style"]["minWidth"] = json!(0);
    if mono {
        t["style"]["fontFamily"] = json!(ink.mono);
    }
    t
}

/// A row's leading column (`flex_1`, `min_w_0`): `top` over `bottom`.
pub(super) fn two_lines(_ink: &Ink, id: &str, top: Value, bottom: Value, extra: Option<Value>) -> Value {
    let children: Vec<Value> = [top, bottom].into_iter().chain(extra).collect();
    json!({"id": format!("{id}_text"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "flexShrink": 1, "minWidth": 0},
        "children": children})
}

/// One group row in a List (`glass_row_shell`, `gap_3`): the hairline
/// above it where `divided`, then `children`.
pub(super) fn row_shell(ink: &Ink, id: &str, children: Vec<Value>) -> Value {
    json!({"id": format!("{id}_item"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [ink.divider(&format!("{id}_divider"), path("divided")), ink.shell(&format!("{id}_row"), false, json!(true), children)]})
}

/// A group's heading (`px_1`, `text_sm` MEDIUM).
fn heading(ink: &Ink, id: &str, title: &str) -> Value {
    let mut text = ink.text(id, json!(title), SM, &ink.foreground);
    text["style"]["fontWeight"] = json!(500);
    text["style"]["paddingHorizontal"] = json!(ink.r(0.25));
    text
}

/// A titled group (`gap_1`) shown where `<data>/show`: the heading over
/// the `glass_group` of `<data>/rows`, one `item` each.
pub(super) fn titled(ink: &Ink, id: &str, title: &str, data: &str, item: Value, _tints: &Tints) -> Value {
    let template = item["id"].as_str().expect("the item's id").to_string();
    let list = json!({"id": format!("{id}_rows"), "component": "List", "props": {"gap": "none"},
        "template": {"component": template, "path": format!("{data}/rows"), "key": "key"}, "children": [item]});
    json!({"id": id, "component": "Box", "visible": path(&format!("{data}/show")), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.25)},
        "children": [heading(ink, &format!("{id}_title"), title), ink.group(&format!("{id}_group"), vec![list])]})
}

/// [`titled`] over an item that is not a plain row.
pub(super) fn titled_item(ink: &Ink, id: &str, title: &str, data: &str, item: Value, tints: &Tints) -> Value {
    titled(ink, id, title, data, item, tints)
}

/// The identity row (`gap_2`): the icon and colour pickers (saved at once)
/// and the name (saved on blur / Enter); then the description card.
fn identity(ink: &Ink) -> Vec<Value> {
    let copy = &APPS.copy;
    let icon = json!({"id": "icon_picker", "component": "IconPicker", "props": {"value": path("/pane/icon"), "color": path("/pane/color")}, "on": {"change": {"event": {"name": "icon"}}}});
    let color = json!({"id": "color_picker", "component": "ColorPicker", "props": {"value": path("/pane/color")}, "on": {"change": {"event": {"name": "color"}}}});
    let name = json!({"id": "name", "component": "Input", "props": {"variant": "bare", "label": "Name", "name": "name", "placeholder": copy.name_placeholder, "value": path("/pane/name"), "disabled": not(path("/pane/manage"))},
        "style": {"flexGrow": 1, "minWidth": 0}, "on": {"submit": {"event": {"name": "rename"}}}});
    let mut row = ink.shell("identity", false, json!(true), vec![icon, color, name]);
    row["style"]["gap"] = json!(ink.r(0.5));
    let description = json!({"id": "description", "component": "Textarea", "props": {"variant": "row", "label": "Description", "name": "description", "rows": 3, "autosize": true, "maxRows": 12,
        "placeholder": copy.description_placeholder, "value": path("/pane/description"), "disabled": not(path("/pane/manage"))}});
    vec![ink.group("identity_group", vec![row]), ink.group("description_group", vec![description])]
}

/// #VAPP-243: Record access, one read-only row per declared kind.
fn policies(ink: &Ink, tints: &Tints) -> Value {
    let row = row_shell(ink, "policy", vec![two_lines(ink, "policy", text_line(ink, "policy_kind", path("kind"), SM, &ink.foreground, true), text_line(ink, "policy_caption", path("caption"), XS, &ink.muted, false), None)]);
    titled(ink, &APPS.test_ids.policies, &APPS.copy.policies.title, "/pane/policies", row, tints)
}

/// VAPP-120: the Tools group (`app-settings-tools`): the title and hint,
/// then one row per declared action with its switch (or "Always asks"),
/// or the none line.
fn tools(ink: &Ink) -> Value {
    let copy = *APP_ACTIONS_COPY;
    let switch = visible(switch("tool_switch", "tool", "tool"), path("switch"));
    let mut always = ink.text("tool_always", json!(copy.always), SM, &ink.muted);
    always["visible"] = path("always");
    always["style"]["flexShrink"] = json!(0);
    let row = row_shell(ink, "tool", vec![two_lines(ink, "tool", text_line(ink, "tool_title", path("title"), SM, &ink.foreground, false), text_line(ink, "tool_name", path("name"), XS, &ink.muted, true), None), switch, always]);
    let list = json!({"id": "tools_rows", "component": "List", "visible": path("/pane/tools/hasRows"), "props": {"gap": "none"},
        "template": {"component": "tool_item", "path": "/pane/tools/rows", "key": "key"}, "children": [row]});
    let none = json!({"id": "tools_none", "component": "Box", "visible": path("/pane/tools/none"), "style": {"display": "flex", "flexDirection": "column", "paddingHorizontal": ink.r(1.), "paddingVertical": ink.r(0.75)},
        "children": [ink.text("tools_none_text", json!(copy.none), SM, &ink.muted)]});
    let mut hint = ink.text("tools_hint", json!(copy.hint), SM, &ink.muted);
    hint["style"]["paddingHorizontal"] = json!(ink.r(0.25));
    json!({"id": "app-settings-tools", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.25)},
        "children": [
            {"id": "tools_head", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [heading(ink, "tools_title", &copy.title), hint]},
            ink.group("tools_group", vec![list, none]),
        ]})
}

/// The tab's tree (`gap_4`, at most `editorTabs.settingsMaxWidth` wide).
fn document(ink: &Ink, tints: &Tints) -> Value {
    let mut records = ink.text("records", path("/pane/records"), SM, &ink.muted);
    records["style"]["paddingHorizontal"] = json!(ink.r(0.25));
    let export = visible(button(ink, tints, "export", Paint::Outline, Some("ui-share"), json!(APPS.copy.menu.export), json!(false), json!({"name": "export"})), path("/pane/export"));
    let delete = visible(button(ink, tints, "delete", Paint::Danger, Some("ui-delete"), json!(APPS.copy.menu.delete), json!(false), json!({"name": "delete"})), path("/pane/delete"));
    let actions = json!({"id": "actions", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "gap": ink.r(0.5), "paddingTop": ink.r(0.5)}, "children": [export, delete]});
    let mut children = identity(ink);
    children.extend([
        records,
        policies(ink, tints),
        tools(ink),
        super::app_mcp::shares_group(ink, tints),
        super::app_mcp::accounts_group(ink, tints),
        super::app_mcp::tools_group(ink, tints),
        super::app_mcp::automations_group(ink, tints),
        super::app_mcp::webhooks_group(ink, tints),
        actions,
    ]);
    json!({"id": APPS.test_ids.settings, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "maxWidth": APPS.editor_tabs.settings_max_width, "gap": ink.r(1.)}, "children": children})
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// #EXP-1629: the tab's document raises no issue a debug build's mount
    /// check refuses (#EXP-1767's class).
    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let ink = Ink {
            rem: 14.,
            primary: "#fafafaff".into(),
            foreground: "#ffffffff".into(),
            hint: "#ffffffff".into(),
            value: "#ffffffff".into(),
            muted: "#a1a1aaff".into(),
            muted_glyph: "#a1a1aaff".into(),
            fill_row: "#ffffff0a".into(),
            stroke_row: "#ffffff14".into(),
            fill_card: "#ffffff0f".into(),
            stroke_card: "#ffffff1a".into(),
            code: "#27272aff".into(),
            radius_lg: 12.,
            radius_md: 8.,
            fill_section: "#ffffff0a".into(),
            fill_active: "#ffffff1a".into(),
            mono: "$type.family.mono".into(),
            pill_button_fill: "#ffffff0f".into(),
            pill_button_fill_disabled: "#ffffff0b".into(),
            primary_button_fill: "#f4f4f5ff".into(),
            primary_button_hover: "#e4e4e7ff".into(),
            primary_button_fill_disabled: "#f4f4f580".into(),
            primary_button_fg: "#18181bff".into(),
        };
        let tints = Tints {
            radius: 10.,
            outline_fill: "#ffffff08".into(),
            outline_stroke: "#ffffff1a".into(),
            outline_fg: "#fafafaff".into(),
            outline_hover: "#ffffff0d".into(),
            outline_fill_dim: "#ffffff04".into(),
            outline_stroke_dim: "#ffffff0d".into(),
            outline_fg_dim: "#a1a1aa80".into(),
            danger: "#dc2626ff".into(),
            danger_fg: "#ffffffff".into(),
            danger_hover: "#b91c1cff".into(),
            list_hover: "#ffffff0f".into(),
        };
        let tree = document(&ink, &tints);
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-app", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn the_tools_group_lists_the_declared_actions() {
        // VAPP-120: patterns expand, non-actions drop, the contract's order.
        let package = json!({"functions": ["harness_toast", "exponential_issues_update_status", "exponential_comments_create"]});
        assert_eq!(
            declared_tools(Some(&package)),
            vec!["exponential_comments_create".to_string(), "exponential_issues_update_status".to_string()]
        );
        assert!(declared_tools(None).is_empty());
        assert!(declared_tools(Some(&json!({"functions": "nope"}))).is_empty());
        assert_eq!(action_title("exponential_issues_update_status"), "Set issue status");
        assert!(!APP_ACTIONS_COPY.none.is_empty());
    }

    #[test]
    fn a_switch_writes_the_whole_grant_list() {
        let granted = vec!["exponential_comments_create".to_string()];
        assert_eq!(
            toggled_grants(&granted, "exponential_issues_update", true),
            vec!["exponential_comments_create".to_string(), "exponential_issues_update".to_string()]
        );
        assert_eq!(toggled_grants(&granted, "exponential_comments_create", true), granted);
        assert!(toggled_grants(&granted, "exponential_comments_create", false).is_empty());
        let dotted = vec!["exponential.comments.create".to_string()];
        assert_eq!(toggled_grants(&dotted, "exponential_issues_update", true), vec!["exponential_comments_create".to_string(), "exponential_issues_update".to_string()]);
        assert!(toggled_grants(&dotted, "exponential_comments_create", false).is_empty());
    }

    #[test]
    fn the_app_row_reads_its_grants_tolerantly() {
        let row: AppRow = serde_json::from_value(json!({"id": "a", "granted_tools": ["exponential_issues_update"]})).unwrap();
        assert_eq!(row.granted_tools, vec!["exponential_issues_update".to_string()]);
        let text: AppRow = serde_json::from_value(json!({"id": "a", "granted_tools": "[\"exponential.issues.get\"]"})).unwrap();
        assert_eq!(text.granted_tools, vec!["exponential.issues.get".to_string()]);
        for garbage in [json!(null), json!(7), json!({"x": 1})] {
            let row: AppRow = serde_json::from_value(json!({"id": "a", "granted_tools": garbage})).unwrap();
            assert!(row.granted_tools.is_empty());
        }
        let absent: AppRow = serde_json::from_value(json!({"id": "a"})).unwrap();
        assert!(absent.granted_tools.is_empty());
    }
}
