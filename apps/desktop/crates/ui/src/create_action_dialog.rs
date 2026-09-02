//! The **New action** dialog (EXP-431, its own module since EXP-615) — the
//! web `create-action-dialog.tsx` twin.
//!
//! Authoring an action is a RUN: the dialog fills the hidden "Create action"
//! builtin's inputs (description · name · repository · icon) and starts it
//! through [`crate::action_run::start_action_run`], exactly like any other
//! action run. Nothing is written to the server here — the agent authors the
//! action with `exponential_actions_create`.
//!
//! Layout (web parity, EXP-694 grouped): icon picker + Name are ONE row of a
//! [`crate::surface::glass_group`] with the Description textarea under them —
//! placeholder-titled, no labels above — then a second group holding the
//! optional Repository and an ALWAYS-visible Automation row summarising
//! what will be bound ("No automation" when nothing is). Clicking it slides
//! the shared [`crate::automation_editor`] section over the form inside the
//! SAME frame — one fixed dialog size, no resize between the two halves. The
//! right column is the ONE shared
//! [`crate::launch_options::LaunchOptionsSection`].
//!
//! The automation cannot be SAVED here (the action does not exist yet):
//! [`trigger_note`] appends the wire JSON to the creator run's description
//! with an explicit instruction to pass it to `exponential_automations_create`
//! once the action is created — byte-identical to what the web appends.

use gpui::{
    div, px, size, AnyWindowHandle, App, AppContext as _, ClickEvent, Entity,
    InteractiveElement as _, IntoElement, ParentElement, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use coding::{ActionInputValue, LaunchOrigin};

use crate::action_run::{self, ActionRepo, ActionRepoRow, StartActionArgs};
use crate::controls::WebControl as _;
use crate::automation_editor::{automation_devices, AutomationEditorState, AutomationSpec};
use crate::coding_flow::CodingHub;
use crate::icons::registry;
use crate::launch_options::LaunchOptionsSection;
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

/// Append the machine-readable automation instruction to the creator run's
/// `description` input (EXP-530/583), value AND display alike — the prompt
/// renders the display, the value is what the agent is told it received.
/// Compact JSON so the whole thing stays one readable line.
///
/// The wording is cross-client copy: the web create dialog appends the exact
/// same block ([`trigger_note`] is locked by
/// `trigger_note_matches_the_web_block`).
fn append_trigger_note(inputs: &mut [ActionInputValue], spec: &AutomationSpec) {
    let note = trigger_note(spec);
    for input in inputs.iter_mut() {
        if input.key != "description" {
            continue;
        }
        input.value.push_str(&note);
        if let Some(display) = input.display.as_mut() {
            display.push_str(&note);
        }
    }
}

/// The appended block itself (see [`append_trigger_note`]). Byte-identical to
/// web `formatAutomationBlock` (`lib/action-triggers.ts`), key order included:
/// `deviceId`, `trigger`, then the launch pins only when they are set. The
/// dialog never talks to the server — the creator agent copies this JSON into
/// `exponential_automations_create` once the action exists.
fn trigger_note(spec: &AutomationSpec) -> String {
    let mut payload = serde_json::Map::new();
    payload.insert("deviceId".to_string(), serde_json::json!(spec.device_id));
    payload.insert("trigger".to_string(), spec.trigger.clone());
    for (key, value) in [
        ("agent", spec.agent.as_deref()),
        ("model", spec.model.as_deref()),
        ("effort", spec.effort.as_deref()),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            payload.insert(key.to_string(), serde_json::json!(value));
        }
    }
    let json = serde_json::to_string(&serde_json::Value::Object(payload))
        .unwrap_or_else(|_| "{}".to_string());
    format!(
        "\n\nAutomation — after creating the action, call \
         exponential_automations_create with its id and exactly these fields: \
         `{json}`. An automated run fills no inputs, so declare none as required."
    )
}

/// Open the dialog empty (the Actions header's "New action" button).
pub fn open(window: &mut Window, cx: &mut App, team_id: String) {
    open_prefilled(window, cx, team_id, None, None, None);
}

