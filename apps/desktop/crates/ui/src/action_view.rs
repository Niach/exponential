//! ONE action as a page (SLOP-2) — the web `t/$teamSlug/actions/$actionId`
//! page: the action's prompt, its triggers and its runs. The desktop is a
//! WIDE layout, so the three read as SECTIONS of one scroller (a group band
//! over flat hairline-divided rows), in this order and titled exactly
//! `Prompt`, `Triggers`, `Runs`; phones draw the same three as tabs.
//!
//! It replaced two things: the "Edit action" dialog (its fields are the
//! Prompt section, [`crate::action_prompt_form`]) and the Automations screen
//! (an automation IS a trigger of its action now, and "Recent automated runs"
//! is this action's Runs). Triggers are LOCAL-ONLY: there is no server
//! scheduler — the bound device selects its enabled triggers off the synced
//! `actions` rows and self-starts ([`crate::automation_host`]). This page is
//! the owner's editing surface plus the answer to "did they fire?".
//!
//! Every trigger write is ONE `actions.update({id, triggers})`, a whole-array
//! replace ([`crate::trigger_editor`]'s write shapes) built off the action's
//! CURRENT triggers — the synced row's, or the last write's while its echo is
//! still on the way. One write at a time: the rows' controls wait for it.
//!
//! EXP-832: `render` builds elements and NOTHING else — the joins (devices,
//! pins, the run tree) happen once per data change in [`ActionView::refresh`].

use std::collections::HashSet;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, AppContext as _, ClickEvent, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant},
    menu::{DropdownMenu as _, PopupMenuItem},
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use coding::automations::ActionTrigger;

use crate::action_prompt_form::ActionPromptForm;
use crate::actions_view::page_scaffold;
use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{nav_for_window, resolved_screen, Navigation, Screen};
use crate::queries;
use crate::run_rows;
use crate::surface::{glass_section_header, list_row};
use crate::trigger_editor::{self, TRIGGER_REQUIRED_INPUTS_HINT};

/// How many runs the Runs section lists before it stops (newest first) — the
/// page is not virtualised, and a daily trigger alone adds a row a day.
const RUNS_CAP: usize = 100;

pub struct ActionView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// The action on screen; `None` until the first [`Screen::Action`].
    action_id: Option<String>,
    /// The Prompt section — built once the action's row is synced, rebuilt
    /// when the page is pointed at another action.
    prompt: Option<Entity<ActionPromptForm>>,
    /// EXP-897: the parent runs whose child runs are folded away, by tree
    /// node key (the sessions lists' rule). Per view, never persisted.
    collapsed: HashSet<String>,
    derived: ActionDerived,
    /// A refused trigger write (the server's own sentence), shown under the
    /// Triggers rows until the next write.
    trigger_error: Option<SharedString>,
    /// A trigger write is in flight — the switches and the ⋯ menus wait, so
    /// two whole-array writes never overlap.
    trigger_writing: bool,
    _subscriptions: Vec<Subscription>,
    /// The Runs rows carry relative times and a liveness that expires, and a
    /// trigger's device dot follows the clock — the sessions lists' 5s tick.
    _tick: Task<()>,
}

/// The page's data, ready to draw.
#[derive(Default)]
struct ActionDerived {
    /// The action's synced row; `None` while it has not landed — or once it
    /// is gone ([`Self::ready`] tells the two apart).
    action: Option<api::actions::Action>,
    /// The `actions` shape's readiness.
    ready: bool,
    /// One per READABLE trigger, in stored order, with its joins done.
    triggers: Vec<TriggerRow>,
    /// The action's runs as the session TREE every session list draws.
    runs: Vec<RunRow>,
}

/// One Triggers row: the trigger plus everything the row would otherwise
/// look up per render. Only the device's online dot (it follows the clock)
/// is left to `render`.
#[derive(Clone, Debug, PartialEq)]
struct TriggerRow {
    trigger: ActionTrigger,
    /// `Daily at 09:00 (device time)`, `When an issue is created`, …
    sentence: String,
    /// The bound device's label — a device that isn't in this user's synced
    /// rows (a teammate's private machine) keeps its raw id.
    device_label: String,
    device_last_seen_at: Option<String>,
    /// `Claude Code · opus` when the trigger pinned anything.
    pins: Option<String>,
}

