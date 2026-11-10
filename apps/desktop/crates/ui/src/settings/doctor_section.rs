//! THIS device's readiness block (EXP-1196) — rendered by Settings → Tools
//! AND the first-run devices step, so the two can never drift.
//!
//! It is [`crate::device_readiness`] fed live from the hub's doctor run
//! (`CodingHub::doctor.device`, rebuilt on every run): Required / Coding
//! agents / Computer use bands, one row per item, every action offered (this
//! IS the device). The one host-only extra: an agent's install row carries a
//! "Custom path" pill that reveals the CLI path field + Save path (closed by
//! default), saved straight through [`CodingHub::save_settings`], which
//! re-runs the doctor — the "did my path fix it?" loop.
//!
//! Saving a path overlays ONLY that one field onto the hub's LIVE settings.
//! For the Agents pane this is an external owned-field change — its `resync`
//! rewrites its inputs from the hub, at worst refreshing away unsaved sibling
//! edits there, the same as any external save today.
//!
//! #EXP-1770: the block is the shared readiness document
//! ([`super::readiness`]); [`LocalDoctor`] is its state here, [`document`]
//! its tree with Check again under it. Settings → Tools puts both in its own
//! document; [`DoctorPanel`] mounts them for the first-run step.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use eui_gpui::{binding::Binding, view::SurfaceView};
use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement as _, Render, Styled as _, Subscription, Window};
use serde_json::{json, Value};

use coding::device_doctor::{DoctorAction, DoctorState};
use coding::CodingAgent;

use crate::coding_flow::CodingHub;
use crate::device_readiness;

use super::doc::{path, IdeToasts, Ink, PillPaint};
use super::readiness::{self, Event, Facts, Paint, PathField, Shape};

/// THIS device's block: the Custom path fields' text and which are open.
#[derive(Default)]
pub(crate) struct LocalDoctor {
    /// The paths as typed.
    typed: HashMap<CodingAgent, String>,
    /// The paths the fields were last given (their bound values).
    shown: HashMap<CodingAgent, String>,
    /// The hub paths the fields were last synced from — external-change
    /// detection only (each field saves itself; there is no pane-wide Save).
    synced_paths: Option<(String, String)>,
    /// The agents whose Custom path field is open (closed by default).
    path_open: HashSet<CodingAgent>,
    /// Draw the block's own "Check again" pill (the first-run step pins it
    /// in its footer instead).
    recheck: bool,
}

impl LocalDoctor {
    pub(crate) fn new(cx: &App) -> Self {
        let mut this = Self { recheck: true, ..Self::default() };
        this.resync(cx);
        this
    }

    /// Mirror the hub's agent paths into the fields whenever they change out
    /// from under us (an Agents-pane save, another window).
    pub(crate) fn resync(&mut self, cx: &App) {
        let settings = CodingHub::global_ref(cx).map(|hub| hub.read(cx).settings.clone()).unwrap_or_default();
        let paths = (settings.claude_path.clone(), settings.codex_path.clone());
        if self.synced_paths.as_ref() == Some(&paths) {
            return;
        }
        for agent in CodingAgent::ALL {
            let value = settings.path_for(agent).to_string();
            self.typed.insert(agent, value.clone());
            self.shown.insert(agent, value);
        }
        self.synced_paths = Some(paths);
    }

    /// Opens `agent`'s Custom path field (the pictures).
    #[cfg(test)]
    pub(crate) fn open_path(&mut self, agent: CodingAgent) {
        self.path_open.insert(agent);
    }

    /// Persist ONE agent's path (blank degrades to the default program name,
    /// mirroring `Settings::load`) — overlaid onto the hub's LIVE settings so
    /// this can never roll back a sibling pane's save. The save re-runs the
    /// doctor.
    fn save_path(&mut self, agent: CodingAgent, cx: &mut App) {
        let raw = self.typed.get(&agent).map(|raw| raw.trim().to_string()).unwrap_or_default();
        let value = if raw.is_empty() {
            match agent {
                CodingAgent::Claude => coding::settings::DEFAULT_CLAUDE_PATH,
                CodingAgent::Codex => coding::settings::DEFAULT_CODEX_PATH,
            }
            .to_string()
        } else {
            raw
        };
        let hub = CodingHub::global(cx);
        let mut settings = hub.read(cx).settings.clone();
        match agent {
            CodingAgent::Claude => settings.claude_path = value,
            CodingAgent::Codex => settings.codex_path = value,
        }
        let _ = CodingHub::save_settings(&hub, settings, cx);
    }

    /// A press in the block. Every action runs here (this IS the device).
    pub(crate) fn handle(&mut self, event: Event, window: &mut Window, cx: &mut App) {
        match event {
            Event::Action { key, action } => device_readiness::run_local_action(&key, action, window, cx),
            Event::Toggle(on) => device_readiness::set_local_computer_use(on, cx),
            Event::CustomPath(key) => {
                if let Some(agent) = device_readiness::agent_for(&key) {
                    if !self.path_open.remove(&agent) {
                        self.path_open.insert(agent);
                    }
                }
            }
            Event::PathEdit { key, value } => {
                if let Some(agent) = device_readiness::agent_for(&key) {
                    self.typed.insert(agent, value);
                }
            }
            Event::SavePath(key) => {
                if let Some(agent) = device_readiness::agent_for(&key) {
                    self.save_path(agent, cx);
                }
            }
            Event::Recheck => {
                let hub = CodingHub::global(cx);
                CodingHub::refresh_doctor(&hub, cx);
            }
        }
    }

