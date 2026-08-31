//! The "New automation" / "Edit automation" dialog (EXP-583) — the web
//! automation form 1:1: pick the ACTION, pick the TRIGGER, pick the machine
//! that runs it, optionally pin agent/model/effort. One
//! `automations.create` / `automations.update` on Save.
//!
//! The action picker offers CUSTOM actions only, and only the ones with no
//! REQUIRED input: an automated run has nobody to type them, and the server
//! refuses such a target outright ([`AUTOMATION_REQUIRED_INPUTS_HINT`]) — so
//! the dialog never offers a pick that cannot be saved. The builtins
//! ("Create action", "Fix merge conflicts") are excluded the same way the
//! server excludes them: they aren't DB rows and can never be automated.
//!
//! Everything below the action picker is [`AutomationEditorState`], shared
//! with the suggestion-prefilled create flow in
//! [`crate::start_coding_dialog`].

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, size, App, AppContext as _, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Disableable as _,
};

use crate::automation_editor::{AutomationEditorState, AUTOMATION_REQUIRED_INPUTS_HINT};
use crate::controls::WebControl as _;
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

/// Open the dialog for a NEW automation on `team_id` (the Automations band's
/// owner-only "New automation" button).
pub(crate) fn open_new(window: &mut Window, cx: &mut App, team_id: String) {
    open_inner(window, cx, team_id, None)
}

/// Open the dialog on an EXISTING automation (the row's ⋯ → Edit). A no-op
/// when the row isn't synced (racing a delete).
pub(crate) fn open_edit(window: &mut Window, cx: &mut App, automation_id: String) {
    let Some(store) = sync::Store::try_global(cx) else {
        return;
    };
    let Some(automation) = store
        .collections()
        .automations
        .read(cx)
        .get(&automation_id)
        .map(api::automations::from_row)
    else {
        return;
    };
    let team_id = automation.team_id.clone();
    open_inner(window, cx, team_id, Some(automation))
}

fn open_inner(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    existing: Option<api::automations::Automation>,
) {
    let editing = existing.is_some();
    let height = (window.viewport_size().height * 0.85).min(px(480.));
    let spec = DialogSpec::new(
        if editing {
            "Edit automation"
        } else {
            "New automation"
        },
        size(px(520.), height),
    )
    .resizable(size(px(420.), px(360.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| AutomationDialogView::new(team_id.clone(), existing.clone(), window, cx));
        let busy = view.clone();
        DialogContent::new(view)
            // The view pins its own footer and scrolls the form — the shell's
            // wrapper would scroll "Save" out of reach.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).submitting)
    });
}

struct AutomationDialogView {
    team_id: String,
    /// `Some` = editing that row; `None` = creating.
    automation_id: Option<String>,
    /// The trigger this row synced with — an UNSUPPORTED shape (a newer
    /// client's kind) blocks the save instead of being overwritten.
    synced_trigger: Option<serde_json::Value>,
    action_id: Option<String>,
    automation: AutomationEditorState,
    submitting: bool,
    error: Option<SharedString>,
    scroll: ScrollHandle,
}

impl AutomationDialogView {
    fn new(
        team_id: String,
        existing: Option<api::automations::Automation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let mut automation = AutomationEditorState::new(team_id.clone(), window, cx);
        match &existing {
            Some(row) => {
                automation.seed_trigger(row.trigger.as_ref(), window, cx);
                automation.seed_runner(
                    Some(&row.device_id),
                    row.agent.as_deref(),
                    row.model.as_deref(),
                    row.effort.as_deref(),
                );
                // A row saved before EXP-615 may carry a NULL agent — the
                // strip has no "Device default" pill anymore, so seed it.
                automation.ensure_agent_seeded(cx);
            }
            // One capable machine = no pick to make.
            None => automation.seed_default_device(cx),
        }
        Self {
            team_id,
            automation_id: existing.as_ref().map(|row| row.id.clone()),
            synced_trigger: existing.as_ref().and_then(|row| row.trigger.clone()),
            action_id: existing.as_ref().map(|row| row.action_id.clone()),
            automation,
            submitting: false,
            error: None,
            scroll: ScrollHandle::new(),
        }
    }

