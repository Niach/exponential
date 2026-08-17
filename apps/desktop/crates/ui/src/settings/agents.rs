//! Settings → Agents (EXP-288 — split out of the old Coding pane; the
//! remaining this-device knobs live in [`super::tools`]).
//!
//! The JetBrains-SDK-settings-style pane for the Start-coding launcher,
//! grouped HARD by agent (EXP-206):
//!
//! | Card   | Contents                                                     |
//! |--------|--------------------------------------------------------------|
//! | Agents | Default agent, then one TAB per agent: CLI path + model +    |
//! |        | effort, plus the agent's own toggles — Claude: plan mode,    |
//! |        | ultracode, skip permissions; Codex: skip permissions; pi:    |
//! |        | nothing (no permission system)                               |
//!
//! Model/effort are [`crate::coding_selects`] choice selects (never free
//! text — the closed alias sets the CLI accepts). The per-agent toggles are
//! what the shared Start-coding dialog prefills from — ONE set of defaults
//! for single-issue and batch runs alike (EXP-206).
//!
//! Settings persist through [`crate::coding_flow::CodingHub`] to the local
//! per-install `settings.json` — never synced. Saving re-runs the doctor
//! against the new agent paths, and the doctor's report is exactly what
//! gates the Start-coding button (§7.1 step 1 ANDs `agent.ok && git.ok`).
//! The "Tooling doctor" report itself renders in Settings → Tools
//! ([`super::doctor_section`], EXP-367 — one panel shared with the
//! onboarding tools step).
//!
//! This pane and the Tools pane share ONE settings struct but own DISJOINT
//! fields — see the [`super::tools`] module doc for the drafted/save/resync
//! contract that keeps their saves from clobbering each other.
//!
//! The personal API key is provisioned and rotated **fully automatically**
//! (`api::users::ensure_personal_key` on the first coding session; the
//! `.exp-mcp.json` writer picks it up), so there is no key UI here at all.

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    select::Select,
    switch::Switch,
    tab::{Tab, TabBar, TabVariant},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size,
};

use coding::{CodingAgent, Settings};

use crate::coding_flow::CodingHub;
use crate::controls::WebControl as _;
use crate::coding_selects::{
    agent_icon, choice_select, effort_choices_for, model_choices_for, selected, ChoiceSelect,
    AGENT_CHOICES,
};

use super::{card_title, error_notice, section};

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

