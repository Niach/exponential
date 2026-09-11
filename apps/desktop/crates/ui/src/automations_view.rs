//! The Automations center screen (EXP-686): the web `t/$teamSlug/automations`
//! page — the team's `automations` rows (EXP-583: their own entity, never a
//! field on an action) plus the cross-action "Recent automated runs" log.
//!
//! Split out of the old three-tab Actions page verbatim; the Actions screen
//! keeps only its list, Devices keeps the machines, and the suggestion seeds
//! moved to Getting started (the header's lightbulb leads there).
//!
//! Automations are LOCAL-ONLY: there is no server scheduler — the bound
//! device selects its enabled rows off Electric and self-starts. This page is
//! purely the owner's editing surface plus the answer to "did they fire?".
//! The runs list renders even with zero automations (EXP-686): a run log is
//! the first thing you look for after deleting the automation that produced
//! it, and it is the ONLY finished-runs list on any client (EXP-676).
//!
//! EXP-832: `render` builds elements and NOTHING else. gpui re-renders the
//! whole window on any `notify` (a hover on a managed control, a tooltip's
//! tasks elsewhere on the page), and this page used to filter + clone every
//! `coding_sessions` row of the team, parse every trigger and re-join
//! actions/devices per render. All of that now happens ONCE per data change
//! in the `observe` callbacks ([`AutomationsView::refresh`]) into
//! [`AutomationsDerived`], which `render` only reads.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, Entity, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::ButtonVariant,
    menu::{DropdownMenu as _, PopupMenuItem},
    switch::Switch,
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use crate::actions_view::{page_scaffold, suggestions_button};
use crate::surface::glass_section_header;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::native_dialog::{self, AlertSpec};
use crate::queries;
use crate::run_rows;

pub struct AutomationsView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// EXP-832: what the page shows, derived off the synced collections in
    /// [`Self::refresh`] — never in `render`.
    derived: AutomationsDerived,
    _subscriptions: Vec<Subscription>,
}

/// EXP-832: the Automations page's data, ready to draw.
#[derive(Default)]
struct AutomationsDerived {
    /// The team the rows belong to (the active team at the last refresh).
    team_id: Option<String>,
    /// One per automation, in the server's list order, with its joins done.
    rows: Vec<AutomationRow>,
    /// The "Recent automated runs" log, newest first, already capped to
    /// [`RECENT_RUNS_CAP`].
    recent_runs: Vec<domain::rows::CodingSession>,
}

/// One automation row's data: the automation itself plus everything the
/// row used to look up per render — its action's name and glyph, the
/// trigger sentence, the bound device, the launch pins and the newest run it
/// fired. Only the clock-dependent bits (the device's online dot, the last
/// run's relative time) are left to `render`.
struct AutomationRow {
    automation: api::automations::Automation,
    /// The target action's name — "Action" while it hasn't synced (or was
    /// just deleted): the binding is real, and the owner can retarget it.
    action_name: String,
    action_icon: Option<String>,
    /// The trigger sentence (`Every day at 09:00 (device time)`, …).
    summary: String,
    /// The bound device's label and its `last_seen_at` — a device that isn't
    /// in this user's synced rows (a teammate's private machine) keeps its
    /// raw id and shows no dot; the binding is still real.
    device_label: String,
    device_last_seen_at: Option<String>,
    /// "codex · opus" when the automation pinned anything.
    pins: Option<String>,
    /// The most recent run THIS automation started (a manual run of the same
    /// action says nothing about whether the automation works): when it
    /// started and whether it has ended.
    last_run: Option<(Option<String>, bool)>,
}

