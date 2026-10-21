//! The "New trigger" / "Edit trigger" dialog (EXP-583; SLOP-2; EXP-1294: a
//! trigger lives on its AGENT, so there is nothing to pick but the WHEN) —
//! opened from the agent settings page's Triggers rows. Saving is ONE
//! `agents.update({id, triggers})`, a whole-array replace
//! ([`crate::trigger_editor`]'s write shapes); no session is started —
//! NEW-2: the SERVER fires the agent's triggers.
//!
//! The whole form is [`TriggerEditorState`]; this file is its window, its
//! footer and the write. EXP-1319: the New agent page has no agent row yet,
//! so its Schedule edits a LOCAL array ([`DraftTriggers`]) the create sends.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    px, size, App, AppContext as _, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Disableable as _, ElementExt as _,
};
use serde_json::Value;
use std::cell::Cell;
use std::rc::Rc;

use crate::controls::WebControl as _;
use crate::native_dialog::{self, AlertSpec, DialogContent, DialogSpec};
use crate::queries;
use crate::trigger_editor::{self, TriggerEditorState};

/// EXP-1319: the New agent page's local trigger array — read at save time,
/// replaced whole on a save (nothing is written until the agent is created).
#[derive(Clone)]
pub(crate) struct DraftTriggers {
    pub(crate) team_id: String,
    pub(crate) read: Rc<dyn Fn(&App) -> Vec<Value>>,
    pub(crate) write: Rc<dyn Fn(Vec<Value>, &mut App)>,
}

/// Where a save lands: the agent definition (`agents.update`) or a draft.
#[derive(Clone)]
enum Target {
    Agent(String),
    Draft(DraftTriggers),
}

impl Target {
    fn current(&self, cx: &App) -> Vec<Value> {
        match self {
            Target::Agent(agent_id) => trigger_editor::agent_triggers(agent_id, cx),
            Target::Draft(draft) => (draft.read)(cx),
        }
    }
}

/// Open the dialog for a NEW trigger on the agent `agent_id`. A no-op when
/// the agent isn't synced.
pub(crate) fn open_new(window: &mut Window, cx: &mut App, agent_id: String) {
    open_inner(window, cx, Target::Agent(agent_id), None)
}

/// Open the dialog on an EXISTING trigger of the agent. A no-op when the
/// agent or the trigger isn't synced (racing a delete).
pub(crate) fn open_edit(window: &mut Window, cx: &mut App, agent_id: String, trigger_id: String) {
    open_existing(window, cx, Target::Agent(agent_id), trigger_id)
}

/// EXP-1319: a NEW trigger on the New agent page's draft.
pub(crate) fn open_new_draft(window: &mut Window, cx: &mut App, draft: DraftTriggers) {
    open_inner(window, cx, Target::Draft(draft), None)
}

/// EXP-1319: an existing trigger of the New agent page's draft.
pub(crate) fn open_edit_draft(window: &mut Window, cx: &mut App, draft: DraftTriggers, trigger_id: String) {
    open_existing(window, cx, Target::Draft(draft), trigger_id)
}

fn open_existing(window: &mut Window, cx: &mut App, target: Target, trigger_id: String) {
    let Some(trigger) = target
        .current(cx)
        .into_iter()
        .find(|trigger| trigger_editor::trigger_id(trigger) == Some(trigger_id.as_str()))
    else {
        return;
    };
    open_inner(window, cx, target, Some(trigger))
}

fn open_inner(window: &mut Window, cx: &mut App, target: Target, existing: Option<Value>) {
    let team_id = match &target {
        Target::Agent(agent_id) => crate::session_data::agent_def(agent_id, cx).and_then(|row| row.team_id),
        Target::Draft(draft) => Some(draft.team_id.clone()),
    };
    if team_id.is_none() {
        return;
    }
    let editing = existing.is_some();
    // The window opens at the cap and then FITS its form (P66): no slack
    // under a short schedule form, the form's own scroll past the cap.
    let max_height = window.viewport_size().height * 0.85;
    // NEW-2: the schedule picker = up to five rows under the strip.
    let height = max_height.min(px(460.));
    let spec = DialogSpec::new(
        if editing { "Edit trigger" } else { "New trigger" },
        size(px(520.), height),
    )
    .resizable(size(px(420.), px(320.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            let mut view = TriggerDialogView::new(target.clone(), existing.clone(), window, cx);
            view.max_height = max_height;
            view
        });
        let busy = view.clone();
        DialogContent::new(view)
            // The view pins its own footer and scrolls the form — the shell's
            // wrapper would scroll the confirm button out of reach.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).submitting)
    });
}