/// Open it with a Suggestions-tab seed (EXP-530): a ready-written brief, its
/// glyph, and — for an "Action + automation" suggestion — the trigger the
/// creator agent should bind. Everything stays editable; nothing is authored
/// until the run does it.
pub fn open_prefilled(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    description: Option<String>,
    icon: Option<String>,
    automation: Option<serde_json::Value>,
) {
    // The launched terminal tab lands back in the OPENER window (the dialog
    // is its own native window — EXP-284).
    let opener = window.window_handle();
    // FIXED size per open: the automation detail replaces the form INSIDE
    // this frame, so the window must not resize between the two halves.
    let height = (window.viewport_size().height * 0.85).min(px(520.));
    let spec =
        DialogSpec::new("New action", size(px(720.), height)).resizable(size(px(600.), px(460.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            CreateActionDialogView::new(team_id, description, icon, automation, opener, window, cx)
        });
        // No busy gate: the submit hands off to the runner and closes in the
        // same frame — this dialog is never mid-flight.
        DialogContent::new(view)
            // The view pins its own action bar and scrolls only the body.
            .self_scrolling()
    });
}

/// Which half of the ONE frame is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pane {
    Form,
    Automation,
}

pub struct CreateActionDialogView {
    team_id: String,
    opener: AnyWindowHandle,
    pane: Pane,
    name: Entity<InputState>,
    description: Entity<TextareaState>,
    /// The curated glyph the authored action lands with (EXP-273).
    icon: Option<String>,
    repo: Option<ActionRepoRow>,
    team_repos: Vec<ActionRepoRow>,
    /// The automation the creator agent should bind after authoring — see the
    /// module docs; `automation_set` is what the summary row reflects.
    automation: AutomationEditorState,
    automation_set: bool,
    launch: LaunchOptionsSection,
    body_scroll: ScrollHandle,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl CreateActionDialogView {
    fn new(
        team_id: String,
        description: Option<String>,
        icon: Option<String>,
        automation_trigger: Option<serde_json::Value>,
        opener: AnyWindowHandle,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // Both placeholders come from the builtin's own input definitions, so
        // they can never drift from the other three clients.
        let inputs = api::actions::builtin_create_action(&team_id).inputs;
        let placeholder = |key: &str| {
            inputs
                .iter()
                .find(|input| input.key == key)
                .and_then(|input| input.placeholder.clone())
                .unwrap_or_default()
        };
        let name = cx.new(|cx| {
            InputState::new(window, cx).placeholder(SharedString::from(placeholder("name")))
        });
        let description_state = cx.new(|cx| {
            let mut state = crate::controls::web_textarea(4, 12, window, cx)
                .placeholder(SharedString::from(placeholder("description")));
            if let Some(seed) = description.as_ref() {
                state.set_value(seed.clone(), window, cx);
            }
            state
        });

        let mut automation = AutomationEditorState::new(team_id.clone(), window, cx);
        if let Some(trigger) = automation_trigger.as_ref() {
            automation.seed_trigger(Some(trigger), window, cx);
            automation.seed_default_device(cx);
        }

        let hub = CodingHub::global(cx);
        let subscriptions = vec![
            // Doctor lands / re-runs → the footer gate moves AND the agent
            // strip re-filters to the installed agents (EXP-206).
            cx.observe_in(&hub, window, |this: &mut Self, _, window, cx| {
                this.launch.reconcile_agent(window, cx);
                cx.notify();
            }),
            // The footer gate ("Fill in Description.") re-evaluates per
            // keystroke.
            cx.subscribe(&description_state, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];

        let mut this = Self {
            team_id,
            opener,
            pane: Pane::Form,
            name,
            description: description_state,
            icon,
            repo: None,
            team_repos: Vec::new(),
            automation,
            automation_set: automation_trigger.is_some(),
            launch: LaunchOptionsSection::new(window, cx),
            body_scroll: ScrollHandle::new(),
            error: None,
            _subscriptions: subscriptions,
        };
        this.fetch_team_repos(cx);
        this.launch.reconcile_agent(window, cx);
        this
    }

    /// Prefetch `repositories.list` for the repo picker. Best-effort: a failed
    /// fetch just leaves the (optional) picker empty.
    fn fetch_team_repos(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let team_id = self.team_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { action_run::fetch_repositories(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(rows) => {
                    this.team_repos = rows;
                    cx.notify();
                }
                Err(err) => log::warn!("[ui] repositories.list failed: {err}"),
            });
        })
        .detach();
    }

