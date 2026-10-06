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

use std::collections::HashSet;

use gpui::{
    App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{h_flex, input::InputState, v_flex, Disableable as _};

use coding::device_doctor::{DoctorAction, DoctorState};
use coding::CodingAgent;

use crate::coding_flow::CodingHub;
use crate::controls::{glass_input, WebControl as _};
use crate::device_readiness::{self, RowExtras};
use crate::surface::{glass_pill, glass_pill_button, PillMode, PillSize};

use super::section;

pub struct DoctorPanel {
    claude_input: Entity<InputState>,
    codex_input: Entity<InputState>,
    /// The hub paths the inputs were last synced from — external-change
    /// detection only (each input saves itself; there is no pane-wide Save).
    synced_paths: Option<(String, String)>,
    /// The agents whose Custom path field is open (closed by default).
    path_open: HashSet<CodingAgent>,
    /// Draw the block's own "Check again" pill (the first-run step pins it
    /// in its footer instead).
    recheck: bool,
    _subscriptions: Vec<Subscription>,
}

impl DoctorPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let claude_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CLAUDE_PATH));
        let codex_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CODEX_PATH));

        // Creating the hub also kicks the FIRST doctor run (§7.7 onboarding).
        let hub = CodingHub::global(cx);
        let subscriptions = vec![cx.observe_in(&hub, window, |this, _, window, cx| {
            this.resync(window, cx);
            cx.notify();
        })];

        let mut this = Self {
            claude_input,
            codex_input,
            synced_paths: None,
            path_open: HashSet::new(),
            recheck: true,
            _subscriptions: subscriptions,
        };
        this.resync(window, cx);
        this
    }

    /// The first-run step's spelling: no own "Check again" (its footer has it).
    pub fn without_recheck(mut self) -> Self {
        self.recheck = false;
        self
    }

    fn input_for(&self, agent: CodingAgent) -> &Entity<InputState> {
        match agent {
            CodingAgent::Claude => &self.claude_input,
            CodingAgent::Codex => &self.codex_input,
        }
    }

    /// Mirror the hub's agent paths into the inputs whenever they change out
    /// from under us (an Agents-pane save, another window).
    fn resync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();
        let paths = (settings.claude_path.clone(), settings.codex_path.clone());
        if self.synced_paths.as_ref() == Some(&paths) {
            return;
        }
        for agent in CodingAgent::ALL {
            let input = self.input_for(agent).clone();
            let value = settings.path_for(agent).to_string();
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        self.synced_paths = Some(paths);
        cx.notify();
    }

    /// Persist ONE agent's path (blank degrades to the default program name,
    /// mirroring `Settings::load`) — overlaid onto the hub's LIVE settings so
    /// this can never roll back a sibling pane's save. The save re-runs the
    /// doctor.
    fn save_path(&mut self, agent: CodingAgent, cx: &mut gpui::Context<Self>) {
        let raw = self.input_for(agent).read(cx).value().trim().to_string();
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
        cx.notify();
    }

    /// The install row's extras: the Custom path pill, and (open) the path
    /// field + Save path under the row.
    fn path_extras(
        &self,
        agent: CodingAgent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> RowExtras {
        let open = self.path_open.contains(&agent);
        let toggle = glass_pill(
            SharedString::from(format!("doctor-custom-path-{}", agent.id())),
            PillSize::Sm,
            PillMode::Select { selected: open },
            cx,
        )
        .child(device_readiness::CUSTOM_PATH)
        .on_click(cx.listener(move |this, _, _, cx| {
            if !this.path_open.remove(&agent) {
                this.path_open.insert(agent);
            }
            cx.notify();
        }));
        let below = open.then(|| {
            h_flex()
                .w_full()
                .gap_2()
                .items_center()
                .child(
                    gpui::div()
                        .flex_1()
                        .min_w_0()
                        .child(glass_input(self.input_for(agent), window, cx).web_input_sm()),
                )
                .child(
                    glass_pill_button(
                        SharedString::from(format!("doctor-save-path-{}", agent.id())),
                        PillSize::Sm,
                        cx,
                    )
                    .label(device_readiness::SAVE_PATH)
                    .on_click(cx.listener(move |this, _, _, cx| this.save_path(agent, cx))),
                )
                .into_any_element()
        });
        RowExtras {
            trailing: Some(toggle.into_any_element()),
            below,
        }
    }
}

/// "Check again": re-run the doctor (the block re-renders when it lands).
pub(crate) fn recheck_button(id: &'static str, running: bool, cx: &App) -> impl IntoElement {
    glass_pill_button(id, PillSize::Sm, cx)
        .label(device_readiness::RECHECK)
        .loading(running)
        .disabled(running)
        .on_click(|_, _, cx| {
            let hub = CodingHub::global(cx);
            CodingHub::refresh_doctor(&hub, cx);
        })
}

impl Render for DoctorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let hub = CodingHub::global(cx);
        let (doctor, running) = {
            let hub = hub.read(cx);
            (hub.doctor.device.clone(), hub.doctor.running)
        };
        let block = match &doctor {
            None => device_readiness::render_loading(),
            Some(doctor) => {
                let mut props = device_readiness::local_props("doctor", cx);
                for item in &doctor.items {
                    let Some(agent) = device_readiness::agent_for(&item.key) else {
                        continue;
                    };
                    if item.state != DoctorState::Ok && item.action == Some(DoctorAction::Install) {
                        let extras = self.path_extras(agent, window, cx);
                        props.extras.insert(item.key.clone(), extras);
                    }
                }
                device_readiness::render_sections(
                    &device_readiness::sections(doctor, true, None),
                    props,
                    cx,
                )
            }
        };
        let body = section(cx).child(v_flex().w_full().min_w_0().child(block));
        if self.recheck {
            body.child(h_flex().child(recheck_button("doctor-check", running, cx)))
        } else {
            body
        }
    }
}