/// What a save writes.
#[derive(Clone)]
enum Write {
    /// The form's trigger, added or in place of the edited one.
    Save,
    /// The edited trigger, removed.
    Delete(String),
}

struct TriggerDialogView {
    target: Target,
    /// `Some` = editing that stored element (its id and its on/off flag are
    /// kept); `None` = adding one.
    existing: Option<Value>,
    editor: TriggerEditorState,
    submitting: bool,
    scroll: ScrollHandle,
    /// The window-height cap (85% of the opener's viewport).
    max_height: gpui::Pixels,
    /// The form's natural height and the scroll body's box, last frame (the
    /// window viewport minus the body = the chrome around it).
    content_h: Rc<Cell<gpui::Pixels>>,
    body_h: Rc<Cell<gpui::Pixels>>,
    /// The content height the window was last fitted to.
    fitted: Option<gpui::Pixels>,
}

impl TriggerDialogView {
    fn new(target: Target, existing: Option<Value>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut editor = TriggerEditorState::new(window, cx);
        if let Some(trigger) = &existing {
            editor.seed_trigger(trigger, window, cx);
        }
        Self {
            target,
            existing,
            editor,
            submitting: false,
            scroll: ScrollHandle::new(),
            max_height: px(400.),
            content_h: Rc::default(),
            body_h: Rc::default(),
            fitted: None,
        }
    }

    fn existing_id(&self) -> Option<String> {
        self.existing
            .as_ref()
            .and_then(trigger_editor::trigger_id)
            .map(str::to_string)
    }

    /// The prompts fixture's `delete-trigger` confirm, then the write.
    fn confirm_delete(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(id) = self.existing_id() else {
            return;
        };
        let dialog = cx.entity().downgrade();
        let handle = window.window_handle();
        let spec = AlertSpec::from_prompt("Delete trigger", &domain::prompts::delete_trigger())
            .on_ok(move |_, cx| {
                let id = id.clone();
                let dialog = dialog.clone();
                let _ = handle.update(cx, move |_, window, cx| {
                    let _ = dialog.update(cx, |view, cx| view.write(Write::Delete(id), window, cx));
                });
                true
            });
        native_dialog::open_alert(window, cx, spec);
    }