    /// The summary the always-visible Automation row shows: the trigger
    /// sentence · the bound machine — or "No automation".
    fn automation_summary(&self, cx: &App) -> SharedString {
        if !self.automation_set {
            return "No automation".into();
        }
        let mut parts: Vec<String> = Vec::new();
        if let Ok(trigger) = self.automation.to_trigger(cx) {
            if let Some(parsed) = crate::automation_editor::parsed_trigger(Some(&trigger)) {
                parts.push(coding::automations::trigger_summary(&parsed));
            }
        }
        if let Some(device_id) = self.automation.device_id.as_deref() {
            let label = automation_devices(cx)
                .into_iter()
                .find(|device| device.device_id == device_id)
                .map(|device| device.label)
                .unwrap_or_else(|| device_id.to_string());
            parts.push(label);
        }
        if parts.is_empty() {
            return "No automation".into();
        }
        SharedString::from(parts.join(" · "))
    }

    /// Why the Create button is disabled right now; `None` = launchable.
    fn launch_blocker(&self, cx: &mut App) -> Option<SharedString> {
        match CodingHub::global(cx).read(cx).doctor.report.as_ref() {
            None => return Some("Checking local tools…".into()),
            // Per-agent gate (EXP-201): only git + the SELECTED agent block.
            Some(report) => {
                if let Some(failed) = report.first_failure_for(self.launch.agent) {
                    return Some(
                        failed
                            .error
                            .clone()
                            .unwrap_or_else(|| format!("{} is not available", failed.tool))
                            .into(),
                    );
                }
            }
        }
        if self.description.read(cx).value().trim().is_empty() {
            return Some("Fill in Description.".into());
        }
        None
    }

    /// Start the creator run: the builtin's inputs, the automation block
    /// appended to the brief, then the shared runner (which spawns the tab in
    /// the OPENER window). The dialog closes on hand-off — the runner
    /// surfaces any failure on the window itself.
    fn launch(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.launch_blocker(cx).is_some() {
            return;
        }
        let description = self.description.read(cx).value().trim().to_string();
        let name = self.name.read(cx).value().trim().to_string();
        let action = api::actions::builtin_create_action(&self.team_id);
        let mut inputs: Vec<ActionInputValue> = Vec::new();
        for input in &action.inputs {
            let value = match input.key.as_str() {
                "description" => description.clone(),
                "name" => name.clone(),
                "repo" => self
                    .repo
                    .as_ref()
                    .map(|repo| repo.id.clone())
                    .unwrap_or_default(),
                "icon" => self.icon.clone().unwrap_or_default(),
                _ => String::new(),
            };
            if value.is_empty() {
                continue; // empty optionals are omitted, definition order kept
            }
            let display = match input.key.as_str() {
                "repo" => self
                    .repo
                    .as_ref()
                    .map(|repo| repo.full_name.clone())
                    .unwrap_or_else(|| value.clone()),
                _ => value.clone(),
            };
            inputs.push(ActionInputValue {
                key: input.key.clone(),
                label: input.label.clone(),
                input_type: input.input_type.clone(),
                value,
                display: Some(display),
            });
        }
        // A half-filled automation blocks the launch rather than starting a
        // run that silently drops it.
        if self.automation_set {
            match self.automation.to_spec(cx) {
                Err(message) => {
                    self.error = Some(message);
                    cx.notify();
                    return;
                }
                Ok(spec) => append_trigger_note(&mut inputs, &spec),
            }
        }
        let options = self.launch.options(false, cx);
        let handle = self.opener;
        native_dialog::close_dialog_window(window, cx);
        action_run::start_action_run(
            StartActionArgs {
                action_id: action.id,
                team_id: self.team_id.clone(),
                // The creator always runs repo-less (its repo INPUT only pins
                // the AUTHORED action's binding) — the runner enforces that.
                repo: ActionRepo::Resolve,
                options,
                origin: LaunchOrigin::Local,
                inputs,
                target: Some(handle),
                activate_app: false,
                reservation: None,
                // A person clicked Create — never an automation firing.
                trigger: None,
                automation_id: None,
                on_failed: None,
            },
            cx,
        );
    }