impl AutomationsDerived {
    /// The page's data for `team_id` (none when no team is active).
    fn compute(cx: &App, team_id: Option<String>) -> Self {
        let Some(team) = team_id.as_deref() else {
            return Self::default();
        };
        let (actions, _) = queries::team_actions(cx, team);
        let (automations, _) = queries::team_automations(cx, team);
        let devices = automation_devices(cx);
        let runs = automated_runs(cx, Some(team));
        let rows = automations
            .into_iter()
            .map(|automation| {
                let action = actions.iter().find(|action| action.id == automation.action_id);
                let device = devices
                    .iter()
                    .find(|device| device.device_id == automation.device_id);
                let last_run = runs
                    .iter()
                    .find(|session| fired_by(session, &automation))
                    .map(|session| {
                        (
                            run_rows::run_started_at(session).map(str::to_string),
                            run_rows::run_has_ended(session),
                        )
                    });
                AutomationRow {
                    summary: trigger_summary_line(automation.trigger.as_ref()),
                    action_name: action
                        .map(|action| action.name.clone())
                        .unwrap_or_else(|| "Action".to_string()),
                    action_icon: action.and_then(|action| action.icon.clone()),
                    device_label: device
                        .map(|device| device.label.clone())
                        .unwrap_or_else(|| automation.device_id.clone()),
                    device_last_seen_at: device.and_then(|device| device.last_seen_at.clone()),
                    pins: launch_pins_label(&automation),
                    last_run,
                    automation,
                }
            })
            .collect();
        Self {
            team_id,
            rows,
            recent_runs: runs.into_iter().take(RECENT_RUNS_CAP).collect(),
        }
    }
}

