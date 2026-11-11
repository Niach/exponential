//! Settings → Issues (EXP-630): how issues behave team-wide — the estimate
//! scale (Linear's "Estimates" setting, off by default) and the PR
//! automation card (EXP-319, moved here from the Statuses pane). Labels and
//! Statuses hang under this entry in the nav.
//!
//! Web parity: `components/team/issues-section.tsx`. Member-visible like the
//! web page: the PR automation pickers are member-managed, the scale is
//! owner-only (`teams.update`) and reads as plain text for everyone else.
//!
//! #EXP-1629: the pane is a document on eUI's native host binding: each
//! picker a `button` Select (the outline small button, 176 px, web's
//! `w-44`), whose trigger leads with the picked option's own glyph.

use std::rc::Rc;

use eui_gpui::{binding::Binding, view::SurfaceView};
use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement as _, Render, Styled as _, Subscription, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};
use sync::Store;

use domain::rows::IssueStatusRow;
use domain::statuses::{resolve_pr_target, resolve_row, IssueStatusCategory, ResolvedStatus};

use crate::navigation::{active_team_id, Navigation};

use super::doc::{hex, path, IdeToasts, Ink, SM};
use super::is_owner;

/// Both pickers share this width so their edges line up (EXP-328, web's
/// `w-44`).
const PICKER_WIDTH: f32 = 176.;

/// The picker key of the row that is NOT a status: `*Automation=false`
/// (web's `none`). It can never collide with a real key — those are a row
/// uuid or `builtin:<key>`.
const PR_TARGET_NONE: &str = "none";

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// The settings picker rows, in Linear's order and wording (web
/// `ESTIMATION_TYPE_OPTIONS`).
const ESTIMATION_OPTIONS: &[(&str, &str, &str)] = &[
    (domain::contract::ISSUE_ESTIMATION_NONE, "Not in use", ""),
    (domain::contract::ISSUE_ESTIMATION_EXPONENTIAL, "Exponential", "1, 2, 4, 8, 16 points"),
    (domain::contract::ISSUE_ESTIMATION_FIBONACCI, "Fibonacci", "1, 2, 3, 5, 8 points"),
    (domain::contract::ISSUE_ESTIMATION_LINEAR, "Linear", "1, 2, 3, 4, 5 points"),
    (domain::contract::ISSUE_ESTIMATION_TSHIRT, "T-shirt", "XS, S, M, L, XL"),
];

/// "T-shirt" / "Not in use": the read-only text (web's label alone).
fn estimation_label(value: &str) -> &'static str {
    ESTIMATION_OPTIONS.iter().find(|(wire, _, _)| *wire == value).map_or(ESTIMATION_OPTIONS[0].1, |(_, label, _)| label)
}