    // -- render ---------------------------------------------------------------

    /// The always-visible Automation row: glyph · "Automation" + summary ·
    /// chevron. Clicking it opens the detail (and, on the first open, adopts
    /// the single automation-capable machine as the runner). EXP-694: a row
    /// OF the repository group (the web `GlassGroup` twin), so the stateful
    /// row rides inside a plain wrapper the group can divide.
    fn automation_row(&self, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let theme = cx.theme();
        let foreground = theme.foreground;
        let hover = theme.list_active.opacity(0.5);
        let summary = self.automation_summary(cx);
        div().w_full().child(
            crate::surface::glass_row_shell()
                .id("ca-automation-row")
                .cursor_pointer()
                .hover(move |this| this.bg(hover))
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    if !this.automation_set {
                        this.automation_set = true;
                        this.automation.seed_default_device(cx);
                    }
                    this.pane = Pane::Automation;
                    cx.notify();
                }))
                .child(
                    Icon::from(registry::ACTION_AUTOMATION)
                        .xsmall()
                        .text_color(foreground.opacity(0.5)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().text_color(foreground).child("Automation"))
                        .child(
                            div()
                                .text_xs()
                                .truncate()
                                .text_color(foreground.opacity(0.5))
                                .child(summary),
                        ),
                )
                .child(
                    Icon::from(registry::UI_CHEVRON_RIGHT)
                        .xsmall()
                        .text_color(foreground.opacity(0.5)),
                ),
        )
    }

    /// The detail pane's header: back · "Automation" · Remove automation.
    fn automation_header(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                // EXP-698: the one 32px glass chrome every detail-header action
                // wears.
                crate::controls::glass_icon_button(
                    "ca-automation-back",
                    Icon::from(registry::UI_CHEVRON_LEFT),
                    cx,
                )
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.pane = Pane::Form;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child("Automation"),
            )
            .child(
                crate::surface::glass_pill_button("ca-automation-remove", crate::surface::PillSize::Sm, cx)
                    .label("Remove automation")
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.automation_set = false;
                        this.pane = Pane::Form;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// The left form column: icon + Name · Description · Repository ·
    /// Automation row.
    fn form_column(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let icon_picked = self.icon.clone();
        let icon_picker = crate::board_form::icon_picker(
            "ca-icon",
            icon_picked.as_deref(),
            true,
            {
                let view = cx.entity().downgrade();
                move |name, _, cx| {
                    if let Some(view) = view.upgrade() {
                        let name = name.map(str::to_string);
                        view.update(cx, |view, cx| {
                            view.icon = name;
                            cx.notify();
                        });
                    }
                }
            },
            cx,
        );
        let repo_row = action_run::repo_picker_row(
            "Repository",
            "ca-repo".into(),
            self.repo.as_ref(),
            self.team_repos.clone(),
            true,
            |this: &mut Self, repo, cx| {
                this.repo = repo;
                cx.notify();
            },
            cx,
        );
        // EXP-694: the same grouped controls as the edit dialog — icon + Name
        // are ONE row, the description is a chrome-less textarea whose
        // placeholder (the builtin's own input definition) is its title.
        let name_row = crate::surface::glass_row_shell().child(icon_picker).child(
            div().flex_1().min_w_0().child(
                Input::new(&self.name)
                    .appearance(false)
                    .h_auto()
                    .px_0()
                    .py_0(),
            ),
        );
        let description_row = div().w_full().child(
            Textarea::new(&self.description)
                .appearance(false)
                .w_full()
                .px_4()
                .py_3(),
        );
        v_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(crate::surface::glass_group_rows(vec![
                name_row,
                description_row,
            ]))
            .child(crate::surface::glass_group_rows(vec![
                repo_row,
                self.automation_row(cx),
            ]))
            .into_any_element()
    }

    /// Footer: blocker copy + Cancel + Create — pinned to the dialog's bottom
    /// edge in BOTH panes (the web dialog keeps its footer across the slide).
    fn footer(&self, blocker: Option<SharedString>, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let mut footer = h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(cx.theme().border);
        if let Some(reason) = &blocker {
            footer = footer.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .truncate()
                    .text_color(cx.theme().muted_foreground)
                    .child(reason.clone()),
            );
        }
        footer
            .child(div().flex_1())
            .child(
                Button::new("ca-cancel")
                    .outline()
                    .cursor_pointer()
                    .web_sm()
                    .label("Cancel")
                    .on_click(cx.listener(|_, _, window, cx| {
                        native_dialog::close_dialog_window(window, cx);
                    })),
            )
            .child(
                Button::new("ca-create")
                    .primary()
                    .cursor_pointer()
                    .web_sm()
                    .icon(Icon::from(registry::ACTION_CREATE))
                    .label("Create")
                    .disabled(blocker.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.launch(window, cx))),
            )
    }
}