    fn write(&mut self, write: Write, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.submitting {
            return;
        }
        // NEW-2: an unsavable schedule (a weekly with no day, a custom cron
        // that is not five fields) never writes; a delete always may.
        if matches!(write, Write::Save) && !self.editor.errors(cx).is_empty() {
            return;
        }
        // The array a save replaces is the agent's CURRENT one, read at
        // submit time — a teammate's edit synced while this dialog was open
        // must ride along, not be overwritten by a stale snapshot (nor a
        // write of our own whose echo is still on the way).
        let current = self.target.current(cx);
        let when = self.editor.to_trigger(cx);
        let triggers = match (&write, self.existing_id()) {
            (Write::Delete(id), _) => trigger_editor::triggers_without(&current, id),
            (Write::Save, Some(id)) => {
                // The on/off flag is the row switch's — an edit never flips it.
                let enabled = current
                    .iter()
                    .find(|trigger| trigger_editor::trigger_id(trigger) == Some(id.as_str()))
                    .or(self.existing.as_ref())
                    .is_none_or(trigger_editor::trigger_enabled);
                let element = trigger_editor::agent_trigger_element(&when, Some(&id), enabled);
                trigger_editor::triggers_with_replaced(&current, &id, element)
            }
            // A new trigger is born ON — it only exists to fire. A draft's
            // carries its own id (the page edits and toggles it by id).
            (Write::Save, None) => {
                let id = matches!(self.target, Target::Draft(_))
                    .then(|| uuid::Uuid::new_v4().to_string());
                trigger_editor::triggers_with_added(
                    &current,
                    trigger_editor::agent_trigger_element(&when, id.as_deref(), true),
                )
            }
        };
        let agent_id = match &self.target {
            Target::Agent(agent_id) => agent_id.clone(),
            Target::Draft(draft) => {
                (draft.write)(triggers, cx);
                native_dialog::close_dialog_window(window, cx);
                return;
            }
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.submitting = true;
        cx.notify();

        let input = api::agents::UpdateInput {
            triggers: Some(triggers.clone()),
            ..api::agents::UpdateInput::new(agent_id.clone())
        };
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::agents::update(&trpc, &input) })
                .await;
            let _ = this.update_in(window, |view, window, cx| match result {
                // The synced echo repaints the Schedule rows; until it lands
                // the sent array is the next write's base.
                Ok(_) => {
                    trigger_editor::note_triggers_written(&agent_id, triggers);
                    native_dialog::close_dialog_window(window, cx)
                }
                // FEED-79: a failed save is the error toast; Retry writes the
                // same form again.
                Err(err) => {
                    view.submitting = false;
                    let dialog = cx.entity().downgrade();
                    let (verb, thing) = match write {
                        Write::Save => ("save", "the trigger"),
                        Write::Delete(_) => ("delete", "the trigger"),
                    };
                    crate::action_error::failed_action_with_retry(
                        verb,
                        thing,
                        crate::action_error::action_error_description(&err),
                        move |window, cx| {
                            let write = write.clone();
                            let _ = dialog.update(cx, |view, cx| view.write(write, window, cx));
                        },
                        cx,
                    );
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Size the WINDOW to the form (the chat dialog's EXP-1155 fit): its
    /// measured content plus the chrome around the scroll body, capped at
    /// [`Self::max_height`]. Refits only when the content height moved, so a
    /// manual drag holds; never touches a maximized or fullscreen window.
    fn fit_window(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let content = self.content_h.get();
        let body = self.body_h.get();
        if content <= px(0.) || body <= px(0.) || self.fitted == Some(content) {
            return;
        }
        if window.is_maximized() || window.is_fullscreen() {
            return;
        }
        self.fitted = Some(content);
        let current = window.viewport_size();
        let chrome = (current.height - body).max(px(0.));
        let target = (content + chrome).min(self.max_height.max(px(320.)));
        if (target - current.height).abs() <= px(1.) {
            return;
        }
        native_dialog::resize_dialog_keeping_top(window, cx, size(current.width, target));
    }
}

impl Render for TriggerDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.fit_window(window, cx);
        let view_id = cx.entity_id();
        let (content_slot, body_slot) = (self.content_h.clone(), self.body_h.clone());
        let form = self
            .editor
            .render("trigger-dialog", |this: &mut Self| &mut this.editor, window, cx);
        let editing = self.existing.is_some();
        let savable = self.editor.errors(cx).is_empty();

        let footer = h_flex()
            .flex_shrink_0()
            .gap_2()
            // An edit's Delete sits apart on the leading edge (the prompts
            // contract's quiet destructive answer), Cancel + Save trailing.
            .when(editing, |footer| {
                footer.child(
                    Button::new("trigger-delete")
                        .ghost()
                        .cursor_pointer()
                        .web_sm()
                        .text_color(cx.theme().danger)
                        .label("Delete")
                        .disabled(self.submitting)
                        .on_click(cx.listener(|this, _, window, cx| this.confirm_delete(window, cx))),
                )
            })
            .child(gpui::div().flex_1())
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
                    .label(SharedString::from(if self.submitting {
                        "Saving…"
                    } else if editing {
                        "Save changes"
                    } else {
                        "Add trigger"
                    }))
                    .disabled(self.submitting || !savable)
                    .loading(self.submitting)
                    .on_click(cx.listener(|this, _, window, cx| this.write(Write::Save, window, cx))),
            );

        v_flex()
            .size_full()
            .gap_3()
            // The pane must be a DIRECT flex item: `div()` defaults to
            // `Display::Block`, so an intermediate wrapper ignores the pane's
            // `flex_1` and its `size_full` scroll area resolves against an
            // indefinite height — the whole form collapses to nothing.
            .child(
                crate::scroll_pane::v_scroll_pane(
                    "trigger-dialog-scroll",
                    &self.scroll,
                    v_flex()
                        .w_full()
                        .flex_shrink_0()
                        .child(form)
                        .pr_2()
                        .pb_2()
                        .on_prepaint(move |bounds, _, cx| {
                            if content_slot.get() != bounds.size.height {
                                content_slot.set(bounds.size.height);
                                cx.notify(view_id);
                            }
                        }),
                )
                .on_prepaint(move |bounds, _, cx| {
                    if body_slot.get() != bounds.size.height {
                        body_slot.set(bounds.size.height);
                        cx.notify(view_id);
                    }
                }),
            )
            .child(footer.pt_3().border_t_1().border_color(cx.theme().border))
    }
}