/// One Runs row: a run or a group row, plus its place in the session tree
/// (EXP-818/EXP-897/EXP-1061 — nested ×4, folded by its node key).
#[derive(Clone, Debug, PartialEq)]
struct RunRow {
    key: String,
    depth: usize,
    has_children: bool,
    /// A run, titled by what started it ([`run_rows::action_run_title`]).
    facts: run_rows::RunListFacts,
}

impl ActionDerived {
    fn compute(cx: &App, action_id: Option<&str>) -> Self {
        let (Some(action_id), Some(store)) = (action_id, sync::Store::try_global(cx)) else {
            return Self::default();
        };
        let collections = store.collections().clone();
        let actions = collections.actions.read(cx);
        let ready = actions.is_ready();
        let Some(row) = actions.get(action_id) else {
            return Self { ready, ..Self::default() };
        };
        let devices = collections.devices.read(cx);
        let triggers = coding::automations::parse_action_triggers(row.triggers.as_ref())
            .into_iter()
            .map(|trigger| {
                let device = devices
                    .iter()
                    .find(|device| device.device_id.as_deref() == Some(trigger.device_id.as_str()));
                TriggerRow {
                    sentence: trigger_editor::trigger_sentence(&trigger.when),
                    device_label: device
                        .and_then(|device| device.label.clone())
                        .unwrap_or_else(|| trigger.device_id.clone()),
                    device_last_seen_at: device.and_then(|device| device.last_seen_at.clone()),
                    pins: launch_pins_label(&trigger),
                    trigger,
                }
            })
            .collect();

        let now = chrono::Utc::now().timestamp();
        let runs = queries::action_runs(cx, action_id);
        // The cap is a RUN cap, applied BEFORE the tree (EXP-1061): a child
        // whose parent fell off it is a top-level orphan. Then the ONE tree
        // every session list draws.
        let mut listed: Vec<&domain::rows::CodingSession> = runs.iter().collect();
        listed.truncate(RUNS_CAP);
        let runs = crate::sessions_section::flatten_session_tree(listed, |node| {
            let session: &domain::rows::CodingSession = node.session();
            // Every row here ran THIS action — the title says what started
            // it instead of repeating the action's name.
            run_rows::RunListFacts::derive(session, now, cx).with_title(
                run_rows::action_run_title(session.started_reason.as_deref()),
            )
        })
        .into_iter()
        .map(|row| RunRow {
            key: row.key,
            depth: row.depth,
            has_children: row.has_children,
            facts: row.run,
        })
        .collect();

        Self {
            action: Some(api::actions::from_row(row)),
            ready,
            triggers,
            runs,
        }
    }
}