impl Render for CreateActionDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let blocker = self.launch_blocker(cx);
        let body = match self.pane {
            Pane::Form => v_flex().w_full().gap_3().child(
                h_flex()
                    .w_full()
                    .gap_5()
                    .items_start()
                    .child(self.form_column(cx))
                    .child(v_flex().flex_1().min_w_0().child(self.launch.render(
                        "ca-launch",
                        |this: &mut Self| &mut this.launch,
                        None,
                        false,
                        cx,
                    ))),
            ),
            // The detail REPLACES the form inside the same fixed frame — the
            // web's slide-over, without the transform.
            Pane::Automation => v_flex()
                .w_full()
                .gap_3()
                .child(self.automation_header(cx))
                .child(self.automation.render_with_heading(
                    "ca-automation",
                    |this: &mut Self| &mut this.automation,
                    // The back-button header says "Automation"; the section
                    // label below it says "Trigger", exactly like web.
                    true,
                    cx,
                )),
        };
        let body = match &self.error {
            Some(error) => body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            ),
            None => body,
        };

        let body_scroll = self.body_scroll.clone();
        v_flex()
            .size_full()
            .gap_3()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .id("ca-body-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&body_scroll)
                            .child(body),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(Scrollbar::new(&body_scroll).axis(ScrollbarAxis::Vertical)),
                    ),
            )
            .child(self.footer(blocker, cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The machine-readable block is cross-client copy — byte-locked against
    /// web `formatAutomationBlock` (`lib/action-triggers.ts`) and the mobile
    /// mirrors, including the compact `JSON.stringify` value form and its key
    /// order (`deviceId`, `trigger`, then only the pins that are set).
    #[test]
    fn trigger_note_matches_the_web_block() {
        let note = trigger_note(&AutomationSpec {
            trigger: serde_json::json!({
                "kind": "schedule",
                "interval": "daily",
                "minuteOfDay": 420,
            }),
            device_id: "d".to_string(),
            agent: None,
            model: None,
            effort: None,
        });
        assert_eq!(
            note,
            "\n\nAutomation — after creating the action, call \
             exponential_automations_create with its id and exactly these fields: \
             `{\"deviceId\":\"d\",\"trigger\":\
             {\"kind\":\"schedule\",\"interval\":\"daily\",\"minuteOfDay\":420}}`. \
             An automated run fills no inputs, so declare none as required."
        );

        // The pins ride AFTER the trigger, in agent/model/effort order, and
        // only when set — an empty pin is omitted, never sent as "".
        let pinned = trigger_note(&AutomationSpec {
            trigger: serde_json::json!({"kind": "event", "event": "created"}),
            device_id: "d".to_string(),
            agent: Some("codex".to_string()),
            model: None,
            effort: Some("high".to_string()),
        });
        assert!(
            pinned.contains(
                "`{\"deviceId\":\"d\",\"trigger\":{\"kind\":\"event\",\"event\":\"created\"},\
                 \"agent\":\"codex\",\"effort\":\"high\"}`"
            ),
            "{pinned}"
        );
    }

    /// EXP-615: the creator run fills the builtin's inputs by KEY — the
    /// dialog's fields and the shipped definition must stay in lockstep (a
    /// renamed key would silently drop the value from the run).
    #[test]
    fn the_dialog_fills_every_builtin_input_key() {
        let builtin = api::actions::builtin_create_action("team-1");
        let keys: Vec<&str> = builtin
            .inputs
            .iter()
            .map(|input| input.key.as_str())
            .collect();
        assert_eq!(keys, vec!["description", "name", "repo", "icon"]);
    }
}