pub struct IssuesPane {
    nav: Entity<Navigation>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// The bench's state (headless has no team, membership or statuses).
    #[cfg(test)]
    shot: Option<IssuesShot>,
    /// The bench's frames to wait before its press (a picker's trigger).
    #[cfg(test)]
    shot_press: Option<(String, u8)>,
    _subscriptions: Vec<Subscription>,
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct IssuesShot {
    pub team: domain::rows::Team,
    pub owner: bool,
    pub statuses: Vec<IssueStatusRow>,
    /// The picker shown open (0 = PR opens, 1 = PR merges, 2 = the scale).
    pub open: Option<usize>,
}

impl IssuesPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // Both cards read the synced teams row — without this the
            // mutation's Electric echo never re-renders them.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify()),
        ];
        let pane = cx.weak_entity();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["estimate", "prOpened", "prMerged"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let value = event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str).unwrap_or_default().to_string();
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update.
                    cx.defer(move |cx| {
                        let _ = pane.update(cx, |pane, cx| pane.action(name, value, cx));
                    });
                    None
                });
            }
            binding
        });
        let view = super::doc::mount_with("settings-issues", document(&Ink::of(cx)), &binding, Rc::new(super::doc::ide_plugin()), true, window, cx);
        Self {
            nav,
            binding,
            view,
            #[cfg(test)]
            shot: None,
            #[cfg(test)]
            shot_press: None,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(crate::navigation::nav_for_window(window, cx), window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: IssuesShot, cx: &mut gpui::Context<Self>) {
        self.shot_press = shot.open.map(|open| (format!("{}.trigger", ["pr_opened", "pr_merged", "estimate"][open]), 2));
        self.shot = Some(shot);
        cx.notify();
    }

    /// The bench's press once its trigger is laid out; the headless clock
    /// stands still, so the popup's enter fade is run out by hand.
    #[cfg(test)]
    fn press_shot(&mut self, cx: &mut gpui::Context<Self>) {
        match self.shot_press.take() {
            Some((id, 0)) => self.view.update(cx, |view, cx| {
                let index = view.index_of(&id).unwrap_or_else(|| panic!("no node {id}"));
                view.fire(index, "press", None, cx);
            }),
            Some((id, wait)) => self.shot_press = Some((id, wait - 1)),
            None => {
                if !self.view.read(cx).layers().is_empty() {
                    self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
                }
                return;
            }
        }
        cx.notify();
    }

    /// The pane's team (the bench's, under test).
    fn team(&self, cx: &App) -> Option<domain::rows::Team> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return Some(shot.team.clone());
        }
        super::active_team(cx, &self.nav)
    }

    fn owner(&self, team_id: &str, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.owner;
        }
        is_owner(cx, team_id)
    }

    fn scoped_statuses(&self, cx: &App) -> Vec<(IssueStatusRow, ResolvedStatus)> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            let sorted = domain::statuses::sort_team_statuses(&shot.statuses);
            return (0..sorted.len()).map(|index| (sorted[index].clone(), resolve_row(&sorted, index))).collect();
        }
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return Vec::new();
        };
        let sorted = crate::queries::team_statuses(cx, &team_id);
        (0..sorted.len()).map(|index| (sorted[index].clone(), resolve_row(&sorted, index))).collect()
    }

    /// What a PR picker offers: every synced status but Duplicate (it needs
    /// a canonical issue). Each is a real row, so a pick always carries its
    /// uuid and the constructed `builtin:` vocabulary can't leak into a write.
    fn candidates(statuses: &[(IssueStatusRow, ResolvedStatus)]) -> Vec<ResolvedStatus> {
        statuses.iter().filter(|(_, resolved)| resolved.category != IssueStatusCategory::Duplicate && resolved.row_id.is_some()).map(|(_, resolved)| resolved.clone()).collect()
    }

    /// A document action: a picked scale or PR target.
    fn action(&mut self, name: &str, value: String, cx: &mut gpui::Context<Self>) {
        let Some(team) = self.team(cx) else { return };
        match name {
            "estimate" => {
                if let Some((wire, _, _)) = ESTIMATION_OPTIONS.iter().find(|(wire, _, _)| *wire == value) {
                    set_estimation(cx.weak_entity(), team.id, wire, cx);
                }
            }
            "prOpened" | "prMerged" => {
                let event = if name == "prOpened" { api::statuses::PrAutomationEvent::Opened } else { api::statuses::PrAutomationEvent::Merged };
                let candidates = Self::candidates(&self.scoped_statuses(cx));
                // The sentinel has no row: that is what makes it the "off" target.
                let target = match candidates.iter().find(|status| status.group_key == value).and_then(|status| status.row_id.clone()) {
                    Some(row_id) => api::statuses::PrAutomationTarget::Status(row_id),
                    None => api::statuses::PrAutomationTarget::DoNothing,
                };
                set_pr_automation(cx.weak_entity(), team.id, event, target, cx);
            }
            _ => {}
        }
    }

    /// FEED-79: a failed estimate-scale write is the error toast.
    fn set_estimation_error<T>(&mut self, result: Result<T, api::ApiError>, cx: &mut gpui::Context<Self>) {
        if let Err(err) = result {
            log::warn!("[ui] issues estimate scale write failed: {err}");
            crate::action_error::failed_action_err("update", "the estimate scale", &err, cx);
        }
    }

    /// The PR-automation card's failure writer (EXP-774): one toast for its
    /// writes (FEED-79).
    fn set_pr_automation_error<T>(&mut self, result: Result<T, api::ApiError>, cx: &mut gpui::Context<Self>) {
        if let Err(err) = result {
            log::warn!("[ui] statuses PR automation write failed: {err}");
            crate::action_error::failed_action_err("update", "PR automation", &err, cx);
        }
    }

    /// The state as the document binds it.
    fn project(&self, cx: &App) -> Value {
        let Some(team) = self.team(cx) else {
            return json!({"ready": false, "editable": false, "reader": false, "hasPr": false, "estimate": {"value": "", "label": "", "options": []}, "prOpened": {"value": "", "options": []}, "prMerged": {"value": "", "options": []}});
        };
        let owner = self.owner(&team.id, cx);
        let current = team.estimation().to_string();
        let estimate_options: Vec<Value> = ESTIMATION_OPTIONS
            .iter()
            .map(|(wire, label, hint)| match hint.is_empty() {
                true => json!({"label": label, "value": wire}),
                false => json!({"label": label, "value": wire, "trailing": hint}),
            })
            .collect();
        let statuses = self.scoped_statuses(cx);
        let rows: Vec<IssueStatusRow> = statuses.iter().map(|(row, _)| row.clone()).collect();
        let candidates = Self::candidates(&statuses);
        let mut options: Vec<Value> = candidates.iter().map(|status| json!({"label": status.name, "value": status.group_key, "icon": status.glyph.file_name(), "iconColor": hex(crate::icons::status_tint_color(&status.tint, cx))})).collect();
        // Web's `ban`, muted: the automation is off.
        options.push(json!({"label": "Do nothing", "value": PR_TARGET_NONE, "icon": "automation-off", "iconColor": hex(cx.theme().muted_foreground)}));
        let target = |status_id: Option<&str>, automation: Option<bool>, default_builtin: &str| {
            let value = resolve_pr_target(&rows, status_id, automation, default_builtin).map_or_else(|| PR_TARGET_NONE.to_string(), |status| status.group_key);
            json!({"value": value, "options": options})
        };
        json!({
            "ready": true,
            "editable": owner,
            "reader": !owner,
            // Web: no statuses to offer, no card.
            "hasPr": !candidates.is_empty(),
            "estimate": {"value": current, "label": estimation_label(&current), "options": estimate_options},
            "prOpened": target(team.pr_opened_status_id.as_deref(), team.pr_opened_automation, "in_review"),
            "prMerged": target(team.pr_merged_status_id.as_deref(), team.pr_merged_automation, "done"),
        })
    }
}

