//! Settings → Agents (EXP-288 — split out of the old Coding pane; the
//! remaining this-device knobs live in [`super::tools`]).
//!
//! The JetBrains-SDK-settings-style pane for the Start-coding launcher,
//! grouped HARD by agent (EXP-206):
//!
//! | Card   | Contents                                                     |
//! |--------|--------------------------------------------------------------|
//! | Agents | Default agent, then one TAB per agent: CLI path + model +    |
//! |        | effort, the agent's own toggles — Claude: ultracode, plan    |
//! |        | mode; Codex: none; pi: plan mode (EXP-690: every run         |
//! |        | bypasses permissions, no toggle) — and (EXP-694) this        |
//! |        | machine's account + usage rows for that agent                |
//!
//! Model/effort are [`crate::coding_selects`] choice selects (never free
//! text — the closed alias sets the CLI accepts). The per-agent toggles are
//! what the shared Start-coding dialog prefills from — ONE set of defaults
//! for single-issue and batch runs alike (EXP-206).
//!
//! EXP-694: the pane wears the SHARED grouped agent picker
//! ([`crate::launch_options::AgentDefaultsGroup`]) — the exact component the
//! Device settings dialog and the Start-coding dialog render — and AUTOSAVES
//! like both of them: no Save button, pickers and switches write on change,
//! a typed CLI path after [`PATH_SAVE_DEBOUNCE`] (or on blur).
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

use std::time::Duration;

use gpui::{
    App, AppContext as _, Div, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Task, Window,
};
use gpui_component::{
    input::{InputEvent, InputState},
    select::Select,
    v_flex,
};

use coding::{CodingAgent, Settings};

use crate::coding_flow::CodingHub;
use crate::coding_selects::{
    agent_icon, choice_select, effort_choices_for, model_choices_for, selected, ChoiceSelect,
    AGENT_CHOICES,
};
use crate::device_settings::{agent_account_rows, login_affordance, own_agent_status};
use crate::launch_options::{AgentDefaultsGroup, AgentPill, DefaultsToggle};
use crate::surface;
use crate::controls::glass_input;

use super::{card_title, error_notice, section};

