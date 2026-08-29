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

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, Entity, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
    switch::Switch,
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use crate::actions_view::{page_scaffold, section_heading, suggestions_button};
use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::native_dialog::{self, AlertSpec};
use crate::queries;

pub struct AutomationsView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// EXP-637: run rows whose summary is expanded (decision 5 — collapsed
    /// by default). Per-view and unpersisted; keyed by session row id.
    expanded_runs: std::collections::HashSet<String>,
    _subscriptions: Vec<Subscription>,
}

impl AutomationsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
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
            subscriptions.push(cx.observe(&actions, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&automations, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&devices, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&sessions, |_, _, cx| cx.notify()));
        }
        Self {
            nav,
            scroll: ScrollHandle::new(),
            expanded_runs: std::collections::HashSet::new(),
            _subscriptions: subscriptions,
        }
    }

    fn team_id(&self, cx: &App) -> Option<String> {
        active_team_id(&self.nav, cx)
    }

    // -- rows (EXP-530 / EXP-583) -------------------------------------------

    /// One dense automation row: the target action's glyph + name, the trigger
    /// sentence, the bound device (label + online dot), the agent/model pins,
    /// the next/last run, the enabled toggle and the owner ⋯ menu.
    #[allow(clippy::too_many_arguments)] // one row, one call site
    fn render_automation_row(
        &self,
        index: usize,
        automation: &api::automations::Automation,
        action: Option<&api::actions::Action>,
        devices: &[AutomationDevice],
        sessions: &[&domain::rows::CodingSession],
        is_owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        // EXP-642: the web `GlassRow` hover (`hover:bg-glass-active/50`).
        let row_hover = theme.list_active.opacity(0.5);
        let parsed = crate::automation_editor::parsed_trigger(automation.trigger.as_ref());
        let summary = parsed
            .as_ref()
            .map(coding::automations::trigger_summary)
            // A row whose trigger this build can't even parse still names
            // itself instead of rendering a blank line.
            .unwrap_or_else(|| "Unsupported trigger — update the app".to_string());
        // An action that hasn't synced (or was just deleted) keeps the row
        // visible — the binding is real, and the owner can retarget it.
        let name = action
            .map(|action| action.name.clone())
            .unwrap_or_else(|| "Action".to_string());
        let icon = action.and_then(|action| action.icon.clone());
        // A device that isn't in this user's synced rows (a teammate's private
        // machine) keeps its raw id — the binding is still real.
        let device = devices
            .iter()
            .find(|device| device.device_id == automation.device_id);
        let device_label = device
            .map(|device| device.label.clone())
            .unwrap_or_else(|| automation.device_id.clone());
        let device_online = device.is_some_and(|device| device.online);

        let mut meta = gpui_component::h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_1p5()
            .text_xs()
            .text_color(muted)
            .child(SharedString::from(summary))
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
            .child(SharedString::from(device_label));
        // The pins, when the automation set any — otherwise the run follows
        // the machine's own launch defaults and there is nothing to say.
        if let Some(pins) = launch_pins_label(automation) {
            meta = meta.child(div().child("·")).child(SharedString::from(pins));
        }
        // Schedules can say when they fire next; event triggers cannot.
        if let Some(next) = parsed
            .as_ref()
            .and_then(crate::automation_editor::next_run_label)
        {
            meta = meta
                .child(div().child("·"))
                .child(SharedString::from(format!("Next {next}")));
        }
        // The most recent run THIS automation started (a manual run of the
        // same action says nothing about whether the automation works).
        if let Some(last) = sessions
            .iter()
            .find(|session| fired_by(session, automation))
        {
            meta = meta
                .child(div().child("·"))
                .child(SharedString::from(last_run_label(last)));
        }

        let toggle_id = automation.id.clone();
        let enabled = automation.enabled;
        crate::surface::glass_row_card()
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
                Button::new(("automation-menu", index))
                    .ghost()
                    .cursor_pointer()
                    .xsmall()
                    .icon(Icon::from(registry::UI_MORE))
                    .dropdown_menu(move |menu, _window, _cx| {
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
                            PopupMenuItem::new("Delete")
                                .icon(Icon::from(registry::UI_DELETE))
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
    fn render_automations(
        &self,
        actions: &[api::actions::Action],
        automations: &[api::automations::Automation],
        team_id: Option<&str>,
        is_owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let devices = automation_devices(cx);
        let runs = automated_runs(cx, team_id);
        let run_refs: Vec<&domain::rows::CodingSession> = runs.iter().collect();

        let rows: gpui::AnyElement = if automations.is_empty() {
            crate::controls::empty_state(
                Icon::from(registry::ACTION_AUTOMATION),
                "No automations yet.",
                "Automate an action with a schedule or an issue event.",
                cx,
            )
            .into_any_element()
        } else {
            let rows: Vec<gpui::AnyElement> = automations
                .iter()
                .enumerate()
                .map(|(index, automation)| {
                    let action = actions
                        .iter()
                        .find(|action| action.id == automation.action_id);
                    self.render_automation_row(
                        index, automation, action, &devices, &run_refs, is_owner, cx,
                    )
                })
                .collect();
            gpui_component::v_flex()
                .min_w_0()
                .gap_2()
                .children(rows)
                .into_any_element()
        };

        let mut body = gpui_component::v_flex().min_w_0().gap_6().child(rows);

        // The cross-action run log — the answer to "did the automations fire?"
        let mut recent = gpui_component::v_flex().min_w_0().gap_2().child(section_heading(
            "Recent automated runs",
            Some(runs.len().min(RECENT_RUNS_CAP)),
            None,
            cx,
        ));
        if runs.is_empty() {
            recent = recent.child(
                div()
                    .px_3()
                    .text_xs()
                    .text_color(muted)
                    .child("Nothing has fired yet."),
            );
        }
        for (index, session) in runs.iter().take(RECENT_RUNS_CAP).enumerate() {
            let session_id = session.id.clone();
            let expanded = self.expanded_runs.contains(&session_id);
            // EXP-637: only a run this machine recorded can be resumed here
            // (the workspace is local) — a run from another device shows its
            // summary and nothing else.
            let resumable = crate::coding_flow::run_is_resumable_ref(&session_id, cx);
            // EXP-686: a LIVE run this process hosts opens from its row — the
            // dock expands onto its tab (or its undocked window is raised).
            // A live run on ANOTHER machine has no terminal here, so its card
            // stays inert.
            let live_tab = (!run_has_ended(session))
                .then(|| local_terminal_tab(&session_id, cx))
                .flatten();
            let on_open: Option<Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>> =
                live_tab.map(|(tab, manager)| {
                    Box::new(move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
                        let Some(manager) = manager.upgrade() else {
                            return;
                        };
                        crate::undock::reveal_terminal_tab(
                            tab,
                            manager,
                            window.window_handle(),
                            cx,
                        );
                    }) as Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>
                });
            let toggle_id = session_id.clone();
            let resume_id = session_id.clone();
            recent = recent.child(render_run_row(
                index,
                session,
                expanded,
                resumable,
                cx.listener(move |this: &mut Self, _, _, cx| {
                    if !this.expanded_runs.insert(toggle_id.clone()) {
                        this.expanded_runs.remove(&toggle_id);
                    }
                    cx.notify();
                }),
                move |_, window, cx| {
                    crate::action_run::resume_run(
                        resume_id.clone(),
                        Some(window.window_handle()),
                        false,
                        coding::LaunchOrigin::Local,
                        cx,
                    );
                },
                on_open,
                cx,
            ));
        }
        body = body.child(recent);
        body.into_any_element()
    }
}

impl Render for AutomationsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let team_id = self.team_id(cx);
        let (actions, _) = match team_id.as_deref() {
            Some(team_id) => queries::team_actions(cx, team_id),
            None => (Vec::new(), true),
        };
        let (automations, _) = match team_id.as_deref() {
            Some(team_id) => queries::team_automations(cx, team_id),
            None => (Vec::new(), true),
        };
        let is_owner = team_id
            .as_deref()
            .is_some_and(|team_id| crate::settings::is_owner(cx, team_id));

        // Owner-only "New automation". No agent gate: authoring a binding
        // starts nothing.
        let new_automation = is_owner
            .then(|| team_id.clone())
            .flatten()
            .map(|new_team| {
                Button::new("automations-new")
                    .outline().cursor_pointer()
                    .web_xs()
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
            .child(suggestions_button("automations-suggestions"))
            .children(new_automation)
            .into_any_element();
        let header = section_heading("Automations", Some(automations.len()), Some(trailing), cx);

        let body = self.render_automations(&actions, &automations, team_id.as_deref(), is_owner, cx);
        let section = gpui_component::v_flex()
            .min_w_0()
            .gap_2()
            .child(header)
            .child(body);

        page_scaffold(
            "automations-screen-scroll",
            &self.scroll,
            gpui_component::v_flex().gap_6().child(section),
        )
    }
}

/// How many rows the "Recent automated runs" list shows before it stops.
const RECENT_RUNS_CAP: usize = 10;

/// One synced device, reduced to what an automation row shows.
struct AutomationDevice {
    device_id: String,
    label: String,
    online: bool,
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
    let now_ms = chrono::Utc::now().timestamp_millis();
    collection
        .read(cx)
        .iter()
        .filter_map(|row| {
            let device_id = row.device_id.clone().filter(|id| !id.is_empty())?;
            Some(AutomationDevice {
                label: row.label.clone().unwrap_or_else(|| device_id.clone()),
                online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms),
                device_id,
            })
        })
        .collect()
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
        run_started_at(b)
            .cmp(&run_started_at(a))
            .then_with(|| b.id.cmp(&a.id))
    });
    runs
}