    /// The automatable actions: this team's CUSTOM rows with no required
    /// input. Builtins are excluded — they are client-constructed, not DB
    /// rows, and the server rejects them as automation targets.
    fn automatable_actions(&self, cx: &App) -> Vec<api::actions::Action> {
        let (actions, _) = queries::team_actions(cx, &self.team_id);
        actions
            .into_iter()
            .filter(|action| !action.builtin && !api::actions::is_builtin_action_id(&action.id))
            .filter(|action| !action.inputs.iter().any(|input| input.required))
            .collect()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.submitting {
            return;
        }
        let Some(action_id) = self.action_id.clone() else {
            self.error = Some("Pick an action to automate.".into());
            cx.notify();
            return;
        };
        let spec = match self.automation.to_spec(cx) {
            Ok(spec) => spec,
            Err(message) => {
                self.error = Some(message);
                cx.notify();
                return;
            }
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };
        self.submitting = true;
        self.error = None;
        cx.notify();

        let existing = self.automation_id.clone();
        let team_id = self.team_id.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    match existing {
                        Some(id) => {
                            let mut input = api::automations::AutomationUpdate::new(id);
                            input.action_id = Some(action_id);
                            input.device_id = Some(spec.device_id);
                            input.trigger = Some(spec.trigger);
                            // Tri-state: clearing a pin means "follow the
                            // device's launch defaults" — never "unchanged".
                            input.agent = api::Patch::set_or_null(spec.agent);
                            input.model = api::Patch::set_or_null(spec.model);
                            input.effort = api::Patch::set_or_null(spec.effort);
                            api::automations::update(&trpc, &input).map(|_| ())
                        }
                        None => api::automations::create(
                            &trpc,
                            &api::automations::AutomationCreate {
                                team_id,
                                action_id,
                                device_id: spec.device_id,
                                trigger: spec.trigger,
                                // A new automation is born ON — the row only
                                // exists to fire.
                                enabled: None,
                                agent: spec.agent,
                                model: spec.model,
                                effort: spec.effort,
                            },
                        )
                        .map(|_| ()),
                    }
                })
                .await;
            let _ = this.update_in(window, |view, window, cx| match result {
                // The synced echo repaints the list — nothing to gate on.
                Ok(()) => native_dialog::close_dialog_window(window, cx),
                Err(err) => {
                    view.submitting = false;
                    view.error = Some(err.user_message().into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The action picker as ONE grouped row (EXP-694): "Action" leading, the
    /// bound action trailing behind a caret — the S2 rhythm every editor
    /// control on every client now shares. With nothing automatable to pick,
    /// the row carries the hint on its second line instead of a control.
    fn render_action_picker(&self, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let actions = self.automatable_actions(cx);
        let foreground = cx.theme().foreground;
        let label: SharedString = match &self.action_id {
            Some(action_id) => actions
                .iter()
                .find(|action| &action.id == action_id)
                .map(|action| SharedString::from(action.name.clone()))
                // The bound action carries a required input now, or hasn't
                // synced — keep the binding visible instead of blanking it.
                .unwrap_or_else(|| "Action".into()),
            None => "Select action…".into(),
        };
        if actions.is_empty() && self.action_id.is_none() {
            return crate::surface::glass_row_shell().child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(div().text_sm().text_color(foreground).child("Action"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(foreground.opacity(0.5))
                            .child(AUTOMATION_REQUIRED_INPUTS_HINT),
                    ),
            );
        }
        let picked = self.action_id.clone();
        let view = cx.entity().downgrade();
        let trigger = Button::new("automation-action")
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_0()
            .py_0()
            .text_color(foreground.opacity(0.7))
            .dropdown_caret(true)
            .label(label)
            .dropdown_menu(move |mut menu, _window, _cx| {
                for action in &actions {
                    let view = view.clone();
                    let action_id = action.id.clone();
                    let checked = picked.as_deref() == Some(action_id.as_str());
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(action.name.clone()))
                            .checked(checked)
                            .on_click(move |_, _, cx| {
                                if let Some(view) = view.upgrade() {
                                    let action_id = action_id.clone();
                                    view.update(cx, |this, cx| {
                                        this.action_id = Some(action_id);
                                        cx.notify();
                                    });
                                }
                            }),
                    );
                }
                menu
            });
        crate::surface::glass_picker_row("Action", None, trigger.into_any_element(), cx)
    }
}

impl Render for AutomationDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let danger = cx.theme().danger;
        // A trigger kind this build predates would be REWRITTEN by a save
        // (the section can only express what it can parse) — block it.
        let unsupported = AutomationEditorState::unsupported(self.synced_trigger.as_ref());
        let disabled = self.submitting || self.action_id.is_none() || unsupported;

        let form = v_flex()
            // EXP-694: groups stack at 8.
            .gap_2()
            .child(crate::surface::glass_group_rows(vec![
                self.render_action_picker(cx)
            ]))
            .child(
                self.automation
                    .render("automation-dialog", |this| &mut this.automation, cx),
            );

        let footer = h_flex()
            .flex_shrink_0()
            .justify_end()
            .gap_2()
            .child(
                Button::new("automation-cancel")
                    .ghost()
                    .cursor_pointer()
                    .web_sm()
                    .label("Cancel")
                    .disabled(self.submitting)
                    .on_click(|_, window, cx| native_dialog::close_dialog_window(window, cx)),
            )
            .child(
                Button::new("automation-save")
                    .primary()
                    .cursor_pointer()
                    .web_sm()
                    .label(if self.submitting {
                        "Saving…"
                    } else if self.automation_id.is_some() {
                        "Save changes"
                    } else {
                        "Create automation"
                    })
                    .disabled(disabled)
                    .loading(self.submitting)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            );

        v_flex()
            .size_full()
            .gap_3()
            // The pane must be a DIRECT flex item: `div()` defaults to
            // `Display::Block`, so an intermediate wrapper ignores the pane's
            // `flex_1` and its `size_full` scroll area resolves against an
            // indefinite height — the whole form collapses to nothing.
            .child(crate::scroll_pane::v_scroll_pane(
                "automation-dialog-scroll",
                &self.scroll,
                form.pr_2().pb_2(),
            ))
            .when(unsupported, |this| {
                this.child(div().text_sm().text_color(danger).child(
                    "This automation was set up by a newer version. Update the app to edit it.",
                ))
            })
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(danger).child(error))
            })
            .child(footer.pt_3().border_t_1().border_color(cx.theme().border))
    }
}
