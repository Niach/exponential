//! Settings → Tools (EXP-288 — renamed from "Coding" and slimmed to the
//! non-agent, this-device knobs; the per-agent settings live in
//! [`super::agents`]).
//!
//! | Section        | Meaning                                              |
//! |----------------|------------------------------------------------------|
//! | Repos root     | Where repositories/worktrees live (`~` works)        |
//! | Branch prefix  | Prepended to the issue identifier (`exp/EXP-42`)     |
//! | Terminal shell | Program new `+` terminal tabs spawn (blank = auto)   |
//! | Tooling doctor | The shared [`super::doctor_section::DoctorPanel`]    |
//! |                | (EXP-367 — moved here from Agents; also the wizard's |
//! |                | tools step)                                          |
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
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};

use coding::Settings;

use crate::coding_flow::CodingHub;
use crate::native_dialog::{self, AlertSpec};

use super::doctor_section::DoctorPanel;
use super::{card_header, card_title, error_notice, section};

pub struct ToolsPane {
    repos_input: Entity<InputState>,
    prefix_input: Entity<InputState>,
    /// EXP-288: the shell new `+` terminal tabs spawn; blank = auto
    /// (the placeholder shows the detected platform default).
    shell_input: Entity<InputState>,
    /// The shared tooling doctor (EXP-367 — also the onboarding tools step).
    doctor: Entity<DoctorPanel>,
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
        let doctor = cx.new(|cx| DoctorPanel::new(window, cx));

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
            doctor,
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

    /// The "Reset IDE data" confirm (EXP-367, built for testing fresh-install
    /// flows): destructive-local-only, so a plain danger confirm suffices —
    /// everything server-side survives and re-syncs on the next sign-in.
    /// EXP-369: the reset now also wipes the clones, so the confirm names the
    /// repos root — the only IDE data that is expensive to recreate.
    fn confirm_reset(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let repos_root = CodingHub::global(cx).read(cx).settings.repos_root.clone();
        let spec = AlertSpec::new(
            "Reset IDE data",
            format!(
                "This signs you out on this device and deletes ALL local IDE data: \
                 settings, accounts, and synced caches. Cloned repositories and \
                 worktrees under {repos_root} are deleted too, including any \
                 uncommitted work in them. The app restarts onto the sign-in screen."
            ),
            "Reset and restart",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            crate::session::reset_ide_data(cx);
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    fn labeled_input(
        label: &'static str,
        input: &Entity<InputState>,
        cx: &App,
    ) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(label))
            .child(Input::new(input).small())
    }
}

impl Render for ToolsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let dirty = self.dirty(cx);

        let card = section(cx)
            .child(card_header(
                "Tools",
                "Local per-machine settings, never synced.",
                cx,
            ))
            .child(Self::labeled_input(
                "Repos & worktrees root",
                &self.repos_input,
                cx,
            ))
            .child(Self::labeled_input("Branch prefix", &self.prefix_input, cx))
            .child(Self::labeled_input("Terminal shell", &self.shell_input, cx));

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

        // EXP-367: local-only destructive hatch for testing fresh-install
        // flows (login, onboarding wizard, tools setup).
        let danger = section(cx)
            .child(card_title("Danger zone"))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Sign out and delete all local settings, accounts, caches, \
                                 and cloned repositories.",
                            ),
                    )
                    .child(
                        Button::new("tools-reset-ide")
                            .danger()
                            .small()
                            .label("Reset IDE data")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm_reset(window, cx);
                            })),
                    ),
            );

        v_flex()
            .w_full()
            .gap_6()
            .child(card)
            .child(save_area)
            .child(self.doctor.clone())
            .child(danger)
    }
}