    /// The state [`document`] binds.
    pub(crate) fn project(&self, cx: &App) -> Value {
        let (doctor, running) = CodingHub::global_ref(cx)
            .map(|hub| {
                let hub = hub.read(cx);
                // EXP-1654: a capture's pinned fixture case, else the live run.
                (device_readiness::dev_doctor_case().or_else(|| hub.doctor.device.clone()), hub.doctor.running)
            })
            .unwrap_or_default();
        let busy = device_readiness::local_busy(cx);
        let notes = HashMap::new();
        let mut paths = HashMap::new();
        for item in doctor.iter().flat_map(|doctor| doctor.items.iter()) {
            let Some(agent) = device_readiness::agent_for(&item.key) else { continue };
            if item.state != DoctorState::Ok && item.action == Some(DoctorAction::Install) {
                let placeholder = match agent {
                    CodingAgent::Claude => coding::settings::DEFAULT_CLAUDE_PATH,
                    CodingAgent::Codex => coding::settings::DEFAULT_CODEX_PATH,
                };
                paths.insert(item.key.clone(), PathField { open: self.path_open.contains(&agent), value: self.shown.get(&agent).cloned().unwrap_or_default(), placeholder: placeholder.to_string() });
            }
        }
        let facts = Facts { busy: &busy, toggle: true, notes: &notes, paths: &paths, tail: 0 };
        let mut state = match &doctor {
            None => readiness::project(Shape::Loading, &facts),
            Some(doctor) => readiness::project(Shape::Sections(&device_readiness::sections(doctor, true, None)), &facts),
        };
        state["recheck"] = json!(self.recheck);
        state["running"] = json!(running);
        state["idle"] = json!(!running);
        state
    }
}

/// THIS device's block (`section`: `gap_2`) with Check again under it, its
/// state at `at` ([`LocalDoctor::project`]).
pub(crate) fn document(ink: &Ink, paint: &Paint, id: &str, at: &str) -> Value {
    let block = readiness::block(ink, paint, &format!("{id}_block"), at, true);
    // "Check again": re-run the doctor (the block re-renders when it lands).
    let [live, dim] = readiness::pill(ink, &format!("{id}_recheck"), PillPaint::Glass, json!(device_readiness::RECHECK), path(&format!("{at}/idle")), path(&format!("{at}/running")), json!({"name": "readinessRecheck"}));
    let recheck = json!({"id": format!("{id}_recheck_row"), "component": "Box", "visible": path(&format!("{at}/recheck")), "style": {"display": "flex", "flexDirection": "row"}, "children": [live, dim]});
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.5)}, "children": [block, recheck]})
}

/// [`document`] on its own binding: the first-run devices step's block.
pub struct DoctorPanel {
    doctor: LocalDoctor,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

impl DoctorPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // Creating the hub also kicks the FIRST doctor run (§7.7 onboarding).
        let hub = CodingHub::global(cx);
        let subscriptions = vec![cx.observe(&hub, |this, _, cx| {
            this.doctor.resync(cx);
            cx.notify();
        })];
        let panel = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new("/doctor", Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in readiness::EVENTS {
                let panel = panel.clone();
                binding.on_action(name, move |_, action, cx| {
                    let event = readiness::event(name, &action.context, action.payload.as_ref().unwrap_or(&Value::Null))?;
                    let panel = panel.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = panel.update(cx, |panel, cx| {
                                panel.doctor.handle(event, window, cx);
                                cx.notify();
                            });
                        });
                    });
                    None
                });
            }
            binding
        });
        let tree = document(&Ink::of(cx), &Paint::of(cx), "doctor", "/doctor");
        let view = super::doc::mount_with("settings-doctor", tree, &binding, Rc::new(super::doc::ide_plugin()), false, window, cx);
        Self { doctor: LocalDoctor::new(cx), binding, view, _subscriptions: subscriptions }
    }

    /// The first-run step's spelling: no own "Check again" (its footer has it).
    pub fn without_recheck(mut self) -> Self {
        self.doctor.recheck = false;
        self
    }
}

/// "Check again" as the first-run step's footer pill: re-run the doctor
/// (the block re-renders when it lands).
pub(crate) fn recheck_button(id: &'static str, running: bool, cx: &App) -> impl IntoElement {
    use gpui_component::Disableable as _;
    crate::surface::glass_pill_button(id, crate::surface::PillSize::Sm, cx)
        .label(device_readiness::RECHECK)
        .loading(running)
        .disabled(running)
        .on_click(|_, _, cx| {
            let hub = CodingHub::global(cx);
            CodingHub::refresh_doctor(&hub, cx);
        })
}

/// The pictures' install row: Claude Code not installed (a pill, and the
/// Custom path pill beside it).
#[cfg(test)]
pub(crate) fn claude_not_installed(doctor: &mut coding::device_doctor::DeviceDoctor) {
    if let Some(item) = doctor.items.iter_mut().find(|item| item.key == "claude") {
        item.state = DoctorState::Missing;
        item.action = Some(DoctorAction::Install);
        item.detail = Some("Not installed".to_string());
    }
}

impl Render for DoctorPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "DoctorPanel").entered();
        let state = self.doctor.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|current| *current = state, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EXP-1629: the document raises no issue a debug build's mount check
    /// refuses (#EXP-1767's class).
    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let tree = document(&super::super::readiness::tests::test_ink(), &super::super::readiness::tests::test_paint(), "doctor", "/doctor");
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-doctor", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }
}