impl AutomationsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = vec![cx.observe(&nav, |this, _, cx| this.refresh(cx))];
        // EXP-583: the rows join actions (name + glyph), devices (label +
        // online dot) and coding_sessions (last / recent automated runs), so
        // all four synced collections drive this screen.
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (
                collections.actions.clone(),
                collections.automations.clone(),
                collections.devices.clone(),
                collections.coding_sessions.clone(),
            )
        });
        if let Some((actions, automations, devices, sessions)) = watched {
            subscriptions.push(cx.observe(&actions, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&automations, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&devices, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&sessions, |this, _, cx| this.refresh(cx)));
        }
        let derived = AutomationsDerived::compute(cx, active_team_id(&nav, cx));
        Self {
            nav,
            scroll: ScrollHandle::new(),
            derived,
            _subscriptions: subscriptions,
        }
    }

    fn team_id(&self, cx: &App) -> Option<String> {
        active_team_id(&self.nav, cx)
    }

    /// EXP-832: re-derive the page off the collections and repaint — the
    /// ONE place the rows and the run log are computed.
    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.derived = AutomationsDerived::compute(cx, self.team_id(cx));
        cx.notify();
    }

    // -- rows (EXP-530 / EXP-583) -------------------------------------------

    /// One dense automation row: the target action's glyph + name, the trigger
    /// sentence, the bound device (label + online dot), the agent/model pins,
    /// the next/last run, the enabled toggle and the owner ⋯ menu.
    fn render_automation_row(
        &self,
        index: usize,
        row: &AutomationRow,
        now_ms: i64,
        is_owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        // EXP-811: the ONE row hover, `list_hover` (glass fillRow) on every client.
        let row_hover = theme.list_hover;
        let automation = &row.automation;
        // The dot follows the clock: a machine goes offline WITHOUT a row
        // change, so this is the one join left to the render.
        let device_online = row.device_last_seen_at.as_deref().is_some_and(|seen| {
            crate::device_settings::row_is_online(Some(seen), now_ms)
        });

        let mut meta = gpui_component::h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_1p5()
            .text_xs()
            .text_color(muted)
            .child(SharedString::from(row.summary.clone()))
            .child(div().child("·"))
            .when(device_online, |this| {
                this.child(
                    div()
                        .size_1p5()
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(theme::tokens::GREEN.to_hsla()),
                )
            })
            .child(SharedString::from(row.device_label.clone()));
        // The pins, when the automation set any — otherwise the run follows
        // the machine's own launch defaults and there is nothing to say.
        if let Some(pins) = row.pins.clone() {
            meta = meta.child(div().child("·")).child(SharedString::from(pins));
        }
        if let Some((started_at, ended)) = &row.last_run {
            meta = meta.child(div().child("·")).child(SharedString::from(last_run_label(
                started_at.as_deref(),
                *ended,
                now_ms / 1000,
            )));
        }

        let toggle_id = automation.id.clone();
        let enabled = automation.enabled;
        let name = row.action_name.clone();
        let icon = row.action_icon.clone();
        crate::surface::flat_row()
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2p5()
            .hover(move |this| this.bg(row_hover))
            .child(
                div()
                    .flex_shrink_0()
                    .child(crate::icons::action_icon(icon.as_deref()).xsmall().text_color(muted)),
            )
            .child(
                gpui_component::v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .text_color(theme.foreground)
                            .child(SharedString::from(name)),
                    )
                    .child(meta),
            )
            .child(
                // Owner-only per the permissions model: members SEE the state
                // (a disabled switch), owners flip it.
                Switch::new(("automation-enabled", index))
                    .checked(enabled)
                    .disabled(!is_owner)
                    .on_click(cx.listener(move |_, on: &bool, _, cx| {
                        spawn_automation_enabled(cx, toggle_id.clone(), *on);
                    })),
            )
            .children(is_owner.then(|| self.render_automation_menu(index, automation, cx)))
            .into_any_element()
    }

    /// The row's owner ⋯ menu: Edit (the shared form) / Delete (confirmed).
    fn render_automation_menu(
        &self,
        index: usize,
        automation: &api::automations::Automation,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let edit_id = automation.id.clone();
        let delete_view = cx.entity().downgrade();
        let delete_id = automation.id.clone();
        div()
            .flex_shrink_0()
            .child(
                // EXP-698: the one 32px glass chrome every row action wears.
                crate::controls::glass_icon_button(
                    ("automation-menu", index),
                    Icon::from(registry::UI_MORE),
                    cx,
                )
                    .dropdown_menu(move |menu, _window, cx| {
                        let edit_id = edit_id.clone();
                        let delete_view = delete_view.clone();
                        let delete_id = delete_id.clone();
                        menu.item(
                            PopupMenuItem::new("Edit")
                                .icon(Icon::from(registry::UI_EDIT))
                                .on_click(move |_, window, cx| {
                                    crate::automation_dialog::open_edit(
                                        window,
                                        cx,
                                        edit_id.clone(),
                                    );
                                }),
                        )
                        .item(
                            crate::controls::danger_menu_item(
                                "Delete",
                                Icon::from(registry::UI_DELETE),
                                cx,
                            )
                                .on_click(move |_, window, cx| {
                                    let Some(view) = delete_view.upgrade() else {
                                        return;
                                    };
                                    let id = delete_id.clone();
                                    view.update(cx, |this, cx| {
                                        this.prompt_delete_automation(id, window, cx);
                                    });
                                }),
                        )
                    }),
            )
            .into_any_element()
    }

    /// Destructive native actions confirm first (the machines Remove pattern).
    fn prompt_delete_automation(
        &mut self,
        automation_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let spec = AlertSpec::new(
            "Delete automation",
            "Delete this automation? The action stays; nothing will start it \
             automatically any more."
                .to_string(),
            "Delete",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            spawn_automation_delete(cx, automation_id.clone());
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// The page body: one row per automation, then the "Recent automated
    /// runs" list. EXP-686: the runs list renders even with ZERO automations
    /// (deleting the automation must not hide the runs it produced), so the
    /// empty state stands in for the ROWS, never for the whole page.
    fn render_automations(&self, is_owner: bool, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let runs = &self.derived.recent_runs;

        let rows: gpui::AnyElement = if self.derived.rows.is_empty() {
            crate::controls::empty_state(
                Icon::from(registry::ACTION_AUTOMATION),
                "No automations yet.",
                "Automate an action with a schedule or an issue event.",
                cx,
            )
            .into_any_element()
        } else {
            let rows: Vec<gpui::AnyElement> = self
                .derived
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| self.render_automation_row(index, row, now_ms, is_owner, cx))
                .collect();
            gpui_component::v_flex()
                .min_w_0()
                .gap_2()
                .children(rows)
                .into_any_element()
        };

        let mut body = gpui_component::v_flex().min_w_0().gap_6().child(rows);

        // The cross-action run log — the answer to "did the automations fire?"
        // NO gap on the headed section (EXP-697): the header's `pb_2` IS the
        // 8px to the list, so the run rows live in their own gapped column.
        let recent = gpui_component::v_flex()
            .min_w_0()
            .child(glass_section_header("Recent automated runs", None, cx));
        let mut run_rows_column = gpui_component::v_flex().min_w_0();
        if runs.is_empty() {
            run_rows_column = run_rows_column.child(
                div()
                    .px_3()
                    .text_xs()
                    .text_color(muted)
                    .child("Nothing has fired yet."),
            );
        }
        for (index, session) in runs.iter().enumerate() {
            let open_id = session.id.clone();
            // EXP-746: the row itself lives in `run_rows` now — the Devices
            // screen's Running and Past lists draw the same card. EXP-773
            // flattened it to a plain link: the transcript, the run's summary
            // and Resume are the fullscreen session view's, not the list's.
            let parts = run_rows::automation_row_parts(session, now_ms / 1000);
            run_rows_column = run_rows_column.child(run_rows::render_run_row(
                run_rows::RunRowSpec {
                    id_prefix: "run",
                    index,
                    lead: run_rows::RunRowLead::Automation,
                    identifier: None,
                    title: parts.title,
                    caption: Some(parts.caption),
                    on_open: Some(Box::new(move |_, window, cx| {
                        crate::session_screen::open_session(&open_id, window, cx);
                    })),
                    kill: None,
                },
                cx,
            ));
        }
        body = body.child(recent.child(run_rows_column));
        body.into_any_element()
    }
}

