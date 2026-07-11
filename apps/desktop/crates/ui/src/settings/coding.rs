//! Settings → Coding (masterplan-v3 §7.7 DC-3, §7.2).
//!
//! The JetBrains-SDK-settings-style pane for the Start-coding launcher:
//!
//! | Setting                | Default               |
//! |------------------------|-----------------------|
//! | Claude CLI path        | `claude`              |
//! | Claude model           | Fable (select)        |
//! | Claude effort          | CLI default (select)  |
//! | Repos & worktrees root | `~/Exponential/repos` |
//! | Branch prefix          | `exp/`                |
//! | Tooling doctor         | "Check tools"         |
//!
//! Model/effort are [`crate::coding_selects`] choice selects (never free
//! text — the closed alias sets the CLI accepts), plus the release-run
//! subagent defaults and three toggles: ultracode (release runs) and the two
//! NATIVE plan-mode defaults (issue runs ON / release runs OFF) the shared
//! Start-coding dialog prefills from.
//!
//! Settings persist through [`crate::coding_flow::CodingHub`] to the local
//! per-install `settings.json` — never synced. Saving re-runs the doctor
//! against the new claude path, and the doctor's report is exactly what
//! gates the Start-coding button (§7.1 step 1 ANDs `claude.ok && git.ok`,
//! including the minimum-version gate).
//!
//! The personal API key is provisioned and rotated **fully automatically**
//! (`api::users::ensure_personal_key` on the first coding session; the
//! `.mcp.json` writer picks it up), so there is no key UI here at all — no
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
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
};

use coding::{DoctorReport, Settings, ToolCheck};

use crate::coding_flow::CodingHub;
use crate::coding_selects::{
    choice_select, selected, ChoiceSelect, EFFORT_CHOICES, MODEL_CHOICES,
    SUBAGENT_EFFORT_CHOICES, SUBAGENT_MODEL_CHOICES,
};

use super::{card, card_header, error_notice};

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

