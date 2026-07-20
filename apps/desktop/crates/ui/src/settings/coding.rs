//! Settings → Coding (masterplan-v3 §7.7 DC-3, §7.2).
//!
//! The JetBrains-SDK-settings-style pane for the Start-coding launcher,
//! grouped HARD by agent (EXP-206 — the old single flat card buried the
//! toggles, and the issue/batch run split is gone):
//!
//! | Card   | Contents                                                     |
//! |--------|--------------------------------------------------------------|
//! | Coding | Default agent, repos & worktrees root, branch prefix         |
//! | Agents | One TAB per agent: CLI path + model + effort, plus the       |
//! |        | agent's own toggles — Claude: plan mode, ultracode, skip     |
//! |        | permissions; Codex: skip permissions; pi: nothing (no        |
//! |        | permission system)                                           |
//! | Doctor | "Check tools" report                                         |
//!
//! Model/effort are [`crate::coding_selects`] choice selects (never free
//! text — the closed alias sets the CLI accepts). The per-agent toggles are
//! what the shared Start-coding dialog prefills from — ONE set of defaults
//! for single-issue and batch runs alike (EXP-206): Claude plan mode ON,
//! ultracode OFF, skip permissions OFF everywhere.
//!
//! Settings persist through [`crate::coding_flow::CodingHub`] to the local
//! per-install `settings.json` — never synced. Saving re-runs the doctor
//! against the new claude path, and the doctor's report is exactly what
//! gates the Start-coding button (§7.1 step 1 ANDs `claude.ok && git.ok`,
//! including the minimum-version gate).
//!
//! The personal API key is provisioned and rotated **fully automatically**
//! (`api::users::ensure_personal_key` on the first coding session; the
//! `.exp-mcp.json` writer picks it up), so there is no key UI here at all — no
//! value field, no reveal, no copy, no manual entry, no status row.

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    select::Select,
    skeleton::Skeleton,
    switch::Switch,
    tab::{Tab, TabBar},
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, Size,
};

use coding::{CodingAgent, DoctorReport, Settings, ToolCheck};

use crate::coding_flow::CodingHub;
use crate::coding_selects::{
    choice_select, effort_choices_for, model_choices_for, selected, ChoiceSelect, AGENT_CHOICES,
};

use super::{card, card_header, error_notice};

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