/// EXP-694: the pane AUTOSAVES like the Device settings dialog — no Save
/// button. A typed CLI path settles for this long before it is written; a
/// blur (or Enter) commits it immediately, and every picker/switch writes
/// straight through on change.
const PATH_SAVE_DEBOUNCE: Duration = Duration::from_millis(800);

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
    /// The hub settings the controls were last synced from (the autosave
    /// baseline: a control rewrite the pane itself performed drafts back to
    /// it and writes nothing).
    synced: Option<Settings>,
    /// The pending debounced CLI-path write (dropping it cancels).
    path_save: Option<Task<()>>,
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
            // EXP-694 autosave: typing settles, a blur commits.
            subscriptions.push(cx.subscribe(input, |this: &mut Self, _, event: &InputEvent, cx| {
                match event {
                    InputEvent::Change => this.queue_path_save(cx),
                    InputEvent::Blur | InputEvent::PressEnter { .. } => {
                        this.path_save = None;
                        this.save(cx);
                    }
                    InputEvent::Focus => {}
                }
                cx.notify();
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
            // EXP-694 autosave: confirming a choice IS the save (the baseline
            // guard in `save` swallows the pane's own rewrites).
            subscriptions.push(cx.observe(select, |this: &mut Self, _, cx| {
                this.save(cx);
                cx.notify();
            }));
        }
        // Leaving Settings drops the pane — pay out what the path debounce
        // still owes on the way out (the web/iOS unmount-flush twin).
        cx.on_release(|this, cx| this.flush_pending_path(cx)).detach();

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
            synced: None,
            path_save: None,
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

    /// EXP-694: hold a typed CLI path until the typing settles
    /// ([`PATH_SAVE_DEBOUNCE`]); a newer keystroke replaces (and so cancels)
    /// the pending task.
    fn queue_path_save(&mut self, cx: &mut gpui::Context<Self>) {
        self.path_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PATH_SAVE_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.save(cx));
        }));
    }

    /// EXP-694: the autosave — every control writes through the hub the
    /// moment it changes, so the pane has no Save button. The baseline guard
    /// is what makes that safe: a rewrite `resync` performed drafts back to
    /// `synced` and writes nothing.
    fn save(&mut self, cx: &mut gpui::Context<Self>) {
        self.save_error = self.commit(cx);
        cx.notify();
    }

    /// The hub write itself, without the pane's error/notify plumbing — shared
    /// with the on-release flush, which has no `Context` left to notify.
    /// Returns the failure, if any.
    fn commit(&mut self, cx: &mut App) -> Option<SharedString> {
        // A failed write stays retryable: the hub adopts the value in memory
        // even when the file write fails, so the baseline reads clean and the
        // standing error is what re-arms the next commit.
        if !self.dirty(cx) && self.save_error.is_none() {
            return None;
        }
        let drafted = self.drafted(cx);
        let hub = CodingHub::global(cx);
        // Overlay ONLY the owned fields onto the hub's LIVE settings, so a
        // save here can never roll back a concurrent Tools-pane save (or the
        // rail pref).
        let mut settings = hub.read(cx).settings.clone();
        Self::overlay_owned(&mut settings, &drafted);
        let error = CodingHub::save_settings(&hub, settings.clone(), cx)
            .err()
            .map(SharedString::from);
        // `synced` follows the hub via the observer's resync; setting it here
        // too keeps the baseline honest when the observer coalesces.
        self.synced = Some(settings);
        error
    }

    /// EXP-694: the debounce's other half — the pane is rebuilt per Settings
    /// navigation, so leaving it drops [`Self::path_save`] and a CLI path
    /// typed inside the 800ms would never be written. TAKING the task first
    /// keeps this single-shot: a blur/Enter commit already cleared it, so the
    /// flush can never double-write.
    fn flush_pending_path(&mut self, cx: &mut App) {
        if self.path_save.take().is_some() {
            self.save_error = self.commit(cx);
        }
    }

    // -- render pieces --------------------------------------------------------

    /// EXP-694: a [`ChoiceSelect`] as a grouped picker row — the label
    /// leading, the value trailing behind the select's own caret, no field
    /// chrome (the group IS the field).
    fn picker_row(label: &'static str, select: &ChoiceSelect, cx: &App) -> Div {
        surface::glass_picker_row(
            label,
            None,
            surface::glass_picker_select(Select::new(select)).into_any_element(),
            cx,
        )
    }

    /// The Agents card (EXP-206): one TAB per agent, each holding that
    /// agent's CLI path + model/effort selects and its OWN toggles — plan
    /// mode and ultracode exist only on the Claude tab (EXP-690 retired the
    /// skip-permissions toggle: every run bypasses).
    ///
    /// EXP-694: it is the SHARED [`AgentDefaultsGroup`] — byte-for-byte the
    /// component the Device settings dialog and the Start-coding cluster
    /// render, with this pane's CLI-path row spliced above Model and this
    /// machine's own account + usage rows under the toggles. The old centered
    /// `TabBar` pill strip and the title-above-control fields are gone.
    fn render_agents_section(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let agent_tab = self.agent_tab;
        let active_ix = CodingAgent::ALL
            .iter()
            .position(|agent| *agent == agent_tab)
            .unwrap_or(0);
        let pills: Vec<AgentPill> = CodingAgent::ALL
            .into_iter()
            .map(|agent| AgentPill {
                label: SharedString::from(agent.label()),
                icon: Some(agent_icon(agent)),
                dimmed: false,
                note: None,
            })
            .collect();

        // EXP-484/694: this machine's own account + usage for the open tab,
        // the same two rows the device dialog renders for a remote machine.
        let (accounts, usage) = own_agent_status(cx);
        let account = accounts.get(agent_tab.id());
        let signed_in = account.map(|account| account.signed_in).unwrap_or(false);
        let affordance = login_affordance(agent_tab, true, true, &[], signed_in);
        let account_rows = agent_account_rows(
            agent_tab,
            account,
            usage.get(agent_tab.id()),
            None,
            affordance,
            false,
            move |_: &mut Self, switch, cx| {
                crate::agent_login::open_login_tab(agent_tab, switch, cx)
            },
            cx,
        );

        let (path, model, effort) = match agent_tab {
            CodingAgent::Claude => (
                &self.claude_input,
                self.model_select.clone(),
                self.effort_select.clone(),
            ),
            CodingAgent::Codex => (
                &self.codex_input,
                self.codex_model_select.clone(),
                self.codex_effort_select.clone(),
            ),
            CodingAgent::Pi => (
                &self.pi_input,
                self.pi_model_select.clone(),
                self.pi_thinking_select.clone(),
            ),
        };
        let path_row = surface::glass_input_row(
            "CLI path",
            surface::glass_row_input(glass_input(path, window, cx)).into_any_element(),
            cx,
        );

        let mut group = AgentDefaultsGroup::new(
            "settings-agents",
            agent_tab,
            pills,
            Some(active_ix),
            move |this: &mut Self, ix, _window, cx| {
                if let Some(agent) = CodingAgent::ALL.get(ix).copied() {
                    this.agent_tab = agent;
                    cx.notify();
                }
            },
            model,
            effort,
        )
        .effort_disabled(agent_tab == CodingAgent::Claude && self.claude_ultracode)
        .leading(vec![path_row])
        .trailing(account_rows);
        if agent_tab == CodingAgent::Claude {
            group = group
                .toggle(DefaultsToggle::new(
                    "claude-ultracode",
                    "Ultracode",
                    self.claude_ultracode,
                    |this: &mut Self, on, cx| {
                        this.claude_ultracode = on;
                        this.save(cx);
                    },
                ))
                .toggle(DefaultsToggle::new(
                    "claude-plan-mode",
                    "Plan mode",
                    self.claude_plan_mode,
                    |this: &mut Self, on, cx| {
                        this.claude_plan_mode = on;
                        this.save(cx);
                    },
                ));
        }
        if agent_tab == CodingAgent::Pi {
            group = group.toggle(DefaultsToggle::new(
                "pi-plan-mode",
                "Plan mode",
                self.pi_plan_mode,
                |this: &mut Self, on, cx| {
                    this.pi_plan_mode = on;
                    this.save(cx);
                },
            ));
        }

        section(cx).child(card_title("Agents")).child(
            // 8px between the two groups (EXP-694's group rhythm).
            v_flex()
                .w_full()
                .gap_2()
                .child(surface::glass_group_rows(vec![Self::picker_row(
                    "Default agent",
                    &self.agent_select,
                    cx,
                )]))
                .child(group.render(cx)),
        )
    }
}

impl Render for AgentsPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let agents_card = self.render_agents_section(window, cx);

        // EXP-694: no Save button — every control autosaves, so the only
        // thing left below the card is a write that failed.
        let mut body = v_flex().w_full().gap_6().child(agents_card);
        if let Some(error) = &self.save_error {
            body = body.child(error_notice(error.clone(), cx));
        }
        body
    }
}
