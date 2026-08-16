//! The shared "Tooling doctor" panel (EXP-367) — rendered by Settings →
//! Tools AND the first-run onboarding tools step, so the two can never
//! drift.
//!
//! One row per tool (the three agent CLIs, then git), colored by
//! [`row_severity`] — git failing is always red (required); a failing agent
//! is muted INFORMATION while another agent is green, and only goes red when
//! NO agent is installed (coding is disabled then). Every failing row grows
//! actionable guidance: an install hint + link, and (for the agents) an
//! inline absolute-path input saved straight through
//! [`CodingHub::save_settings`].
//!
//! Saving a path here overlays ONLY that one field onto the hub's LIVE
//! settings (the merge-preserving save re-runs the doctor). For the Agents
//! pane this is an external owned-field change — its `resync` rewrites its
//! inputs from the hub, at worst refreshing away unsaved sibling edits
//! there, the same as any external save today.

use gpui::{
    div, App, AppContext as _, Entity, Hsla, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::Button,
    h_flex,
    input::{Input, InputEvent, InputState},
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use coding::{CodingAgent, DoctorReport, Tool, ToolCheck};

use crate::coding_flow::CodingHub;
use crate::controls::WebControl as _;
use crate::icons::registry;

use super::{card_header, section};

// ---------------------------------------------------------------------------
// Row severity (the EXP-367 red/white fix)
// ---------------------------------------------------------------------------

/// How a doctor row renders. The old rows colored by "is this the DEFAULT
/// agent", which read as random red/white; severity now follows what the
/// failure MEANS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowSeverity {
    Ok,
    /// Failing but optional — another agent covers coding.
    Muted,
    /// Failing and blocking — git, or an agent when NO agent is installed.
    Danger,
}

/// Pure severity rule: git failing is always danger (required for
/// everything); agent rows are muted information while at least one agent is
/// green, and ALL go danger when none is (coding is disabled then).
pub(crate) fn row_severity(check: &ToolCheck, report: &DoctorReport) -> RowSeverity {
    if check.ok {
        RowSeverity::Ok
    } else if check.tool == Tool::Git || !report.any_agent_ok() {
        RowSeverity::Danger
    } else {
        RowSeverity::Muted
    }
}

/// Per-tool install guidance: a one-line hint (with the copy-pasteable
/// command where one exists) + the canonical download page.
pub(crate) fn install_hint(tool: Tool) -> (&'static str, &'static str) {
    match tool {
        Tool::Git => (
            "Install Git (on Windows: winget install --id Git.Git). Just installed it? \
             Click Check tools again.",
            "https://git-scm.com/downloads",
        ),
        Tool::Claude => (
            // EXP-419: link the CLI install docs, not the product page — a
            // fresh machine needs the claude CLI, not the desktop app.
            "Install Claude Code: npm install -g @anthropic-ai/claude-code",
            "https://code.claude.com/docs/en/quickstart#step-1-install-claude-code",
        ),
        Tool::Codex => (
            "Install the Codex CLI: npm install -g @openai/codex",
            "https://developers.openai.com/codex/cli",
        ),
        Tool::Pi => (
            "Install the pi coding agent from pi.dev",
            "https://pi.dev",
        ),
    }
}