pub struct CodingPane {
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
    repos_input: Entity<InputState>,
    prefix_input: Entity<InputState>,
    /// Which agent tab of the Agents card is showing — pure UI state, not
    /// persisted (EXP-206).
    agent_tab: CodingAgent,
    /// Per-agent run defaults (the shared dialog's prefill — EXP-206: one
    /// set for issue and batch runs alike): Claude plan mode ON out of the
    /// box, everything else OFF.
    claude_ultracode: bool,
    claude_plan_mode: bool,
    claude_skip_permissions: bool,
    codex_skip_permissions: bool,
    /// The hub settings the controls were last synced from (dirty baseline).
    synced: Option<Settings>,
    save_error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl CodingPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let claude_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CLAUDE_PATH));
        let codex_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CODEX_PATH));
        let pi_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_PI_PATH));
        let repos_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_REPOS_ROOT));
        let prefix_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(coding::settings::DEFAULT_BRANCH_PREFIX)
        });
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
        for input in [&claude_input, &codex_input, &pi_input, &repos_input, &prefix_input] {
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
            repos_input,
            prefix_input,
            agent_tab: defaults.default_agent,
            claude_ultracode: defaults.claude_ultracode,
            claude_plan_mode: defaults.claude_plan_mode,
            claude_skip_permissions: defaults.claude_skip_permissions,
            codex_skip_permissions: defaults.codex_skip_permissions,
            synced: None,
            save_error: None,
            _subscriptions: subscriptions,
        };
        this.resync(window, cx);
        this
    }

    /// Mirror the hub's settings into the controls whenever they change out
    /// from under us (save from another pane instance, first build).
    fn resync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();
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
        self.repos_input.update(cx, |input, cx| {
            input.set_value(settings.repos_root.clone(), window, cx)
        });
        self.prefix_input.update(cx, |input, cx| {
            input.set_value(settings.branch_prefix.clone(), window, cx)
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

    /// The settings the controls currently describe. Blank text fields
    /// degrade to the §7.7 defaults — a hand-blanked pane can never produce
    /// an unusable launcher (mirrors `Settings::load`); the selects are
    /// closed sets by construction.
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
        Settings {
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
            claude_skip_permissions: self.claude_skip_permissions,
            codex_skip_permissions: self.codex_skip_permissions,
            repos_root: value(&self.repos_input, &defaults.repos_root),
            branch_prefix: value(&self.prefix_input, &defaults.branch_prefix),
        }
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
        self.save_error = CodingHub::save_settings(&hub, drafted.clone(), cx)
            .err()
            .map(SharedString::from);
        // `synced` follows the hub via the observer's resync; setting it here
        // too keeps the Save button honest when the observer coalesces.
        self.synced = Some(drafted);
        cx.notify();
    }

    // -- render pieces --------------------------------------------------------

    fn labeled_input(
        label: &'static str,
        hint: &'static str,
        input: &Entity<InputState>,
        cx: &App,
    ) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(label))
            .child(Input::new(input).small())
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.7))
                    .child(hint),
            )
    }

    /// A labeled [`ChoiceSelect`] row (the select analog of `labeled_input`).
    fn labeled_select(
        label: &'static str,
        hint: &'static str,
        select: &ChoiceSelect,
        cx: &App,
    ) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(label))
            .child(Select::new(select).small())
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.7))
                    .child(hint),
            )
    }

    /// One toggle row: label + hint on the left, a `Switch` on the right
    /// (the notifications pane's row shape).
    fn toggle_row(
        id: &'static str,
        label: &'static str,
        hint: &'static str,
        checked: bool,
        on_click: impl Fn(&mut Self, &bool, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                v_flex()
                    .gap_0p5()
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground.opacity(0.7))
                            .child(hint),
                    ),
            )
            .child(
                Switch::new(id)
                    .checked(checked)
                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                        on_click(this, checked, cx);
                        cx.notify();
                    })),
            )
    }

    /// One doctor row: green check + version, or the actionable error — red
    /// for launch-blocking rows (git + the default agent), muted for the
    /// OPTIONAL agents (EXP-201: only the agent you launch with must be
    /// installed, so a missing codex is information, not an alarm).
    fn doctor_row(check: &ToolCheck, muted_when_failing: bool, cx: &App) -> impl IntoElement {
        let (icon, color, detail): (IconName, gpui::Hsla, SharedString) = if check.ok {
            (
                IconName::CircleCheck,
                theme::tokens::GREEN.to_hsla(),
                check.version.clone().unwrap_or_default().into(),
            )
        } else if muted_when_failing {
            (
                IconName::CircleX,
                cx.theme().muted_foreground,
                check
                    .error
                    .clone()
                    .unwrap_or_else(|| format!("{} is not installed", check.tool))
                    .into(),
            )
        } else {
            (
                IconName::CircleX,
                cx.theme().danger,
                check
                    .error
                    .clone()
                    .unwrap_or_else(|| format!("{} is not available", check.tool))
                    .into(),
            )
        };
        let detail_color = if check.ok || muted_when_failing {
            cx.theme().muted_foreground
        } else {
            cx.theme().danger
        };
        h_flex()
            .gap_2()
            .items_center()
            .child(Icon::new(icon).small().text_color(color))
            .child(
                div()
                    .w_16()
                    .flex_shrink_0()
                    .text_sm()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(check.tool.label())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(detail_color)
                    .child(detail),
            )
    }

    fn render_doctor_card(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let hub = CodingHub::global(cx);
        let (report, running, default_agent): (Option<DoctorReport>, bool, CodingAgent) = {
            let hub = hub.read(cx);
            (
                hub.doctor.report.clone(),
                hub.doctor.running,
                hub.settings.default_agent,
            )
        };

        let mut body = card(cx).child(card_header(
            "Tooling doctor",
            "git is required; of the agent CLIs, only the one you launch with must be \
             installed. A red row blocks that launch until it's fixed.",
            cx,
        ));
        match &report {
            None => {
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(Skeleton::new().h_4().w_64())
                        .child(Skeleton::new().h_4().w_56()),
                );
            }
            Some(report) => {
                for agent in CodingAgent::ALL {
                    body = body.child(Self::doctor_row(
                        report.check_for(agent),
                        agent != default_agent,
                        cx,
                    ));
                }
                body = body.child(Self::doctor_row(&report.git, false, cx));
            }
        }
        body.child(
            h_flex().child(
                Button::new("doctor-check")
                    .outline()
                    .xsmall()
                    .label("Check tools")
                    .loading(running)
                    .disabled(running)
                    .on_click(cx.listener(|_, _, _, cx| {
                        let hub = CodingHub::global(cx);
                        CodingHub::refresh_doctor(&hub, cx);
                    })),
            ),
        )
    }
}