impl Render for AutomationsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-832: the rows were derived when the data last changed
        // (`refresh`); this only draws them. The team is read off the
        // derivation too, so a header and its rows never disagree.
        let team_id = self.derived.team_id.clone();
        let is_owner = team_id
            .as_deref()
            .is_some_and(|team_id| crate::settings::is_owner(cx, team_id));

        // Owner-only "New automation". No agent gate: authoring a binding
        // starts nothing.
        let new_automation = is_owner
            .then(|| team_id.clone())
            .flatten()
            .map(|new_team| {
                crate::surface::glass_pill_button("automations-new", crate::surface::PillSize::Sm, cx)
                    .icon(Icon::from(registry::ACTION_AUTOMATION))
                    .label("New automation")
                    .on_click(move |_, window, cx| {
                        crate::automation_dialog::open_new(window, cx, new_team.clone());
                    })
                    .into_any_element()
            });
        // EXP-686: the same lightbulb the Actions header carries.
        let trailing = gpui_component::h_flex()
            .items_center()
            .gap_1()
            .child(suggestions_button("automations-suggestions", cx))
            .children(new_automation)
            .into_any_element();
        let header = glass_section_header("Automations", Some(trailing), cx);

        let body = self.render_automations(is_owner, cx);
        // NO gap here (EXP-697) — the header's `pb_2` is the spacing to the
        // list; the body carries its own row gaps.
        let section = gpui_component::v_flex().min_w_0().child(header).child(body);

        page_scaffold(
            "automations-screen-scroll",
            &self.scroll,
            gpui_component::v_flex().gap_6().child(section),
        )
    }
}

/// How many rows the "Recent automated runs" list shows before it stops.
const RECENT_RUNS_CAP: usize = 10;

/// One synced device, reduced to what an automation row shows. The online
/// dot is derived from `last_seen_at` at render time (EXP-832): a machine
/// goes offline by the clock, without a row change.
struct AutomationDevice {
    device_id: String,
    label: String,
    last_seen_at: Option<String>,
}

/// The synced devices, keyed by their steer id — the Automations list resolves
/// each trigger's `deviceId` through this. Unlike the editor's picker this is
/// NOT cap-filtered: a device that stopped advertising `automations` must
/// still render its label on the trigger bound to it.
fn automation_devices(cx: &App) -> Vec<AutomationDevice> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collection = store.collections().devices.clone();
    collection
        .read(cx)
        .iter()
        .filter_map(|row| {
            let device_id = row.device_id.clone().filter(|id| !id.is_empty())?;
            Some(AutomationDevice {
                label: row.label.clone().unwrap_or_else(|| device_id.clone()),
                last_seen_at: row.last_seen_at.clone(),
                device_id,
            })
        })
        .collect()
}

/// The row's trigger sentence. A schedule fires on the BOUND MACHINE's wall
/// clock, so the recurrence carries the caveat the row used to hang off an
/// absolute next-run date (EXP-812: the calendar moved that date under every
/// screenshot, and the recurrence says the same thing). A row whose trigger
/// this build can't even parse still names itself instead of rendering a
/// blank line.
fn trigger_summary_line(trigger: Option<&serde_json::Value>) -> String {
    crate::automation_editor::parsed_trigger(trigger)
        .as_ref()
        .map(|parsed| {
            let sentence = coding::automations::trigger_summary(parsed);
            if matches!(parsed.kind, coding::automations::TriggerKind::Schedule(_)) {
                format!("{sentence} (device time)")
            } else {
                sentence
            }
        })
        .unwrap_or_else(|| "Unsupported trigger — update the app".to_string())
}