/// EXP-409: guidance for an INSTALLED agent that is signed out — the fix is
/// a login, not an install, so the install hint would mislead.
pub(crate) fn sign_in_hint(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => {
            "Sign in: run `claude` in a terminal and complete the login (works over ssh too)."
        }
        Tool::Codex => "Sign in: run `codex login` in a terminal.",
        Tool::Pi => {
            "Give pi a credential: run `pi` and sign in with /login, or set a provider \
             API key (e.g. ANTHROPIC_API_KEY)."
        }
        Tool::Git => "",
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

pub struct DoctorPanel {
    claude_input: Entity<InputState>,
    codex_input: Entity<InputState>,
    pi_input: Entity<InputState>,
    /// The hub paths the inputs were last synced from — external-change
    /// detection only (each input saves itself; there is no pane-wide Save).
    synced_paths: Option<(String, String, String)>,
    _subscriptions: Vec<Subscription>,
}

impl DoctorPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let claude_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CLAUDE_PATH));
        let codex_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CODEX_PATH));
        let pi_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_PI_PATH));

        // Creating the hub also kicks the FIRST doctor run (§7.7 onboarding).
        let hub = CodingHub::global(cx);
        let mut subscriptions = vec![cx.observe_in(&hub, window, |this, _, window, cx| {
            this.resync(window, cx);
            cx.notify();
        })];
        for input in [&claude_input, &codex_input, &pi_input] {
            subscriptions.push(cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify(); // live dirty tracking on the Save-path button
                }
            }));
        }

        let mut this = Self {
            claude_input,
            codex_input,
            pi_input,
            synced_paths: None,
            _subscriptions: subscriptions,
        };
        this.resync(window, cx);
        this
    }

    fn input_for(&self, agent: CodingAgent) -> &Entity<InputState> {
        match agent {
            CodingAgent::Claude => &self.claude_input,
            CodingAgent::Codex => &self.codex_input,
            CodingAgent::Pi => &self.pi_input,
        }
    }

    /// Mirror the hub's agent paths into the inputs whenever they change out
    /// from under us (an Agents-pane save, another window).
    fn resync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();
        let paths = (
            settings.claude_path.clone(),
            settings.codex_path.clone(),
            settings.pi_path.clone(),
        );
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
    /// doctor, which is exactly the "did my path fix it?" feedback loop.
    fn save_path(&mut self, agent: CodingAgent, cx: &mut gpui::Context<Self>) {
        let raw = self.input_for(agent).read(cx).value().trim().to_string();
        let value = if raw.is_empty() {
            match agent {
                CodingAgent::Claude => coding::settings::DEFAULT_CLAUDE_PATH,
                CodingAgent::Codex => coding::settings::DEFAULT_CODEX_PATH,
                CodingAgent::Pi => coding::settings::DEFAULT_PI_PATH,
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
            CodingAgent::Pi => settings.pi_path = value,
        }
        let _ = CodingHub::save_settings(&hub, settings, cx);
        cx.notify();
    }

    /// One tool row: status icon + monospace tool name + detail, icon AND
    /// detail sharing the severity color (the tool name stays foreground in
    /// every state — the old mixed red-icon/white-text rows read as noise).
    fn tool_row(check: &ToolCheck, severity: RowSeverity, cx: &App) -> impl IntoElement {
        let (icon, color): (crate::icons::ExpIcon, Hsla) = match severity {
            RowSeverity::Ok => (registry::UI_SUCCESS, theme::tokens::GREEN.to_hsla()),
            RowSeverity::Muted => (registry::UI_ERROR, cx.theme().muted_foreground),
            RowSeverity::Danger => (registry::UI_ERROR, cx.theme().danger),
        };
        let detail: SharedString = if check.ok {
            check.version.clone().unwrap_or_default().into()
        } else {
            check
                .error
                .clone()
                .unwrap_or_else(|| format!("{} is not installed", check.tool))
                .into()
        };
        let detail_color = match severity {
            RowSeverity::Ok | RowSeverity::Muted => cx.theme().muted_foreground,
            RowSeverity::Danger => cx.theme().danger,
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

    /// The guidance block under a failing row: install hint + link, plus the
    /// inline path input for agent tools (git has no path setting — it must
    /// be on PATH). An installed-but-signed-out agent (EXP-409) instead gets
    /// the sign-in hint alone — its binary and path are fine.
    fn guidance(&self, check: &ToolCheck, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let tool = check.tool;
        let muted = cx.theme().muted_foreground;
        if check.signed_out() {
            return v_flex().pl_7().gap_1p5().child(
                div()
                    .text_xs()
                    .text_color(muted.opacity(0.9))
                    .child(sign_in_hint(tool)),
            );
        }
        let (hint, url) = install_hint(tool);
        let mut block = v_flex()
            .pl_7()
            .gap_1p5()
            .child(div().text_xs().text_color(muted.opacity(0.9)).child(hint))
            .child(
                h_flex().child(
                    Button::new(SharedString::from(format!("doctor-install-{}", tool.label())))
                        .outline().cursor_pointer()
                        .web_xs()
                        .label("Install page")
                        .icon(registry::UI_EXTERNAL_LINK)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            super::open_url(cx, url.to_string());
                        })),
                ),
            );
        if let Some(agent) = tool.agent() {
            block = block.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().min_w_0().child(
                        Input::new(self.input_for(agent)).web_input_sm(),
                    ))
                    .child(
                        Button::new(SharedString::from(format!(
                            "doctor-save-path-{}",
                            agent.id()
                        )))
                        .outline().cursor_pointer()
                        .web_xs()
                        .label("Save path")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.save_path(agent, cx);
                        })),
                    ),
            );
        }
        block
    }
}