impl CodingPane {
    /// The Agents card (EXP-206): one TAB per agent, each holding that
    /// agent's CLI path + model/effort selects and its OWN toggles — plan
    /// mode and ultracode exist only on the Claude tab, skip permissions on
    /// Claude and Codex.
    fn render_agents_card(&mut self, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let active_ix = CodingAgent::ALL
            .iter()
            .position(|agent| *agent == self.agent_tab)
            .unwrap_or(0);
        let tabs = TabBar::new("coding-agent-tabs")
            .with_size(Size::Small)
            .selected_index(active_ix)
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                if let Some(agent) = CodingAgent::ALL.get(*ix).copied() {
                    this.agent_tab = agent;
                    cx.notify();
                }
            }))
            .children(
                CodingAgent::ALL
                    .iter()
                    .map(|agent| Tab::new().label(SharedString::from(agent.label()))),
            );

        let mut body = card(cx)
            .child(card_header(
                "Agents",
                "Each agent's CLI, model, and run defaults — the Start-coding dialog \
                 prefills the toggles, and every launch can still override them.",
                cx,
            ))
            .child(tabs);
        body = match self.agent_tab {
            CodingAgent::Claude => body
                .child(Self::labeled_input(
                    "CLI path",
                    "Command name or absolute path — used verbatim to launch coding sessions.",
                    &self.claude_input,
                    cx,
                ))
                .child(Self::labeled_select(
                    "Model",
                    "Passed as --model on every claude session. Default: Fable.",
                    &self.model_select,
                    cx,
                ))
                .child(Self::labeled_select(
                    "Effort",
                    "CLI default leaves --effort unset.",
                    &self.effort_select,
                    cx,
                ))
                .child(Self::toggle_row(
                    "claude-plan-mode",
                    "Plan mode",
                    "Claude presents a plan and waits for your approval in the \
                     terminal before editing.",
                    self.claude_plan_mode,
                    |this, checked, _| this.claude_plan_mode = *checked,
                    cx,
                ))
                .child(Self::toggle_row(
                    "claude-ultracode",
                    "Dynamic workflows (ultracode)",
                    "Runs sessions with --effort ultracode — works with any model.",
                    self.claude_ultracode,
                    |this, checked, _| this.claude_ultracode = *checked,
                    cx,
                ))
                .child(Self::toggle_row(
                    "claude-skip-permissions",
                    "Skip permissions",
                    "Full bypass (--dangerously-skip-permissions) instead of the \
                     guarded auto mode.",
                    self.claude_skip_permissions,
                    |this, checked, _| this.claude_skip_permissions = *checked,
                    cx,
                )),
            CodingAgent::Codex => body
                .child(Self::labeled_input(
                    "CLI path",
                    "Command name or absolute path of OpenAI's codex CLI.",
                    &self.codex_input,
                    cx,
                ))
                .child(Self::labeled_select(
                    "Model",
                    "Passed as -m; CLI default uses codex's own configured model.",
                    &self.codex_model_select,
                    cx,
                ))
                .child(Self::labeled_select(
                    "Reasoning effort",
                    "Sets model_reasoning_effort; CLI default leaves it unset.",
                    &self.codex_effort_select,
                    cx,
                ))
                .child(Self::toggle_row(
                    "codex-skip-permissions",
                    "Skip permissions",
                    "Full bypass (--dangerously-bypass-approvals-and-sandbox) instead \
                     of the guarded auto preset.",
                    self.codex_skip_permissions,
                    |this, checked, _| this.codex_skip_permissions = *checked,
                    cx,
                )),
            CodingAgent::Pi => body
                .child(Self::labeled_input(
                    "CLI path",
                    "Command name or absolute path of the pi coding agent (pi.dev).",
                    &self.pi_input,
                    cx,
                ))
                .child(Self::labeled_select(
                    "Model",
                    "Passed as --model (pi resolves fuzzy patterns); CLI default uses pi's own.",
                    &self.pi_model_select,
                    cx,
                ))
                .child(Self::labeled_select(
                    "Thinking level",
                    "Passed as --thinking; CLI default leaves it unset.",
                    &self.pi_thinking_select,
                    cx,
                ))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground.opacity(0.7))
                        .child("pi has no permission system — sessions always run unguarded."),
                ),
        };
        body
    }
}

impl Render for CodingPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let dirty = self.dirty(cx);

        let general_card = card(cx)
            .child(card_header(
                "Coding",
                "Local launcher settings for \u{201c}Start coding\u{201d} — per machine, never synced.",
                cx,
            ))
            .child(Self::labeled_select(
                "Default agent",
                "Preselected in the Start-coding dialog — every launch can still pick another.",
                &self.agent_select,
                cx,
            ))
            .child(Self::labeled_input(
                "Repos & worktrees root",
                "Where repositories are cloned and per-issue worktrees are created (~ works).",
                &self.repos_input,
                cx,
            ))
            .child(Self::labeled_input(
                "Branch prefix",
                "Prepended to the issue identifier for the coding branch (exp/EXP-42).",
                &self.prefix_input,
                cx,
            ));

        let agents_card = self.render_agents_card(cx);

        let mut save_area = v_flex().gap_2();
        if let Some(error) = &self.save_error {
            save_area = save_area.child(error_notice(error.clone(), cx));
        }
        save_area = save_area.child(
            h_flex().justify_end().child(
                Button::new("coding-save")
                    .primary()
                    .small()
                    .label("Save changes")
                    .disabled(!dirty)
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            ),
        );

        v_flex()
            .gap_4()
            .child(general_card)
            .child(agents_card)
            .child(save_area)
            .child(self.render_doctor_card(cx))
    }
}
