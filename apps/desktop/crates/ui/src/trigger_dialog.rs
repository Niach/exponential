//! The "New trigger" / "Edit trigger" dialog (EXP-583; SLOP-2: a trigger
//! lives on its action, so there is no action to pick) — the web trigger form
//! 1:1: pick WHEN it fires, pick the machine that runs it, optionally pin
//! account/model/effort. Saving is ONE `actions.update({id, triggers})`, a
//! whole-array replace ([`crate::trigger_editor`]'s write shapes); no run is
//! started — the bound device watches its own synced rows and fires by
//! itself.
//!
//! The whole form is [`TriggerEditorState`]; this file is its window, its
//! footer and the write.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, size, App, AppContext as _, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Disableable as _,
};

use coding::automations::ActionTrigger;

use crate::controls::WebControl as _;
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;
use crate::trigger_editor::{self, TriggerEditorState};

/// Open the dialog for a NEW trigger on `action_id` (the Triggers section's
/// owner-only "Add trigger"). A no-op when the action isn't synced.
pub(crate) fn open_new(window: &mut Window, cx: &mut App, action_id: String) {
    open_inner(window, cx, action_id, None)
}

/// Open the dialog on an EXISTING trigger (its row, or the row's ⋯ → Edit).
/// A no-op when the action or the trigger isn't synced (racing a delete).
pub(crate) fn open_edit(window: &mut Window, cx: &mut App, action_id: String, trigger_id: String) {
    let Some(trigger) = trigger_editor::action_triggers(&action_id, cx)
        .into_iter()
        .find(|trigger| trigger.id == trigger_id)
    else {
        return;
    };
    open_inner(window, cx, action_id, Some(trigger))
}

fn open_inner(
    window: &mut Window,
    cx: &mut App,
    action_id: String,
    existing: Option<ActionTrigger>,
) {
    let Some(team_id) = sync::Store::try_global(cx).and_then(|store| {
        store
            .collections()
            .actions
            .read(cx)
            .get(&action_id)
            .and_then(|row| row.team_id.clone())
    }) else {
        return;
    };
    let editing = existing.is_some();
    let height = (window.viewport_size().height * 0.85).min(px(480.));
    let spec = DialogSpec::new(
        if editing { "Edit trigger" } else { "New trigger" },
        size(px(520.), height),
    )
    .resizable(size(px(420.), px(360.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            TriggerDialogView::new(team_id.clone(), action_id.clone(), existing.clone(), window, cx)
        });
        let busy = view.clone();
        DialogContent::new(view)
            // The view pins its own footer and scrolls the form — the shell's
            // wrapper would scroll the confirm button out of reach.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).submitting)
    });
}

struct TriggerDialogView {
    action_id: String,
    /// `Some` = editing that trigger (its id and its on/off flag are kept);
    /// `None` = adding one.
    existing: Option<ActionTrigger>,
    editor: TriggerEditorState,
    submitting: bool,
    error: Option<SharedString>,
    scroll: ScrollHandle,
    /// EXP-721: the `devices` shape can land AFTER the dialog opened — the
    /// seeds re-run on every delta so the runner and the account row are
    /// never left empty by a race.
    _subscriptions: Vec<gpui::Subscription>,
}

impl TriggerDialogView {
    fn new(
        team_id: String,
        action_id: String,
        existing: Option<ActionTrigger>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let mut editor = TriggerEditorState::new(team_id, window, cx);
        match &existing {
            Some(trigger) => {
                editor.seed_trigger(Some(&trigger.raw), window, cx);
                editor.seed_runner(
                    Some(&trigger.device_id),
                    trigger.pins.agent.as_deref(),
                    trigger.pins.account.as_deref(),
                    trigger.pins.model.as_deref(),
                    trigger.pins.effort.as_deref(),
                );
                // A trigger saved before EXP-615 may carry no agent — the
                // row has no "Device default" pick anymore, so seed it.
                editor.ensure_agent_seeded(cx);
            }
            // One capable machine = no pick to make.
            None => editor.seed_default_device(cx),
        }
        // EXP-721: opening the dialog before the `devices` shape has landed
        // left the runner unbound AND the account row unlit forever — the
        // seeds are idempotent (a pick the user already made is never
        // disturbed), so re-running them on every delta is safe.
        let editing = existing.is_some();
        let subscriptions = match sync::Store::try_global(cx) {
            Some(store) => {
                let devices = store.collections().devices.clone();
                vec![cx.observe(&devices, move |this: &mut Self, _, cx| {
                    if editing {
                        this.editor.ensure_agent_seeded(cx);
                    } else {
                        this.editor.seed_default_device(cx);
                    }
                    cx.notify();
                })]
            }
            None => Vec::new(),
        };
        Self {
            action_id,
            existing,
            editor,
            submitting: false,
            error: None,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.submitting {
            return;
        }
        let spec = match self.editor.to_spec(cx) {
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
        // The array a save replaces is the action's CURRENT one, read at
        // submit time — a teammate's edit synced while this dialog was open
        // must ride along, not be overwritten by a stale snapshot (nor a
        // write of our own whose echo is still on the way).
        let current = trigger_editor::action_triggers(&self.action_id, cx);
        let triggers = match &self.existing {
            Some(existing) => {
                // The on/off flag is the row switch's — an edit never flips it.
                let enabled = current
                    .iter()
                    .find(|trigger| trigger.id == existing.id)
                    .map_or(existing.enabled, |trigger| trigger.enabled);
                let element = trigger_editor::trigger_element(&spec, Some(&existing.id), enabled);
                trigger_editor::triggers_with_replaced(&current, &existing.id, element)
            }
            // A new trigger is born ON — it only exists to fire.
            None => trigger_editor::triggers_with_added(
                &current,
                trigger_editor::trigger_element(&spec, None, true),
            ),
        };
        self.submitting = true;
        self.error = None;
        cx.notify();

        let mut input = api::actions::ActionUpdate::new(self.action_id.clone());
        input.triggers = Some(triggers);
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::actions::update(&trpc, &input) })
                .await;
            let _ = this.update_in(window, |view, window, cx| match result {
                // The synced echo repaints the Triggers rows; until it lands
                // the returned array is the next write's base.
                Ok(action) => {
                    trigger_editor::note_triggers_written(&action);
                    native_dialog::close_dialog_window(window, cx)
                }
                Err(err) => {
                    view.submitting = false;
                    view.error = Some(err.user_message().into());
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for TriggerDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let danger = cx.theme().danger;
        let form = self
            .editor
            .render("trigger-dialog", |this: &mut Self| &mut this.editor, window, cx);

        let footer = h_flex()
            .flex_shrink_0()
            .justify_end()
            .gap_2()
            .child(
                Button::new("trigger-cancel")
                    .ghost()
                    .cursor_pointer()
                    .web_sm()
                    .label("Cancel")
                    .disabled(self.submitting)
                    .on_click(|_, window, cx| native_dialog::close_dialog_window(window, cx)),
            )
            .child(
                Button::new("trigger-save")
                    .primary()
                    .cursor_pointer()
                    .web_sm()
                    .label(if self.submitting {
                        "Saving…"
                    } else if self.existing.is_some() {
                        "Save changes"
                    } else {
                        "Add trigger"
                    })
                    .disabled(self.submitting)
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
                "trigger-dialog-scroll",
                &self.scroll,
                v_flex().child(form).pr_2().pb_2(),
            ))
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(danger).child(error))
            })
            .child(footer.pt_3().border_t_1().border_color(cx.theme().border))
    }
}
