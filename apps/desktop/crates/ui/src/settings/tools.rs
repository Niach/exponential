//! Settings → Tools (EXP-288 — renamed from "Coding" and slimmed to the
//! non-agent, this-device knobs; the per-agent settings + doctor moved to
//! [`super::agents`]).
//!
//! | Row            | Meaning                                              |
//! |----------------|------------------------------------------------------|
//! | Repos root     | Where repositories/worktrees live (`~` works)        |
//! | Branch prefix  | Prepended to the issue identifier (`exp/EXP-42`)     |
//! | Terminal shell | Program new `+` terminal tabs spawn (blank = auto)   |
//!
//! Settings persist through [`crate::coding_flow::CodingHub`] to the local
//! per-install `settings.json` — never synced. This pane and the Agents pane
//! share that ONE settings struct but own DISJOINT fields: `drafted` overlays
//! only this pane's fields onto its synced baseline, `save` overlays them
//! onto the hub's LIVE settings (a Tools save can never roll back an Agents
//! save), and `resync` adopts unowned-field changes into the baseline first
//! so a sibling save never wipes edits in flight here.

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};

use coding::Settings;

use crate::coding_flow::CodingHub;

use super::{card_header, error_notice, section};

pub struct ToolsPane {
    repos_input: Entity<InputState>,
    prefix_input: Entity<InputState>,
    /// EXP-288: the shell new `+` terminal tabs spawn; blank = auto
    /// (the placeholder shows the detected platform default).
    shell_input: Entity<InputState>,
    /// The hub settings the controls were last synced from (dirty baseline).
    synced: Option<Settings>,
    save_error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl ToolsPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let repos_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_REPOS_ROOT));
        let prefix_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(coding::settings::DEFAULT_BRANCH_PREFIX)
        });
        let shell_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(terminal::manager::default_shell()));

        let hub = CodingHub::global(cx);
        let mut subscriptions = vec![cx.observe_in(&hub, window, |this, _, window, cx| {
            this.resync(window, cx);
            cx.notify();
        })];
        for input in [&repos_input, &prefix_input, &shell_input] {
            subscriptions.push(cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify(); // live dirty tracking on the Save button
                }
            }));
        }

        let mut this = Self {
            repos_input,
            prefix_input,
            shell_input,
            synced: None,
            save_error: None,
            _subscriptions: subscriptions,
        };
        this.resync(window, cx);
        this
    }

    /// Mirror the hub's settings into the controls whenever they change out
    /// from under us. Unowned fields (agents, rail…) are adopted into the
    /// baseline FIRST, so a sibling pane's save never wipes edits here; only
    /// owned-field changes rewrite the inputs.
    fn resync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();
        if let Some(synced) = self.synced.as_mut() {
            let owned = (
                synced.repos_root.clone(),
                synced.branch_prefix.clone(),
                synced.terminal_shell.clone(),
            );
            *synced = settings.clone();
            synced.repos_root = owned.0;
            synced.branch_prefix = owned.1;
            synced.terminal_shell = owned.2;
        }
        if self.synced.as_ref() == Some(&settings) {
            return;
        }
        self.repos_input.update(cx, |input, cx| {
            input.set_value(settings.repos_root.clone(), window, cx)
        });
        self.prefix_input.update(cx, |input, cx| {
            input.set_value(settings.branch_prefix.clone(), window, cx)
        });
        self.shell_input.update(cx, |input, cx| {
            input.set_value(settings.terminal_shell.clone().unwrap_or_default(), window, cx)
        });
        self.synced = Some(settings);
        cx.notify();
    }

    /// The settings the controls currently describe: the synced baseline with
    /// ONLY this pane's fields overlaid. Blank repos/prefix degrade to the
    /// §7.7 defaults (mirrors `Settings::load`); a blank shell is `None`
    /// (= auto), NOT a default program.
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
        drafted.repos_root = value(&self.repos_input, &defaults.repos_root);
        drafted.branch_prefix = value(&self.prefix_input, &defaults.branch_prefix);
        let shell = self.shell_input.read(cx).value().trim().to_string();
        drafted.terminal_shell = (!shell.is_empty()).then_some(shell);
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
        // save here can never roll back a concurrent Agents-pane save.
        let mut settings = hub.read(cx).settings.clone();
        settings.repos_root = drafted.repos_root;
        settings.branch_prefix = drafted.branch_prefix;
        settings.terminal_shell = drafted.terminal_shell;
        self.save_error = CodingHub::save_settings(&hub, settings.clone(), cx)
            .err()
            .map(SharedString::from);
        // `synced` follows the hub via the observer's resync; setting it here
        // too keeps the Save button honest when the observer coalesces.
        self.synced = Some(settings);
        cx.notify();
    }

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
}

impl Render for ToolsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let dirty = self.dirty(cx);

        let card = section(cx)
            .child(card_header(
                "Tools",
                "Local per-machine settings — never synced.",
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
            .child(Self::labeled_input(
                "Terminal shell",
                "Program for new terminal tabs (launched as a login shell). Leave empty to \
                 use your system default.",
                &self.shell_input,
                cx,
            ));

        let mut save_area = v_flex().gap_2();
        if let Some(error) = &self.save_error {
            save_area = save_area.child(error_notice(error.clone(), cx));
        }
        save_area = save_area.child(
            h_flex().justify_end().child(
                Button::new("tools-save")
                    .primary()
                    .small()
                    .label("Save changes")
                    .disabled(!dirty)
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            ),
        );

        v_flex().w_full().gap_6().child(card).child(save_area)
    }
}