fn run_started_at(session: &domain::rows::CodingSession) -> Option<&str> {
    session
        .started_at
        .as_deref()
        .or(session.created_at.as_deref())
}

/// "Last run ended, 2 hours ago" — the status word plus when it started.
/// EXP-686 dropped the self-reported outcome vocabulary everywhere: a run is
/// either still running or it ended, and the summary says the rest.
fn last_run_label(session: &domain::rows::CodingSession) -> String {
    let status = if run_has_ended(session) { "ended" } else { "running" };
    match run_started_at(session) {
        Some(at) => {
            let when = crate::comments::relative_time(at, chrono::Utc::now().timestamp());
            format!("Last run {status}, {when}")
        }
        None => format!("Last run {status}"),
    }
}

/// Whether the row's synced status is the terminal `ended`.
fn run_has_ended(session: &domain::rows::CodingSession) -> bool {
    session.status.as_deref() == Some(domain::contract::CODING_SESSION_STATUS_ENDED)
}

/// One "Recent automated runs" row: the action's name snapshot and the run's
/// age. No "Automated" badge (EXP-643) — the list header already says so on
/// every client, and EXP-686 dropped the outcome glyph/label with the column
/// itself: an ENDED row shows the time alone, a LIVE one "Running · {when}".
///
/// EXP-637: an ended row is EXPANDABLE (decision 5) — expanded it adds the
/// agent's own summary (rendered as real markdown since EXP-686, with an
/// explicit fallback line when the run left none) and, when the run registry
/// still holds its workspace, a Resume button. A LIVE row has no chevron:
/// `on_open` (present only while THIS machine hosts the run) makes the whole
/// card open that session's terminal instead. Same rule in every runs list on
/// every client.
fn render_run_row(
    index: usize,
    session: &domain::rows::CodingSession,
    expanded: bool,
    resumable: bool,
    on_toggle: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    on_resume: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    on_open: Option<Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>>,
    cx: &App,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let name = session
        .action_name
        .clone()
        // The snapshot survives the action's deletion; only a pre-EXP-253 row
        // could lack it.
        .unwrap_or_else(|| "Action".to_string());
    let ended = run_has_ended(session);
    let when = run_started_at(session)
        .map(|at| crate::comments::relative_time(at, chrono::Utc::now().timestamp()))
        .unwrap_or_default();
    // The ONLY status word left is "Running" (EXP-686) — an ended row is just
    // a name and a time.
    let status = if ended {
        when.clone()
    } else {
        format!("Running · {when}")
    };
    let summary = session.summary.clone().filter(|text| !text.trim().is_empty());
    let session_id = session.id.clone();
    let header = div()
        .flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .child(
            Icon::from(registry::ACTION_AUTOMATION)
                .xsmall()
                .text_color(muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .truncate()
                .text_color(theme.foreground)
                .child(SharedString::from(name)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(status)),
        )
        .when(ended, |this| {
            this.child(
                Button::new(SharedString::from(format!("run-toggle-{session_id}")))
                    .ghost()
                    .xsmall()
                    .icon(
                        Icon::from(if expanded {
                            registry::UI_CHEVRON_UP
                        } else {
                            registry::UI_CHEVRON_DOWN
                        })
                        .xsmall(),
                    )
                    // The card itself may be clickable (a live local run) —
                    // the chevron must never fall through to it.
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        on_toggle(event, window, cx);
                    }),
            )
        });
    let summary_id = session_id.clone();
    crate::surface::glass_row_card()
        .id(("run-card", index))
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .gap_2()
        .px_3()
        .py_2p5()
        .when_some(on_open, |this, on_open| {
            this.cursor_pointer()
                .on_click(move |event, window, cx| on_open(event, window, cx))
        })
        .child(header)
        .when(expanded && ended, |this| {
            this.child(match summary {
                // EXP-686: the agent writes GFM — render it, don't dump the
                // source (the `comments.rs` recipe: selectable so a summary
                // joins the window selection layer).
                Some(summary) => div()
                    .w_full()
                    .min_w_0()
                    .text_xs()
                    .child(
                        crate::markdown::MarkdownView::new(
                            SharedString::from(format!("run-summary-{summary_id}")),
                            summary,
                        )
                        .selectable(true),
                    )
                    .into_any_element(),
                None => div()
                    .w_full()
                    .min_w_0()
                    .text_xs()
                    .text_color(muted)
                    .child("This run left no summary.")
                    .into_any_element(),
            })
            .when(resumable, |this| {
                this.child(
                    div().flex().w_full().child(
                        Button::new(SharedString::from(format!("run-resume-{session_id}")))
                            .ghost()
                            .xsmall()
                            .icon(Icon::from(registry::RUN_RESUME).xsmall())
                            .label("Resume")
                            .on_click(move |event, window, cx| {
                                cx.stop_propagation();
                                on_resume(event, window, cx);
                            }),
                    ),
                )
            })
        })
        .into_any_element()
}

/// The dock tab a LIVE run occupies on THIS machine (EXP-686), if this
/// process is the one hosting it. `None` for a run on another device — there
/// is no terminal here to reveal.
fn local_terminal_tab(
    session_id: &str,
    cx: &App,
) -> Option<(terminal::TabId, gpui::WeakEntity<terminal::TerminalManager>)> {
    let sessions = crate::coding_flow::LocalSessions::global_ref(cx)?;
    let sessions = sessions.read(cx);
    let session = sessions.session_by_id(session_id)?;
    Some((session.tab, session.manager.clone()))
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