pub struct CodingPane {
    claude_input: Entity<InputState>,
    model_select: ChoiceSelect,
    effort_select: ChoiceSelect,
    repos_input: Entity<InputState>,
    prefix_input: Entity<InputState>,
    /// EXP-56 release-run defaults (the launch dialog's prefill).
    subagent_model_select: ChoiceSelect,
    subagent_effort_select: ChoiceSelect,
    release_ultracode: bool,
    /// Native plan-mode defaults (the shared dialog's prefill; issue ON /
    /// release OFF out of the box).
    issue_plan_mode: bool,
    release_plan_mode: bool,
    /// The hub settings the controls were last synced from (dirty baseline).
    synced: Option<Settings>,
    save_error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl CodingPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let claude_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CLAUDE_PATH));
        let repos_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_REPOS_ROOT));
        let prefix_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(coding::settings::DEFAULT_BRANCH_PREFIX)
        });
        let defaults = Settings::default();
        let model_select = choice_select(&MODEL_CHOICES, &defaults.claude_model, window, cx);
        let effort_select = choice_select(&EFFORT_CHOICES, &defaults.claude_effort, window, cx);
        let subagent_model_select =
            choice_select(&SUBAGENT_MODEL_CHOICES, &defaults.subagent_model, window, cx);
        let subagent_effort_select =
            choice_select(&SUBAGENT_EFFORT_CHOICES, &defaults.subagent_effort, window, cx);

        // Creating the hub also kicks the FIRST doctor run (§7.7 onboarding).
        let hub = CodingHub::global(cx);
        let mut subscriptions = vec![
            // Doctor results / external settings changes re-render + resync.
            cx.observe_in(&hub, window, |this, _, window, cx| {
                this.resync(window, cx);
                cx.notify();
            }),
        ];
        for input in [&claude_input, &repos_input, &prefix_input] {
            subscriptions.push(cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify(); // live dirty tracking on the Save button
                }
            }));
        }
        for select in [
            &model_select,
            &effort_select,
            &subagent_model_select,
            &subagent_effort_select,
        ] {
            // Confirming a choice notifies the state — observing keeps the
            // Save button's dirty tracking live without a typed subscription.
            subscriptions.push(cx.observe(select, |_, _, cx| cx.notify()));
        }

        let mut this = Self {
            claude_input,
            model_select,
            effort_select,
            repos_input,
            prefix_input,
            subagent_model_select,
            subagent_effort_select,
            release_ultracode: defaults.release_ultracode,
            issue_plan_mode: defaults.issue_plan_mode,
            release_plan_mode: defaults.release_plan_mode,
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
        self.repos_input.update(cx, |input, cx| {
            input.set_value(settings.repos_root.clone(), window, cx)
        });
        self.prefix_input.update(cx, |input, cx| {
            input.set_value(settings.branch_prefix.clone(), window, cx)
        });
        // The persisted values are load-normalized into the choice sets, so
        // every set_selected_value below finds its row.
        self.model_select.update(cx, |select, cx| {
            select.set_selected_value(&SharedString::from(settings.claude_model.clone()), window, cx)
        });
        self.effort_select.update(cx, |select, cx| {
            select.set_selected_value(
                &SharedString::from(settings.claude_effort.clone()),
                window,
                cx,
            )
        });
        self.subagent_model_select.update(cx, |select, cx| {
            select.set_selected_value(
                &SharedString::from(settings.subagent_model.clone()),
                window,
                cx,
            )
        });
        self.subagent_effort_select.update(cx, |select, cx| {
            select.set_selected_value(
                &SharedString::from(settings.subagent_effort.clone()),
                window,
                cx,
            )
        });
        self.release_ultracode = settings.release_ultracode;
        self.issue_plan_mode = settings.issue_plan_mode;
        self.release_plan_mode = settings.release_plan_mode;
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
            claude_path: value(&self.claude_input, &defaults.claude_path),
            claude_model: selected(&self.model_select, cx),
            claude_effort: selected(&self.effort_select, cx),
            subagent_model: selected(&self.subagent_model_select, cx),
            subagent_effort: selected(&self.subagent_effort_select, cx),
            release_ultracode: self.release_ultracode,
            release_plan_mode: self.release_plan_mode,
            issue_plan_mode: self.issue_plan_mode,
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

    /// One doctor row: green check + version, or red X + the actionable error.
    fn doctor_row(check: &ToolCheck, cx: &App) -> impl IntoElement {
        let (icon, color, detail): (IconName, gpui::Hsla, SharedString) = if check.ok {
            (
                IconName::CircleCheck,
                theme::tokens::GREEN.to_hsla(),
                check.version.clone().unwrap_or_default().into(),
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
                    .text_color(if check.ok {
                        cx.theme().muted_foreground
                    } else {
                        cx.theme().danger
                    })
                    .child(detail),
            )
    }

    fn render_doctor_card(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let hub = CodingHub::global(cx);
        let (report, running): (Option<DoctorReport>, bool) = {
            let hub = hub.read(cx);
            (hub.doctor.report.clone(), hub.doctor.running)
        };

        let mut body = card(cx).child(card_header(
            "Tooling doctor",
            "Start coding needs both tools. A red row blocks the button until it's fixed.",
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
                body = body
                    .child(Self::doctor_row(&report.agent, cx))
                    .child(Self::doctor_row(&report.git, cx));
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

impl Render for CodingPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let dirty = self.dirty(cx);

        let mut settings_card = card(cx)
            .child(card_header(
                "Coding",
                "Local launcher settings for \u{201c}Start coding\u{201d} — per machine, never synced.",
                cx,
            ))
            .child(Self::labeled_input(
                "Claude CLI path",
                "Command name or absolute path — used verbatim to launch coding sessions.",
                &self.claude_input,
                cx,
            ))
            .child(Self::labeled_select(
                "Claude model",
                "Passed as --model on every session. Default: Fable.",
                &self.model_select,
                cx,
            ))
            .child(Self::labeled_select(
                "Claude effort",
                "CLI default leaves --effort unset.",
                &self.effort_select,
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
            ))
            .child(Self::toggle_row(
                "issue-plan-mode",
                "Plan mode — issue runs",
                "Claude presents a plan and waits for your approval in the terminal before editing.",
                self.issue_plan_mode,
                |this, checked, _| this.issue_plan_mode = *checked,
                cx,
            ))
            // EXP-56: defaults for "Start coding" on a whole RELEASE — the
            // launch dialog prefills from these four.
            .child(
                div()
                    .pt_2()
                    .text_sm()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("Release runs"),
            )
            .child(Self::labeled_select(
                "Subagent model",
                "Default model for per-issue subagents in release runs — Inherit uses the orchestrator's model.",
                &self.subagent_model_select,
                cx,
            ))
            .child(Self::labeled_select(
                "Subagent effort",
                "Default effort for per-issue subagents — Inherit uses the orchestrator's effort.",
                &self.subagent_effort_select,
                cx,
            ))
            .child(Self::toggle_row(
                "release-ultracode",
                "Dynamic workflows (ultracode)",
                "Runs the orchestrator with --effort ultracode — works with any model.",
                self.release_ultracode,
                |this, checked, _| this.release_ultracode = *checked,
                cx,
            ))
            .child(Self::toggle_row(
                "release-plan-mode",
                "Plan mode — release runs",
                "The orchestrator presents its wave plan for approval before pushing anything.",
                self.release_plan_mode,
                |this, checked, _| this.release_plan_mode = *checked,
                cx,
            ));
        if let Some(error) = &self.save_error {
            settings_card = settings_card.child(error_notice(error.clone(), cx));
        }
        settings_card = settings_card.child(
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
            .child(settings_card)
            .child(self.render_doctor_card(cx))
    }
}