/// EXP-679: `started_reason` is no longer automation-only — `agent` marks a
/// run ANOTHER coding session started, unattended but nobody's automation.
/// Only schedule/event may match an automation's own history; the
/// "Recent automated runs" list below still takes every unattended run
/// (`started_reason.is_some()`, byte-equal with web/iOS/Android — it is the
/// ONLY finished-runs list, EXP-676).
fn started_by_automation(session: &domain::rows::CodingSession) -> bool {
    matches!(session.started_reason.as_deref(), Some("schedule" | "event"))
}

/// Whether `session` was started by `automation`. New rows carry the
/// `automation_id` outright; a run started before EXP-583 (or by a client
/// that predates it) only says WHICH action fired automatically, so the
/// action id + an automation `started_reason` is the fallback.
fn fired_by(
    session: &domain::rows::CodingSession,
    automation: &api::automations::Automation,
) -> bool {
    match session.automation_id.as_deref() {
        Some(id) => id == automation.id,
        None => {
            session.action_id.as_deref() == Some(automation.action_id.as_str())
                && started_by_automation(session)
        }
    }
}

/// "codex · opus" — the pins an automation set, or `None` when it follows the
/// bound machine's own launch defaults (the common case).
fn launch_pins_label(automation: &api::automations::Automation) -> Option<String> {
    let parts: Vec<String> = [
        automation.agent.as_deref(),
        automation.model.as_deref(),
        automation.effort.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.is_empty())
    .map(str::to_string)
    .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// This team's AUTOMATION-started runs, newest first. `started_reason` is the
/// discriminator the server stamps — a manually started run of the same action
/// never appears here.
fn automated_runs(cx: &App, team_id: Option<&str>) -> Vec<domain::rows::CodingSession> {
    let (Some(store), Some(team_id)) = (sync::Store::try_global(cx), team_id) else {
        return Vec::new();
    };
    let collection = store.collections().coding_sessions.clone();
    let mut runs: Vec<domain::rows::CodingSession> = collection
        .read(cx)
        .iter()
        .filter(|session| session.team_id.as_deref() == Some(team_id))
        .filter(|session| session.started_reason.is_some())
        .cloned()
        .collect();
    // ISO-8601 sorts lexicographically — newest first.
    runs.sort_by(|a, b| {
        run_rows::run_started_at(b)
            .cmp(&run_rows::run_started_at(a))
            .then_with(|| b.id.cmp(&a.id))
    });
    runs
}

/// "Last run ended, 2 hours ago" — the status word plus when it started.
/// EXP-686 dropped the self-reported outcome vocabulary everywhere: a run is
/// either still running or it ended, and the summary says the rest.
fn last_run_label(started_at: Option<&str>, ended: bool, now_secs: i64) -> String {
    let status = if ended { "ended" } else { "running" };
    match started_at {
        Some(at) => {
            let when = crate::comments::relative_time(at, now_secs);
            format!("Last run {status}, {when}")
        }
        None => format!("Last run {status}"),
    }
}

/// Flip an automation's `enabled` flag through `automations.update`
/// (EXP-583). ONLY that key rides the wire, so toggling can never move the
/// trigger's fingerprint and re-seed the host's automation state.
fn spawn_automation_enabled(cx: &mut App, automation_id: String, enabled: bool) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    let input = api::automations::AutomationUpdate::enabled(automation_id, enabled);
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::automations::update(&trpc, &input).map(|_| ()) })
            .await;
        let _ = cx.update(|_| {
            // The synced echo repaints the switch — a failure just leaves it
            // where it was.
            if let Err(err) = result {
                log::warn!("actions: enabling/disabling the automation failed: {err}");
            }
        });
    })
    .detach();
}

/// `automations.delete` over tRPC — the synced collection drops the row and
/// the bound device stops evaluating it on its next beat.
fn spawn_automation_delete(cx: &mut App, automation_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::automations::delete(&trpc, &automation_id) })
            .await;
        let _ = cx.update(|_| {
            if let Err(err) = result {
                log::warn!("actions: deleting the automation failed: {err}");
            }
        });
    })
    .detach();
}