/// `Claude Code · opus` — the agent and model a trigger pinned, or `None`
/// when it follows the bound machine's own launch defaults (the common case).
fn launch_pins_label(trigger: &ActionTrigger) -> Option<String> {
    let agent = trigger.pins.agent.as_deref().map(|agent| {
        coding::CodingAgent::parse(agent)
            .map(|known| known.label().to_string())
            // An agent this build predates still names itself.
            .unwrap_or_else(|| agent.to_string())
    });
    let parts: Vec<String> = [agent, trigger.pins.model.clone()]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

impl ActionView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions =
            vec![cx.observe_in(&nav, window, |this, _, window, cx| this.sync_screen(window, cx))];
        // The rows join actions (the row + its triggers), devices (label +
        // online dot) and coding_sessions (the Runs), so all three drive it.
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (
                collections.actions.clone(),
                collections.devices.clone(),
                collections.coding_sessions.clone(),
                collections.pins.clone(),
            )
        });
        if let Some((actions, devices, sessions, pins)) = watched {
            subscriptions.push(cx.observe_in(&actions, window, |this, _, window, cx| {
                this.on_actions_changed(window, cx)
            }));
            subscriptions.push(cx.observe(&devices, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&sessions, |this, _, cx| this.refresh(cx)));
            // EXP-778: the header's pin toggle reads the per-user pins rows.
            subscriptions.push(cx.observe(&pins, |_, _, cx| cx.notify()));
        }
        // EXP-874: the live run rows' Merge circles paint the shared
        // two-click state.
        let merge_state = crate::pr_merge::MergeState::global(cx);
        subscriptions.push(cx.observe(&merge_state, |_, _, cx| cx.notify()));
        let mut this = Self {
            nav,
            scroll: ScrollHandle::new(),
            action_id: None,
            prompt: None,
            collapsed: HashSet::new(),
            derived: ActionDerived::default(),
            trigger_error: None,
            trigger_writing: false,
            _subscriptions: subscriptions,
            _tick: crate::sessions_section::tick(cx, |this: &mut Self, cx| this.tick_refresh(cx)),
        };
        this.sync_screen(window, cx);
        this
    }

    /// Follow the window's screen: a [`Screen::Action`] naming another action
    /// re-points the page (a fresh Prompt form, the scroll back at the top).
    fn sync_screen(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(Screen::Action { action_id }) = resolved_screen(&self.nav, cx) else {
            return;
        };
        if self.action_id.as_deref() == Some(action_id.as_str()) {
            return;
        }
        self.action_id = Some(action_id);
        self.prompt = None;
        self.collapsed.clear();
        self.trigger_error = None;
        self.scroll = ScrollHandle::new();
        self.derived = ActionDerived::compute(cx, self.action_id.as_deref());
        self.ensure_prompt(window, cx);
        cx.notify();
    }

    /// Build the Prompt form once the action's row is synced (a deep link
    /// can land here before the `actions` shape does).
    fn ensure_prompt(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.prompt.is_some() {
            return;
        }
        let Some(action) = self.derived.action.clone() else {
            return;
        };
        self.prompt = Some(cx.new(|cx| ActionPromptForm::new(action, window, cx)));
    }

    /// The `actions` rows moved: re-derive, build the form if the row just
    /// landed — and leave for the list if the action on screen was DELETED
    /// (by this page's own menu, or by a teammate).
    fn on_actions_changed(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let had_action = self.derived.action.is_some();
        self.refresh(cx);
        self.ensure_prompt(window, cx);
        let on_screen = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::Action { action_id }) if Some(&action_id) == self.action_id.as_ref()
        );
        // Only a READY shape can say "gone" — a refetch empties it in between.
        if had_action && self.derived.action.is_none() && self.derived.ready && on_screen {
            crate::navigation::go_back_to(window, cx, Screen::Actions);
        }
    }

    /// EXP-897: the folded parents of the Runs (the shared
    /// [`crate::sessions_section::Collapsible`] contract).
    pub(crate) fn collapsed_runs_mut(&mut self) -> &mut HashSet<String> {
        &mut self.collapsed
    }

    /// Re-derive the page off the collections and repaint — the ONE place
    /// the rows are computed.
    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.derived = ActionDerived::compute(cx, self.action_id.as_deref());
        cx.notify();
    }

    /// The clock-driven refresh: repaint only when the Runs moved. The
    /// triggers' device dots are read off the clock in `render`, so a quiet
    /// page repaints for them at this cadence only while it has triggers.
    fn tick_refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let next = ActionDerived::compute(cx, self.action_id.as_deref());
        if next.runs != self.derived.runs || !self.derived.triggers.is_empty() {
            self.derived = next;
            cx.notify();
        }
    }

    // -- writes ---------------------------------------------------------------

    /// ONE `actions.update({id, triggers})` — the whole array. A refusal
    /// (required inputs, an incapable device, …) lands under the rows in the
    /// server's own words; the synced echo repaints everything else.
    fn write_triggers(&mut self, triggers: Vec<serde_json::Value>, cx: &mut gpui::Context<Self>) {
        let (Some(action_id), Some(trpc)) = (self.action_id.clone(), queries::trpc_client(cx))
        else {
            return;
        };
        if self.trigger_writing {
            return;
        }
        self.trigger_writing = true;
        self.trigger_error = None;
        cx.notify();
        let mut input = api::actions::ActionUpdate::new(action_id);
        input.triggers = Some(triggers);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::actions::update(&trpc, &input) })
                .await;
            // The returned array is the next write's base until the synced
            // row shows it — recorded even if the page is gone by now.
            if let Ok(action) = &result {
                trigger_editor::note_triggers_written(action);
            }
            let _ = this.update(cx, |this, cx| {
                this.trigger_writing = false;
                if let Err(err) = result {
                    log::warn!("actions: writing the triggers failed: {err}");
                    this.trigger_error = Some(err.user_message().into());
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Flip one trigger's switch. ONLY its `enabled` moves, so toggling can
    /// never move the when-part's fingerprint and re-seed the host's state.
    fn set_trigger_enabled(&mut self, trigger_id: &str, enabled: bool, cx: &mut gpui::Context<Self>) {
        let Some(action_id) = self.action_id.clone() else {
            return;
        };
        let current = trigger_editor::action_triggers(&action_id, cx);
        self.write_triggers(
            trigger_editor::triggers_with_enabled(&current, trigger_id, enabled),
            cx,
        );
    }

    fn delete_trigger(&mut self, trigger_id: &str, cx: &mut gpui::Context<Self>) {
        let Some(action_id) = self.action_id.clone() else {
            return;
        };
        let current = trigger_editor::action_triggers(&action_id, cx);
        self.write_triggers(trigger_editor::triggers_without(&current, trigger_id), cx);
    }

    /// Destructive native actions confirm first (the machines Remove pattern).
    fn prompt_delete_trigger(
        &mut self,
        trigger_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let spec = AlertSpec::new(
            "Delete trigger?",
            "It stops firing. Past runs stay in Runs.".to_string(),
            "Delete",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            if let Some(view) = view.upgrade() {
                view.update(cx, |this, cx| this.delete_trigger(&trigger_id, cx));
            }
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// The web delete dialog's copy behind the shared alert window. The page
    /// leaves for the list once the synced row is gone
    /// ([`Self::on_actions_changed`]).
    fn prompt_delete_action(
        &mut self,
        action_id: String,
        name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let spec = AlertSpec::new(
            "Delete action",
            format!(
                "Delete \"{name}\"? Live runs keep going and keep their label; \
                 this cannot be undone."
            ),
            "Delete",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            crate::actions_view::spawn_action_delete(cx, action_id.clone());
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    // -- render ---------------------------------------------------------------

    /// The header: back · the action's glyph + name · pin · the owner's ⋯
    /// (Delete) · Run.
    fn render_header(
        &self,
        action: &api::actions::Action,
        is_owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let foreground = theme.foreground;

        // EXP-367: no agent CLI → Run disabled with the reason, never hidden.
        let no_agent = crate::coding_flow::no_agent_reason(cx);
        let run_id = action.id.clone();
        let run = crate::controls::glass_icon_button(
            "action-page-run",
            Icon::from(registry::ACTION_RUN),
            cx,
        )
        .tooltip(no_agent.clone().unwrap_or_else(|| "Run".into()))
        .disabled(no_agent.is_some())
        // EXP-825: the composer with this action as the subject chip.
        .on_click(move |_: &ClickEvent, window, cx| {
            crate::navigation::navigate_to_chat(
                window,
                cx,
                crate::navigation::ChatSeed::action(run_id.clone()),
            );
        });

        let menu = is_owner.then(|| {
            let view = cx.entity().downgrade();
            let delete_id = action.id.clone();
            let delete_name = action.name.clone();
            crate::controls::ghost_icon_button("action-page-menu", Icon::from(registry::UI_MORE), cx)
                .dropdown_menu(move |menu, _window, cx| {
                    let view = view.clone();
                    let delete_id = delete_id.clone();
                    let delete_name = delete_name.clone();
                    menu.item(
                        crate::controls::danger_menu_item(
                            "Delete",
                            Icon::from(registry::UI_DELETE),
                            cx,
                        )
                        .on_click(move |_, window, cx| {
                            let Some(view) = view.upgrade() else {
                                return;
                            };
                            let (id, name) = (delete_id.clone(), delete_name.clone());
                            view.update(cx, |this, cx| {
                                this.prompt_delete_action(id, name, window, cx);
                            });
                        }),
                    )
                })
        });

        gpui_component::h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_2()
            .child(
                crate::controls::ghost_icon_button(
                    "action-page-back",
                    Icon::from(registry::UI_BACK),
                    cx,
                )
                .tooltip("Back to actions")
                .on_click(|_: &ClickEvent, window, cx| {
                    crate::navigation::go_back_to(window, cx, Screen::Actions);
                }),
            )
            .child(
                crate::icons::action_icon(action.icon.as_deref())
                    .small()
                    .flex_shrink_0()
                    .text_color(muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_base()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(foreground)
                    .child(SharedString::from(action.name.clone())),
            )
            // EXP-778: the personal pin toggle.
            .child(crate::pins::pin_toggle_button(
                "action-page-pin",
                action.team_id.clone(),
                domain::contract::PIN_KIND_ACTION,
                action.id.clone(),
                cx,
            ))
            .children(menu)
            .child(run)
            .into_any_element()
    }

    /// One trigger row: glyph · [sentence, device + pins, the locked hint] ·
    /// the enabled switch · the owner ⋯ menu.
    fn render_trigger_row(
        &self,
        index: usize,
        row: &TriggerRow,
        now_ms: i64,
        is_owner: bool,
        blocked_by_inputs: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let foreground = theme.foreground;
        // EXP-811: the ONE row hover, `list_hover` (glass fillRow) on every client.
        let row_hover = theme.list_hover;
        let trigger = &row.trigger;
        let Some(action_id) = self.action_id.clone() else {
            return div().into_any_element();
        };
        // The dot follows the clock: a machine goes offline WITHOUT a row
        // change, so this is the one join left to the render.
        let device_online = crate::device_settings::row_is_online(
            row.device_last_seen_at.as_deref(),
            now_ms,
        );
        // A triggered run has nobody to fill required inputs, so the server
        // refuses to ENABLE such a trigger — but one that is already on must
        // stay switchable OFF.
        let locked = blocked_by_inputs && !trigger.enabled;

        let mut meta = gpui_component::h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_1p5()
            .text_xs()
            .text_color(muted)
            .child(crate::surface::live_dot(
                if device_online {
                    theme::tokens::GREEN.to_hsla()
                } else {
                    muted.opacity(0.4)
                },
                false,
            ))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(row.device_label.clone())),
            );
        // The pins, when the trigger set any — otherwise the run follows the
        // machine's own launch defaults and there is nothing to say.
        if let Some(pins) = row.pins.clone() {
            meta = meta
                .child(div().flex_shrink_0().child("·"))
                .child(div().min_w_0().truncate().child(SharedString::from(pins)));
        }

        let mut middle = gpui_component::v_flex()
            .flex_1()
            .min_w_0()
            .gap_0p5()
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .truncate()
                    .text_color(foreground)
                    .child(SharedString::from(row.sentence.clone())),
            )
            .child(meta);
        if locked {
            middle = middle.child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_xs()
                    .text_color(muted)
                    .child(TRIGGER_REQUIRED_INPUTS_HINT),
            );
        }

        let toggle_id = trigger.id.clone();
        let mut switch = crate::controls::web_switch(("trigger-enabled", index))
            .checked(trigger.enabled)
            // Owner-only per the permissions model: members SEE the state (a
            // disabled switch), owners flip it.
            // … and one write at a time: the next starts from this one's result.
            .disabled(!is_owner || locked || self.trigger_writing)
            .on_click(cx.listener(move |this, on: &bool, _, cx| {
                this.set_trigger_enabled(&toggle_id, *on, cx);
            }));
        if locked {
            switch = switch.tooltip(TRIGGER_REQUIRED_INPUTS_HINT);
        }

        let edit_action = action_id.clone();
        let edit_trigger = trigger.id.clone();
        let element = crate::surface::flat_row()
            .id(SharedString::from(format!("trigger-row-{}", trigger.id)))
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2p5()
            // The row's click is the trigger form — the same destination its
            // ⋯ menu has, under the same owner gate. A member has no form to
            // open, so the row stays inert.
            .when(is_owner, |this| {
                this.hover(move |this| this.bg(row_hover))
                    .cursor_pointer()
                    .on_click(move |_: &ClickEvent, window, cx| {
                        crate::trigger_dialog::open_edit(
                            window,
                            cx,
                            edit_action.clone(),
                            edit_trigger.clone(),
                        );
                    })
            })
            .child(
                Icon::from(if trigger.is_schedule() {
                    registry::TRIGGER_SCHEDULE
                } else {
                    registry::TRIGGER_EVENT
                })
                .xsmall()
                .flex_shrink_0()
                .text_color(foreground.opacity(0.7)),
            )
            .child(middle)
            .child(
                // The switch and the menu are their own targets; the row's
                // click must not fire underneath them (web parity).
                gpui_component::h_flex()
                    .id(("trigger-row-controls", index))
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
                    .child(switch)
                    .children(is_owner.then(|| self.render_trigger_menu(index, &action_id, trigger, cx))),
            );
        list_row(element, index).into_any_element()
    }

    /// The row's owner ⋯ menu: Edit (the trigger form) / Delete (confirmed).
    fn render_trigger_menu(
        &self,
        index: usize,
        action_id: &str,
        trigger: &ActionTrigger,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let action_id = action_id.to_string();
        let trigger_id = trigger.id.clone();
        let view = cx.entity().downgrade();
        div()
            .flex_shrink_0()
            .child(
                // EXP-862: a row's "..." is a GHOST glyph, never a circle.
                crate::controls::ghost_icon_button(
                    ("trigger-menu", index),
                    Icon::from(registry::UI_MORE),
                    cx,
                )
                .disabled(self.trigger_writing)
                .dropdown_menu(move |menu, _window, cx| {
                    let (edit_action, edit_trigger) = (action_id.clone(), trigger_id.clone());
                    let delete_view = view.clone();
                    let delete_id = trigger_id.clone();
                    menu.item(
                        PopupMenuItem::new("Edit")
                            .icon(Icon::from(registry::UI_EDIT))
                            .on_click(move |_, window, cx| {
                                crate::trigger_dialog::open_edit(
                                    window,
                                    cx,
                                    edit_action.clone(),
                                    edit_trigger.clone(),
                                );
                            }),
                    )
                    .item(
                        crate::controls::danger_menu_item(
                            "Delete",
                            Icon::from(registry::UI_DELETE),
                            cx,
                        )
                        .on_click(move |_, window, cx| {
                            let Some(view) = delete_view.upgrade() else {
                                return;
                            };
                            let id = delete_id.clone();
                            view.update(cx, |this, cx| {
                                this.prompt_delete_trigger(id, window, cx);
                            });
                        }),
                    )
                }),
            )
            .into_any_element()
    }

    /// The Triggers section: the band (owners: "Add trigger") over one row
    /// per readable trigger, in stored order.
    fn render_triggers(
        &self,
        action: &api::actions::Action,
        is_owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let blocked_by_inputs = action.inputs.iter().any(|input| input.required);

        // Owner-only. No agent gate: authoring a trigger starts nothing.
        let add = is_owner.then(|| {
            let action_id = action.id.clone();
            crate::surface::glass_pill_button("triggers-add", crate::surface::PillSize::Sm, cx)
                .label("Add trigger")
                .on_click(move |_, window, cx| {
                    crate::trigger_dialog::open_new(window, cx, action_id.clone());
                })
                .into_any_element()
        });
        let mut section = gpui_component::v_flex()
            .min_w_0()
            .child(glass_section_header("Triggers", add, cx));
        if self.derived.triggers.is_empty() {
            section = section.child(
                div()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(muted)
                    .child("No triggers. This action runs when someone starts it."),
            );
        } else {
            let rows: Vec<gpui::AnyElement> = self
                .derived
                .triggers
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    self.render_trigger_row(index, row, now_ms, is_owner, blocked_by_inputs, cx)
                })
                .collect();
            section = section.children(rows);
        }
        section
            .when_some(self.trigger_error.clone(), |this, error| {
                this.child(div().px_3().pt_2().text_sm().text_color(danger).child(error))
            })
            .into_any_element()
    }

    /// The Runs section: every run of this action, newest first, as the
    /// session tree, each titled by what started it.
    fn render_runs(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut section = gpui_component::v_flex()
            .min_w_0()
            .child(glass_section_header("Runs", None, cx));
        if self.derived.runs.is_empty() {
            return section
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(muted)
                        .child("No runs yet."),
                )
                .into_any_element();
        }
        // EXP-897: everything under a folded node leaves the list.
        let visible = crate::sessions_section::drop_collapsed(
            self.derived.runs.iter().collect::<Vec<_>>(),
            &self.collapsed,
            |row| row.key.as_str(),
            |row| row.depth,
        );
        // EXP-965: the connector every nested row draws, off the VISIBLE
        // depth sequence (a folded subtree is not part of the tree on screen).
        let guides = domain::tree_guides::guides_for(
            &visible.iter().map(|row| row.depth).collect::<Vec<_>>(),
        );
        for (index, row) in visible.into_iter().enumerate() {
            let fold = crate::sessions_section::fold_for(
                row.key.clone(),
                row.has_children,
                &self.collapsed,
                cx,
            );
            let guides = guides.get(index).cloned().unwrap_or_default();
            let open_id = row.facts.session_id().to_string();
            let element = run_rows::render_run_list_row(
                "action-run",
                index,
                guides,
                fold,
                row.facts.clone(),
                false,
                // This page is context-free (no list to pin the run beside),
                // so the run opens over the rail and its Back — the history —
                // returns here.
                Box::new(move |_, window, cx| {
                    crate::session_screen::open_session(&open_id, window, cx);
                }),
                cx,
            );
            section = section.child(list_row(element, index));
        }
        section.into_any_element()
    }

    /// The page while its action is not there: still syncing, or gone.
    fn render_missing(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        if !self.derived.ready {
            return div()
                .text_sm()
                .text_color(muted)
                .child("Loading…")
                .into_any_element();
        }
        gpui_component::v_flex()
            .items_start()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("This action no longer exists."),
            )
            .child(
                Button::new("action-page-missing-back")
                    .outline()
                    .cursor_pointer()
                    .web_sm()
                    .label("Back to actions")
                    .on_click(|_, window, cx| {
                        crate::navigation::go_back_to(window, cx, Screen::Actions);
                    }),
            )
            .into_any_element()
    }
}