pub struct AgentsPane {
    /// The default agent the Start-coding dialog preselects (EXP-201).
    agent_select: ChoiceSelect,
    claude_input: Entity<InputState>,
    model_select: ChoiceSelect,
    effort_select: ChoiceSelect,
    codex_input: Entity<InputState>,
    codex_model_select: ChoiceSelect,
    codex_effort_select: ChoiceSelect,
    pi_input: Entity<InputState>,
    pi_model_select: ChoiceSelect,
    pi_thinking_select: ChoiceSelect,
    /// Which agent tab of the Agents card is showing — pure UI state, not
    /// persisted (EXP-206).
    agent_tab: CodingAgent,
    /// Per-agent run defaults (the shared dialog's prefill — EXP-206: one
    /// set for issue and batch runs alike): Claude plan mode ON out of the
    /// box, everything else OFF.
    claude_ultracode: bool,
    claude_plan_mode: bool,
    pi_plan_mode: bool,
    claude_skip_permissions: bool,
    codex_skip_permissions: bool,
    /// The hub settings the controls were last synced from (dirty baseline).
    synced: Option<Settings>,
    save_error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl AgentsPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let claude_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CLAUDE_PATH));
        let codex_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CODEX_PATH));
        let pi_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_PI_PATH));
        let defaults = Settings::default();
        let agent_select =
            choice_select(&AGENT_CHOICES, defaults.default_agent.id(), window, cx);
        let model_select = choice_select(
            model_choices_for(CodingAgent::Claude),
            &defaults.claude_model,
            window,
            cx,
        );
        let effort_select = choice_select(
            effort_choices_for(CodingAgent::Claude),
            &defaults.claude_effort,
            window,
            cx,
        );
        let codex_model_select = choice_select(
            model_choices_for(CodingAgent::Codex),
            &defaults.codex_model,
            window,
            cx,
        );
        let codex_effort_select = choice_select(
            effort_choices_for(CodingAgent::Codex),
            &defaults.codex_effort,
            window,
            cx,
        );
        let pi_model_select = choice_select(
            model_choices_for(CodingAgent::Pi),
            &defaults.pi_model,
            window,
            cx,
        );
        let pi_thinking_select = choice_select(
            effort_choices_for(CodingAgent::Pi),
            &defaults.pi_thinking,
            window,
            cx,
        );

        // Creating the hub also kicks the FIRST doctor run (§7.7 onboarding).
        let hub = CodingHub::global(cx);
        let mut subscriptions = vec![
            // Doctor results / external settings changes re-render + resync.
            cx.observe_in(&hub, window, |this, _, window, cx| {
                this.resync(window, cx);
                cx.notify();
            }),
        ];
        for input in [&claude_input, &codex_input, &pi_input] {
            subscriptions.push(cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify(); // live dirty tracking on the Save button
                }
            }));
        }
        for select in [
            &agent_select,
            &model_select,
            &effort_select,
            &codex_model_select,
            &codex_effort_select,
            &pi_model_select,
            &pi_thinking_select,
        ] {
            // Confirming a choice notifies the state — observing keeps the
            // Save button's dirty tracking live without a typed subscription.
            subscriptions.push(cx.observe(select, |_, _, cx| cx.notify()));
        }

        let mut this = Self {
            agent_select,
            claude_input,
            model_select,
            effort_select,
            codex_input,
            codex_model_select,
            codex_effort_select,
            pi_input,
            pi_model_select,
            pi_thinking_select,
            agent_tab: defaults.default_agent,
            claude_ultracode: defaults.claude_ultracode,
            claude_plan_mode: defaults.claude_plan_mode,
            pi_plan_mode: defaults.pi_plan_mode,
            claude_skip_permissions: defaults.claude_skip_permissions,
            codex_skip_permissions: defaults.codex_skip_permissions,
            synced: None,
            save_error: None,
            _subscriptions: subscriptions,
        };
        this.resync(window, cx);
        this
    }

    /// Overlay ONLY this pane's owned fields from `from` onto `onto` —
    /// the single definition `resync`/`save` both lean on so the two can
    /// never drift.
    fn overlay_owned(onto: &mut Settings, from: &Settings) {
        onto.default_agent = from.default_agent;
        onto.claude_path = from.claude_path.clone();
        onto.codex_path = from.codex_path.clone();
        onto.pi_path = from.pi_path.clone();
        onto.claude_model = from.claude_model.clone();
        onto.claude_effort = from.claude_effort.clone();
        onto.codex_model = from.codex_model.clone();
        onto.codex_effort = from.codex_effort.clone();
        onto.pi_model = from.pi_model.clone();
        onto.pi_thinking = from.pi_thinking.clone();
        onto.claude_ultracode = from.claude_ultracode;
        onto.claude_plan_mode = from.claude_plan_mode;
        onto.pi_plan_mode = from.pi_plan_mode;
        onto.claude_skip_permissions = from.claude_skip_permissions;
        onto.codex_skip_permissions = from.codex_skip_permissions;
    }

    /// Mirror the hub's settings into the controls whenever they change out
    /// from under us. Unowned fields (Tools' repos/prefix/shell, the rail
    /// pref) are adopted into the baseline FIRST, so a sibling pane's save
    /// never wipes edits here; only owned-field changes rewrite the controls.
    fn resync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();
        if let Some(synced) = self.synced.as_mut() {
            let mut adopted = settings.clone();
            Self::overlay_owned(&mut adopted, synced);
            *synced = adopted;
        }
        if self.synced.as_ref() == Some(&settings) {
            return;
        }
        self.claude_input.update(cx, |input, cx| {
            input.set_value(settings.claude_path.clone(), window, cx)
        });
        self.codex_input.update(cx, |input, cx| {
            input.set_value(settings.codex_path.clone(), window, cx)
        });
        self.pi_input.update(cx, |input, cx| {
            input.set_value(settings.pi_path.clone(), window, cx)
        });
        // The persisted values are load-normalized into the choice sets, so
        // every set_selected_value below finds its row.
        self.agent_select.update(cx, |select, cx| {
            select.set_selected_value(
                &SharedString::from(settings.default_agent.id()),
                window,
                cx,
            )
        });
        for (select, value) in [
            (&self.model_select, settings.claude_model.clone()),
            (&self.effort_select, settings.claude_effort.clone()),
            (&self.codex_model_select, settings.codex_model.clone()),
            (&self.codex_effort_select, settings.codex_effort.clone()),
            (&self.pi_model_select, settings.pi_model.clone()),
            (&self.pi_thinking_select, settings.pi_thinking.clone()),
        ] {
            select.update(cx, |select, cx| {
                select.set_selected_value(&SharedString::from(value), window, cx)
            });
        }
        self.claude_ultracode = settings.claude_ultracode;
        self.claude_plan_mode = settings.claude_plan_mode;
        self.pi_plan_mode = settings.pi_plan_mode;
        self.claude_skip_permissions = settings.claude_skip_permissions;
        self.codex_skip_permissions = settings.codex_skip_permissions;
        // Open the Agents card on the saved default agent (first sync only —
        // later external saves must not yank the tab from under the user).
        if self.synced.is_none() {
            self.agent_tab = settings.default_agent;
        }
        self.synced = Some(settings);
        cx.notify();
    }

    /// The settings the controls currently describe: the synced baseline with
    /// ONLY this pane's fields overlaid. Blank CLI paths degrade to the §7.7
    /// defaults — a hand-blanked pane can never produce an unusable launcher
    /// (mirrors `Settings::load`); the selects are closed sets by
    /// construction.
    fn drafted(&self, cx: &App) -> Settings {
        let defaults = Settings::default();
        let value = |input: &Entity<InputState>, default: &str| {
            let raw = input.read(cx).value().trim().to_string();
            if raw.is_empty() {
                default.to_string()
            } else {
                raw
            }
        };
        let mut drafted = self.synced.clone().unwrap_or_default();
        let owned = Settings {
            default_agent: CodingAgent::parse(&selected(&self.agent_select, cx))
                .unwrap_or_default(),
            claude_path: value(&self.claude_input, &defaults.claude_path),
            codex_path: value(&self.codex_input, &defaults.codex_path),
            pi_path: value(&self.pi_input, &defaults.pi_path),
            claude_model: selected(&self.model_select, cx),
            claude_effort: selected(&self.effort_select, cx),
            codex_model: selected(&self.codex_model_select, cx),
            codex_effort: selected(&self.codex_effort_select, cx),
            pi_model: selected(&self.pi_model_select, cx),
            pi_thinking: selected(&self.pi_thinking_select, cx),
            claude_ultracode: self.claude_ultracode,
            claude_plan_mode: self.claude_plan_mode,
            pi_plan_mode: self.pi_plan_mode,
            claude_skip_permissions: self.claude_skip_permissions,
            codex_skip_permissions: self.codex_skip_permissions,
            ..defaults
        };
        Self::overlay_owned(&mut drafted, &owned);
        drafted
    }

    fn dirty(&self, cx: &App) -> bool {
        self.synced
            .as_ref()
            .map(|synced| *synced != self.drafted(cx))
            .unwrap_or(false)
    }

    fn save(&mut self, cx: &mut gpui::Context<Self>) {
        let drafted = self.drafted(cx);
        let hub = CodingHub::global(cx);
        // Overlay ONLY the owned fields onto the hub's LIVE settings, so a
        // save here can never roll back a concurrent Tools-pane save (or the
        // rail pref).
        let mut settings = hub.read(cx).settings.clone();
        Self::overlay_owned(&mut settings, &drafted);
        self.save_error = CodingHub::save_settings(&hub, settings.clone(), cx)
            .err()
            .map(SharedString::from);
        // `synced` follows the hub via the observer's resync; setting it here
        // too keeps the Save button honest when the observer coalesces.
        self.synced = Some(settings);
        cx.notify();
    }

    // -- render pieces --------------------------------------------------------

    fn labeled_input(
        label: &'static str,
        input: &Entity<InputState>,
        cx: &App,
    ) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(label))
            .child(Input::new(input).web_input_sm())
    }

    /// A labeled [`ChoiceSelect`] row (the select analog of `labeled_input`).
    /// No hint paragraph — EXP-490 dropped the per-option subtitle notes.
    fn labeled_select(
        label: &'static str,
        select: &ChoiceSelect,
        cx: &App,
    ) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(label))
            .child(Select::new(select).web_input_sm())
    }

    /// One toggle row: a single-line label with the `Switch` on the right —
    /// no hint text (EXP-206: long descriptions pushed the switches out of
    /// the card).
    fn toggle_row(
        id: &'static str,
        label: &'static str,
        checked: bool,
        on_click: impl Fn(&mut Self, &bool, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(div().text_sm().child(label))
            .child(
                Switch::new(id)
                    .checked(checked)
                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                        on_click(this, checked, cx);
                        cx.notify();
                    })),
            )
    }

    /// The Agents card (EXP-206): one TAB per agent, each holding that
    /// agent's CLI path + model/effort selects and its OWN toggles — plan
    /// mode and ultracode exist only on the Claude tab, skip permissions on
    /// Claude and Codex.
    fn render_agents_section(&mut self, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let active_ix = CodingAgent::ALL
            .iter()
            .position(|agent| *agent == self.agent_tab)
            .unwrap_or(0);
        // Centered pill tabs with each agent's brand mark + name (EXP-206).
        // Icon and text ride ONE custom child: `Tab::icon` alone renders
        // icon-ONLY (it drops the label).
        let tabs = h_flex().w_full().justify_center().child(
            TabBar::new("coding-agent-tabs")
                .with_variant(TabVariant::Pill)
                .with_size(Size::Small)
                .selected_index(active_ix)
                .on_click(cx.listener(|this, ix: &usize, _, cx| {
                    if let Some(agent) = CodingAgent::ALL.get(*ix).copied() {
                        this.agent_tab = agent;
                        cx.notify();
                    }
                }))
                .children(CodingAgent::ALL.iter().map(|agent| {
                    Tab::new().child(
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .child(Icon::from(agent_icon(*agent)).size_3p5())
                            .child(SharedString::from(agent.label())),
                    )
                })),
        );

        let mut body = section(cx)
            .child(card_title("Agents"))
            .child(Self::labeled_select("Default agent", &self.agent_select, cx))
            .child(tabs);
        body = match self.agent_tab {
            CodingAgent::Claude => body
                .child(Self::labeled_input("CLI path", &self.claude_input, cx))
                .child(Self::labeled_select("Model", &self.model_select, cx))
                .child(Self::labeled_select("Effort", &self.effort_select, cx))
                .child(Self::toggle_row(
                    "claude-plan-mode",
                    "Plan mode",
                    self.claude_plan_mode,
                    |this, checked, _| this.claude_plan_mode = *checked,
                    cx,
                ))
                .child(Self::toggle_row(
                    "claude-ultracode",
                    "Dynamic workflows (ultracode)",
                    self.claude_ultracode,
                    |this, checked, _| this.claude_ultracode = *checked,
                    cx,
                ))
                .child(Self::toggle_row(
                    "claude-skip-permissions",
                    "Skip permissions",
                    self.claude_skip_permissions,
                    |this, checked, _| this.claude_skip_permissions = *checked,
                    cx,
                )),
            CodingAgent::Codex => body
                .child(Self::labeled_input("CLI path", &self.codex_input, cx))
                .child(Self::labeled_select("Model", &self.codex_model_select, cx))
                .child(Self::labeled_select(
                    "Reasoning effort",
                    &self.codex_effort_select,
                    cx,
                ))
                .child(Self::toggle_row(
                    "codex-skip-permissions",
                    "Skip permissions",
                    self.codex_skip_permissions,
                    |this, checked, _| this.codex_skip_permissions = *checked,
                    cx,
                )),
            CodingAgent::Pi => body
                .child(Self::labeled_input("CLI path", &self.pi_input, cx))
                .child(Self::labeled_select("Model", &self.pi_model_select, cx))
                .child(Self::labeled_select(
                    "Thinking level",
                    &self.pi_thinking_select,
                    cx,
                ))
                .child(Self::toggle_row(
                    "pi-plan-mode",
                    "Plan mode",
                    self.pi_plan_mode,
                    |this, checked, _| this.pi_plan_mode = *checked,
                    cx,
                )),
        };
        body
    }
}

impl Render for AgentsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let dirty = self.dirty(cx);
        let agents_card = self.render_agents_section(cx);

        let mut save_area = v_flex().gap_2();
        if let Some(error) = &self.save_error {
            save_area = save_area.child(error_notice(error.clone(), cx));
        }
        save_area = save_area.child(
            h_flex().justify_end().child(
                Button::new("agents-save")
                    .primary()
                    .web_sm()
                    .label("Save changes")
                    .disabled(!dirty)
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            ),
        );

        // EXP-282: the sections are flat now — whitespace is the only
        // separator, so they sit further apart (the detail column's own
        // section rhythm).
        v_flex().w_full().gap_6().child(agents_card).child(save_area)
    }
}
