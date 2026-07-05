//! Settings → Coding (masterplan-v3 §7.7 DC-3, §7.2).
//!
//! The JetBrains-SDK-settings-style pane for the Start-coding launcher:
//!
//! | Setting              | Default               |
//! |----------------------|-----------------------|
//! | Claude CLI path      | `claude`              |
//! | Repos & worktrees root | `~/Exponential/repos` |
//! | Branch prefix        | `exp/`                |
//! | Tooling doctor       | "Check tools"         |
//!
//! Settings persist through [`crate::coding_flow::CodingHub`] to the local
//! per-install `settings.json` — never synced. Saving re-runs the doctor
//! against the new claude path, and the doctor's report is exactly what
//! gates the Start-coding button (§7.1 step 1 ANDs `claude.ok && git.ok`).
//! The doctor auto-runs when the hub first exists (the §7.7 onboarding rule:
//! clear errors BEFORE Start coding is usable — the red rows here carry the
//! actionable copy: "claude not found on PATH — set an absolute path" /
//! "git not found on PATH").
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
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
};

use coding::{DoctorReport, Settings, ToolCheck};

use crate::coding_flow::CodingHub;

use super::{card, card_header, error_notice};

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

pub struct CodingPane {
    claude_input: Entity<InputState>,
    model_input: Entity<InputState>,
    repos_input: Entity<InputState>,
    prefix_input: Entity<InputState>,
    /// The hub settings the inputs were last synced from (dirty baseline).
    synced: Option<Settings>,
    save_error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl CodingPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let claude_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CLAUDE_PATH));
        let model_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CLAUDE_MODEL));
        let repos_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_REPOS_ROOT));
        let prefix_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_BRANCH_PREFIX));

        // Creating the hub also kicks the FIRST doctor run (§7.7 onboarding).
        let hub = CodingHub::global(cx);
        let mut subscriptions = vec![
            // Doctor results / external settings changes re-render + resync.
            cx.observe_in(&hub, window, |this, _, window, cx| {
                this.resync(window, cx);
                cx.notify();
            }),
        ];
        for input in [&claude_input, &model_input, &repos_input, &prefix_input] {
            subscriptions.push(cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify(); // live dirty tracking on the Save button
                }
            }));
        }

        let mut this = Self {
            claude_input,
            model_input,
            repos_input,
            prefix_input,
            synced: None,
            save_error: None,
            _subscriptions: subscriptions,
        };
        this.resync(window, cx);
        this
    }

    /// Mirror the hub's settings into the inputs whenever they change out
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
        self.model_input.update(cx, |input, cx| {
            input.set_value(settings.claude_model.clone(), window, cx)
        });
        self.repos_input.update(cx, |input, cx| {
            input.set_value(settings.repos_root.clone(), window, cx)
        });
        self.prefix_input.update(cx, |input, cx| {
            input.set_value(settings.branch_prefix.clone(), window, cx)
        });
        self.synced = Some(settings);
        cx.notify();
    }

    /// The settings the inputs currently describe. Blank fields degrade to
    /// the §7.7 defaults — a hand-blanked pane can never produce an unusable
    /// launcher (mirrors `Settings::load`).
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
            claude_model: value(&self.model_input, &defaults.claude_model),
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
                    .child(Self::doctor_row(&report.claude, cx))
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
            .child(Self::labeled_input(
                "Claude model",
                "Passed as --model on every coding session — never your CLI default. Try opus, sonnet, haiku, or fable.",
                &self.model_input,
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