impl Render for ActionView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(action) = self.derived.action.clone() else {
            let missing = self.render_missing(cx);
            return page_scaffold(
                "action-screen-scroll",
                &self.scroll,
                gpui_component::v_flex().child(missing),
            )
            .into_any_element();
        };
        let is_owner = crate::settings::is_owner(cx, &action.team_id);

        let header = self.render_header(&action, is_owner, cx);
        // NO gap on a headed section (EXP-697): the band's own margin IS the
        // spacing to what sits under it.
        let prompt = gpui_component::v_flex()
            .min_w_0()
            .child(glass_section_header("Prompt", None, cx))
            .children(self.prompt.clone().map(|form| div().pt_1().child(form)));
        let triggers = self.render_triggers(&action, is_owner, cx);
        let runs = self.render_runs(cx);

        page_scaffold(
            "action-screen-scroll",
            &self.scroll,
            gpui_component::v_flex()
                .gap_6()
                .child(header)
                .child(prompt)
                .child(triggers)
                .child(runs),
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn trigger(element: serde_json::Value) -> ActionTrigger {
        coding::automations::parse_action_trigger(&element).expect("a readable trigger")
    }

    /// Line 2 of a Triggers row names the pinned agent and model — and says
    /// nothing when the trigger follows the device's own defaults.
    #[test]
    fn launch_pins_label_names_the_agent_and_the_model() {
        let when = json!({"id": "t-1", "deviceId": "d-1",
                          "kind": "schedule", "interval": "daily", "minuteOfDay": 540});
        assert_eq!(launch_pins_label(&trigger(when.clone())), None);

        let mut pinned = when.clone();
        pinned["agent"] = json!("claude");
        pinned["model"] = json!("opus");
        // The effort is a launch detail the row leaves out.
        pinned["effort"] = json!("high");
        let claude = coding::CodingAgent::parse("claude").expect("contract agent").label();
        assert_eq!(
            launch_pins_label(&trigger(pinned)),
            Some(format!("{claude} · opus"))
        );

        // An agent this build predates still names itself.
        let mut future = when;
        future["agent"] = json!("moonshot");
        assert_eq!(launch_pins_label(&trigger(future)), Some("moonshot".to_string()));
    }
}