impl Render for IssuesPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "IssuesPane").entered();
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        #[cfg(test)]
        self.press_shot(cx);
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// A picker: the `button` Select at web's `w-44`, its menu end-aligned.
fn picker(id: &str, label: &str, data: &str, action: &str) -> Value {
    json!({"id": id, "component": "Select", "style": {"width": PICKER_WIDTH, "flexShrink": 0},
        "props": {"label": label, "name": id, "triggerVariant": "button", "align": "end", "value": path(&format!("/pane/{data}/value")), "options": path(&format!("/pane/{data}/options"))},
        "on": {"change": {"event": {"name": action}}}})
}

/// A row's leading label, taking the room the picker leaves.
fn row_label(ink: &Ink, id: &str, label: &str) -> Value {
    let mut text = ink.text(id, json!(label), SM, &ink.foreground);
    text["style"]["flexGrow"] = json!(1);
    text["style"]["minWidth"] = json!(0);
    text
}

/// A card: its band over one glass group (`settings::section`: `gap_2`).
fn card(ink: &Ink, id: &str, title: &str, visible: Value, rows: Vec<Value>) -> Value {
    json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [ink.band(&format!("{id}_band"), title, None), ink.group(&format!("{id}_group"), rows)]})
}

/// The pane's tree (`gap_6`): Estimates (the scale's picker, or its text for
/// a member), then PR automation's two pickers.
fn document(ink: &Ink) -> Value {
    let mut estimate = picker("estimate", "Estimate scale", "estimate", "estimate");
    estimate["visible"] = path("/pane/editable");
    let mut reader = ink.text("estimate_text", path("/pane/estimate/label"), SM, &ink.muted);
    reader["visible"] = path("/pane/reader");
    reader["style"]["flexShrink"] = json!(0);
    let scale = ink.shell("scale", false, json!(true), vec![row_label(ink, "scale_label", "Estimate scale"), estimate, reader]);
    let opened = ink.shell("opened", false, json!(true), vec![row_label(ink, "opened_label", "When a PR opens, move issues to"), picker("pr_opened", "When a PR opens", "prOpened", "prOpened")]);
    let merged = ink.shell("merged", false, json!(true), vec![row_label(ink, "merged_label", "When a PR merges, move issues to"), picker("pr_merged", "When a PR merges", "prMerged", "prMerged")]);
    let estimates = card(ink, "estimates", "Estimates", path("/pane/ready"), vec![scale]);
    let automation = card(ink, "automation", "PR automation", path("/pane/hasPr"), vec![opened, ink.divider("merged_divider", json!(true)), merged]);
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.5), "width": "100%", "minWidth": 0}, "children": [estimates, automation]})
}

/// One estimate-scale write (`teams.update`), awaited off the foreground
/// thread; the Electric echo re-renders the trigger.
fn set_estimation(pane: gpui::WeakEntity<IssuesPane>, team_id: String, scale: &'static str, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                let mut input = api::teams::TeamsUpdateInput::new(team_id);
                input.estimation_type = Some(scale.to_string());
                api::teams::teams_update(&trpc, &input)
            })
            .await;
        let _ = pane.update(cx, |this, cx| {
            this.set_estimation_error(result, cx);
        });
    })
    .detach();
}

/// One PR-automation write (status pick / "Do nothing"), awaited off the
/// foreground thread; the Electric echo re-renders the trigger.
fn set_pr_automation(pane: gpui::WeakEntity<IssuesPane>, team_id: String, event: api::statuses::PrAutomationEvent, target: api::statuses::PrAutomationTarget, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { api::statuses::statuses_set_pr_automation(&trpc, &team_id, event, &target) }).await;
        let _ = pane.update(cx, |this, cx| {
            this.set_pr_automation_error(result, cx);
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_read_only_text_is_the_label_alone() {
        assert_eq!(estimation_label(domain::contract::ISSUE_ESTIMATION_TSHIRT), "T-shirt");
        assert_eq!(estimation_label("unknown"), "Not in use");
    }

    /// #EXP-1629: the document raises no issue a debug build's mount check
    /// would refuse (only the IDE's literal chrome and the bound values).
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
        let tree = document(&ink);
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-issues", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }
}