impl Render for DoctorPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let hub = CodingHub::global(cx);
        let (report, running) = {
            let hub = hub.read(cx);
            (hub.doctor.report.clone(), hub.doctor.running)
        };

        let mut body = section(cx).child(card_header(
            "Tooling doctor",
            "git is required. You cannot start coding without an agent CLI.",
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
                    let check = report.check_for(agent).clone();
                    let severity = row_severity(&check, report);
                    body = body.child(Self::tool_row(&check, severity, cx));
                    if severity != RowSeverity::Ok {
                        body = body.child(self.guidance(&check, cx));
                    }
                }
                let git = report.git.clone();
                let severity = row_severity(&git, report);
                body = body.child(Self::tool_row(&git, severity, cx));
                if severity != RowSeverity::Ok {
                    body = body.child(self.guidance(&git, cx));
                }
            }
        }
        body.child(
            h_flex().child(
                Button::new("doctor-check")
                    .outline().cursor_pointer()
                    .web_sm()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn green(tool: Tool) -> ToolCheck {
        ToolCheck {
            tool,
            ok: true,
            version: Some("1.0.0".to_string()),
            error: None,
            authed: None,
        }
    }

    fn red(tool: Tool) -> ToolCheck {
        ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some(format!("{} not found on PATH", tool.label())),
            authed: None,
        }
    }

    /// EXP-367: severity follows what a failure MEANS — git is always
    /// required; agents are optional while a sibling covers coding.
    #[test]
    fn severity_matrix() {
        // One agent green → the others' failures are muted information.
        let one_ok = DoctorReport {
            claude: green(Tool::Claude),
            codex: red(Tool::Codex),
            pi: red(Tool::Pi),
            git: green(Tool::Git),
        };
        assert_eq!(row_severity(&one_ok.claude, &one_ok), RowSeverity::Ok);
        assert_eq!(row_severity(&one_ok.codex, &one_ok), RowSeverity::Muted);
        assert_eq!(row_severity(&one_ok.pi, &one_ok), RowSeverity::Muted);

        // NO agent installed → every agent row is danger (coding disabled).
        let none_ok = DoctorReport {
            claude: red(Tool::Claude),
            codex: red(Tool::Codex),
            pi: red(Tool::Pi),
            git: red(Tool::Git),
        };
        assert_eq!(row_severity(&none_ok.claude, &none_ok), RowSeverity::Danger);
        assert_eq!(row_severity(&none_ok.pi, &none_ok), RowSeverity::Danger);

        // git failing is danger REGARDLESS of the agents' state.
        assert_eq!(row_severity(&none_ok.git, &none_ok), RowSeverity::Danger);
        let git_only_broken = DoctorReport {
            git: red(Tool::Git),
            ..one_ok
        };
        assert_eq!(
            row_severity(&git_only_broken.git, &git_only_broken),
            RowSeverity::Danger
        );
    }

    /// Every failing tool gets an install link (guidance is never blank).
    #[test]
    fn every_tool_has_an_install_hint() {
        for tool in [Tool::Claude, Tool::Codex, Tool::Pi, Tool::Git] {
            let (hint, url) = install_hint(tool);
            assert!(!hint.is_empty());
            assert!(url.starts_with("https://"), "{url}");
        }
    }

    /// EXP-409: a signed-out agent follows the same severity rules as a
    /// missing one (muted while a sibling covers coding, danger when none
    /// does), and every agent has a non-empty sign-in hint.
    #[test]
    fn signed_out_rows_share_the_severity_rules_and_have_hints() {
        let signed_out = |tool: Tool| ToolCheck {
            authed: Some(false),
            ok: false,
            version: Some("1.0.0".to_string()),
            error: Some("signed out".to_string()),
            tool,
        };
        let one_ok = DoctorReport {
            claude: green(Tool::Claude),
            codex: signed_out(Tool::Codex),
            pi: red(Tool::Pi),
            git: green(Tool::Git),
        };
        assert_eq!(row_severity(&one_ok.codex, &one_ok), RowSeverity::Muted);
        let none_ok = DoctorReport {
            claude: signed_out(Tool::Claude),
            codex: signed_out(Tool::Codex),
            pi: red(Tool::Pi),
            git: green(Tool::Git),
        };
        assert_eq!(row_severity(&none_ok.claude, &none_ok), RowSeverity::Danger);

        for tool in [Tool::Claude, Tool::Codex, Tool::Pi] {
            assert!(!sign_in_hint(tool).is_empty());
        }
    }
}
