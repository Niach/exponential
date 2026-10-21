//! Settings → New agent (EXP-1319): the agent page in CREATE mode, the web's
//! `settings/agents/new` — every field of [`super::agent_detail`] (identity,
//! Device, Account, Schedule, Instructions), all LOCAL until "Create agent"
//! sends `sessions.send({teamId, deviceId, cli, account, agent})`; then the
//! page becomes the new agent's (`SettingsSection::Agent`). Owner-only.
//!
//! Validation is the fixture's `validateAgentForm`, shown inline; a failed
//! create is the "Couldn't create the agent" toast (FEED-79).

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};
use serde_json::Value;
use sync::Store;

use domain::agent_settings::{validate_agent_form, AgentForm, AGENT_SETTINGS};

use super::agent_detail::{
    account_options, agent_devices, current_account, device_picker_control, field_label, seed_device,
    AgentDevice,
};
use super::{section, SettingsSection};
use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::navigation::{navigate, Screen};
use crate::queries;
use crate::trigger_dialog::{self, DraftTriggers};
use crate::trigger_editor;

/// The page's create button label (web `agents-section.tsx`).
const CREATE_AGENT: &str = "Create agent";

pub struct NewAgentPane {
    team_id: String,
    name: Entity<InputState>,
    instructions: Entity<TextareaState>,
    /// Curated icon concept (contract `boardIcon`), sent as `icon`.
    icon: String,
    color: String,
    /// The picked machine's steer id; seeded to the caller's default machine,
    /// else the first online one, once the devices shape has it.
    device_id: Option<String>,
    /// The picked login (`agent`, profile id); `None` = the machine's last
    /// used one, the Account row's own default.
    account: Option<(String, String)>,
    /// The Schedule's draft — shared with the trigger dialog's save.
    triggers: Rc<RefCell<Vec<Value>>>,
    pending: crate::feedback::Pending,
    /// The fixture's validation messages (INVALID INPUT), by field.
    errors: Vec<domain::agent_settings::AgentFormError>,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl NewAgentPane {
    pub fn new(team_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let copy = &AGENT_SETTINGS.copy;
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(copy.name_placeholder.clone()));
        let instructions = cx.new(|cx| {
            crate::controls::web_textarea(4, 16, window, cx)
                .placeholder(copy.instructions_placeholder.clone())
        });
        let mut subscriptions = vec![
            cx.subscribe(&name, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.clear_error("identity", cx);
                }
            }),
            cx.subscribe(&instructions, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.clear_error("instructions", cx);
                }
            }),
        ];
        // The devices shape can land AFTER the page opened: the seed re-runs
        // on every delta (a pick the person made is never disturbed).
        if let Some(store) = Store::try_global(cx) {
            let devices = store.collections().devices.clone();
            subscriptions.push(cx.observe(&devices, |this: &mut Self, _, cx| {
                this.seed_device(cx);
                cx.notify();
            }));
        }
        let defaults = &domain::session_tree::THREADS_LIST.agent_defaults;
        let mut this = Self {
            team_id,
            name,
            instructions,
            icon: defaults.icon.clone(),
            color: defaults.color.clone(),
            device_id: None,
            account: None,
            triggers: Rc::default(),
            pending: crate::feedback::Pending::default(),
            errors: Vec::new(),
            focused_once: false,
            _subscriptions: subscriptions,
        };
        this.seed_device(cx);
        this
    }

    /// The caller's default machine, else the only one, else the first online.
    fn seed_device(&mut self, cx: &App) {
        if self.device_id.is_none() {
            let devices = agent_devices(&self.team_id, cx);
            self.device_id = seed_device(&devices).or_else(|| {
                devices
                    .iter()
                    .find(|device| device.online)
                    .map(|device| device.device_id.clone())
            });
        }
    }

    fn clear_error(&mut self, field: &str, cx: &mut gpui::Context<Self>) {
        let before = self.errors.len();
        self.errors.retain(|error| error.field != field);
        if self.errors.len() != before {
            cx.notify();
        }
    }

    fn error(&self, field: &str) -> Option<SharedString> {
        self.errors
            .iter()
            .find(|error| error.field == field)
            .map(|error| SharedString::from(error.message.clone()))
    }

    /// The Account row's login on the picked machine (THE rule,
    /// `agent_account_value`): the pick, else the machine's last used one.
    fn picked_account(&self, devices: &[AgentDevice]) -> (Vec<coding::AccountOption>, Option<coding::AccountOption>) {
        let options = self
            .device_id
            .as_deref()
            .and_then(|id| devices.iter().find(|device| device.device_id == id))
            .map(account_options)
            .unwrap_or_default();
        let (cli, account) = match &self.account {
            Some((cli, account)) => (Some(cli.as_str()), Some(account.as_str())),
            None => (None, None),
        };
        let current = current_account(&options, account, cli).cloned();
        (options, current)
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.pending.active() {
            return;
        }
        let form = AgentForm {
            name: self.name.read(cx).value().to_string(),
            device_id: self.device_id.clone(),
            instructions: self.instructions.read(cx).value().to_string(),
        };
        self.errors = validate_agent_form(&form);
        if !self.errors.is_empty() {
            cx.notify();
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            crate::action_error::failed_action("create", "the agent", Some("Not signed in.".into()), cx);
            return;
        };
        let (_, login) = self.picked_account(&agent_devices(&self.team_id, cx));
        self.pending.start(cx);
        cx.notify();

        // NEW-2: an agent is a DEFINITION row (`agents.create`), never a
        // session; it runs as conversation sessions on first message.
        let input = api::agents::CreateInput {
            team_id: self.team_id.clone(),
            name: form.name.trim().to_string(),
            icon: Some(self.icon.clone()),
            color: Some(self.color.clone()),
            instructions: form.instructions,
            triggers: self.triggers.borrow().clone(),
            device_id: form.device_id.unwrap_or_default(),
            cli: login.as_ref().map(|login| login.agent.id().to_string()),
            account: login.as_ref().and_then(coding::AccountOption::wire_account),
            model: None,
        };
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::agents::create(&trpc, &input) })
                .await;
            match result {
                Ok(agent) => {
                    // Land on the agent's page once its row has synced (the
                    // nav clamps an unknown `Agent(id)` away).
                    let agent_id = agent.id;
                    let agents = window
                        .update(|_, cx| Store::global(cx).collections().agents.clone())
                        .ok();
                    if let Some(agents) = agents {
                        queries::await_row_visible(&agents, &agent_id, window).await;
                    }
                    let _ = this.update_in(window, |this, window, cx| {
                        this.pending.finish();
                        crate::sidebar::select_settings_section(
                            window,
                            cx,
                            SettingsSection::Agent(agent_id),
                        );
                        navigate(window, cx, Screen::Settings);
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _, cx| {
                        this.pending.finish();
                        // A create can duplicate, so no Retry.
                        crate::action_error::failed_action_err("create", "the agent", &err, cx);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// The board form's identity row: icon picker, colour picker, name.
    fn identity_row(&self, window: &Window, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let view = cx.entity().downgrade();
        let icon_picker = crate::board_form::icon_picker(
            "new-agent",
            registry::PICKABLE_ICONS,
            Some(self.icon.as_str()),
            Some(self.color.as_str()),
            false,
            move |name, _, cx| {
                let Some(name) = name else { return };
                let _ = view.update(cx, |this, cx| {
                    this.icon = name.to_string();
                    cx.notify();
                });
            },
            cx,
        );
        let view = cx.entity().downgrade();
        let color_picker = crate::board_form::color_picker(
            "new-agent",
            &self.color,
            move |color, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.color = color.to_string();
                    cx.notify();
                });
            },
            cx,
        );
        crate::surface::glass_row_shell()
            .gap_2()
            .child(icon_picker)
            .child(color_picker)
            .child(
                div().flex_1().min_w_0().child(
                    crate::surface::glass_row_input(glass_input(&self.name, window, cx)).text_left(),
                ),
            )
    }

    /// The Device row: a pick drops the account pick (a profile id is
    /// device-local, EXP-995).
    fn device_row(&self, devices: &[AgentDevice], cx: &mut gpui::Context<Self>) -> gpui::Div {
        let view = cx.entity().downgrade();
        let control = device_picker_control(
            "new-agent",
            devices,
            self.device_id.clone(),
            Rc::new(move |device_id: String, _: &mut Window, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if this.device_id.as_deref() != Some(device_id.as_str()) {
                        this.account = None;
                    }
                    this.device_id = Some(device_id);
                    this.clear_error("device", cx);
                    cx.notify();
                });
            }),
            cx,
        );
        crate::surface::glass_picker_row(field_label("device"), None, control, cx)
    }

    /// The Account row: THE account picker over the picked machine's logins
    /// (brand mark + email); none reported reads "Last used", muted.
    fn account_row(&self, devices: &[AgentDevice], cx: &mut gpui::Context<Self>) -> gpui::Div {
        let (options, current) = self.picked_account(devices);
        let control = match current {
            None => div()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(AGENT_SETTINGS.copy.account_last_used.clone()))
                .into_any_element(),
            Some(current) => {
                let trigger = trigger_editor::picker_trigger_bare(
                    "new-agent-account".into(),
                    SharedString::from(current.email.clone()),
                    cx,
                )
                .icon(crate::coding_selects::agent_mark(current.agent))
                .child(crate::surface::picker_row_chevron(cx))
                .into_any_element();
                let picked = current.account_option_key();
                let view = cx.entity().downgrade();
                crate::picker::deferred(move |window, cx| {
                    let rows = options.clone();
                    let view = view.clone();
                    crate::picker::account_picker::account_picker(
                        &options,
                        Some(picked),
                        trigger,
                        Rc::new(move |next: Vec<String>, _window: &mut Window, cx: &mut App| {
                            let Some(option) = next.into_iter().next().and_then(|key| {
                                rows.iter().find(|option| option.account_option_key() == key).cloned()
                            }) else {
                                return;
                            };
                            let _ = view.update(cx, |this, cx| {
                                this.account = Some((option.agent.id().to_string(), option.id.clone()));
                                cx.notify();
                            });
                        }),
                    )
                    .id(SharedString::from("new-agent-account-picker"))
                    .render(window, cx)
                })
                .into_any_element()
            }
        };
        crate::surface::glass_picker_row(field_label("account"), None, control, cx)
    }

    /// The trigger dialog's handle on this page's draft array.
    fn draft(&self, view: WeakEntity<Self>) -> DraftTriggers {
        let read = self.triggers.clone();
        let write = self.triggers.clone();
        DraftTriggers {
            team_id: self.team_id.clone(),
            read: Rc::new(move |_| read.borrow().clone()),
            write: Rc::new(move |next, cx| {
                *write.borrow_mut() = next;
                let _ = view.update(cx, |_, cx| cx.notify());
            }),
        }
    }

    /// The Schedule rows, the agent page's: none = ONE row reading "Waits
    /// for messages" that opens the editor; else the "Schedule" row carrying
    /// "+ Add", then one row per trigger (summary opens it, switch pauses it).
    fn schedule_rows(&self, cx: &mut gpui::Context<Self>) -> Vec<gpui::Div> {
        let copy = &AGENT_SETTINGS.copy;
        let label = field_label("triggers");
        let draft = self.draft(cx.entity().downgrade());
        let triggers = self.triggers.borrow().clone();
        if triggers.is_empty() {
            let control = trigger_editor::picker_trigger("new-agent-schedule-new".into(), copy.no_schedule.clone(), cx)
                .on_click(move |_, window, cx| trigger_dialog::open_new_draft(window, cx, draft.clone()))
                .into_any_element();
            return vec![crate::surface::glass_picker_row(label, None, control, cx)];
        }
        let add: gpui::AnyElement = if triggers.len() < domain::contract::AGENT_TRIGGER_MAX_PER_AGENT {
            let draft = draft.clone();
            Button::new("new-agent-schedule-add")
                .outline()
                .cursor_pointer()
                .xsmall()
                .icon(registry::UI_ADD)
                .label(copy.add_trigger.clone())
                .on_click(move |_, window, cx| trigger_dialog::open_new_draft(window, cx, draft.clone()))
                .into_any_element()
        } else {
            div().into_any_element()
        };
        let mut rows = vec![crate::surface::glass_picker_row(label, None, add, cx)];
        rows.extend(triggers.iter().enumerate().filter_map(|(index, trigger)| {
            let trigger_id = trigger_editor::trigger_id(trigger)?.to_string();
            let enabled = trigger_editor::trigger_enabled(trigger);
            let (edit_draft, edit_trigger) = (draft.clone(), trigger_id.clone());
            let summary = trigger_editor::picker_trigger_bare(
                format!("new-agent-trigger-{index}").into(),
                domain::agent_settings::trigger_summary(trigger),
                cx,
            )
            .text_color(cx.theme().foreground)
            .on_click(move |_, window, cx| {
                trigger_dialog::open_edit_draft(window, cx, edit_draft.clone(), edit_trigger.clone())
            });
            let switch = crate::controls::web_switch(("new-agent-trigger-enabled", index))
                .checked(enabled)
                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                    let next = trigger_editor::triggers_with_enabled(&this.triggers.borrow(), &trigger_id, *on);
                    *this.triggers.borrow_mut() = next;
                    cx.notify();
                }));
            Some(
                crate::surface::glass_row_shell()
                    .text_sm()
                    .child(h_flex().flex_1().min_w_0().justify_start().child(summary))
                    .child(switch),
            )
        }));
        rows
    }

    /// The Instructions card: the chrome-less textarea in a glass group, its
    /// label as the caption under it.
    fn instructions_card(&self, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let mut card = v_flex()
            .w_full()
            .gap_1()
            .child(
                crate::surface::glass_group().child(
                    Textarea::new(&self.instructions)
                        .appearance(false)
                        .w_full()
                        .px_4()
                        .py_3(),
                ),
            )
            .child(
                div()
                    .px_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(field_label("instructions")),
            );
        if let Some(message) = self.error("instructions") {
            card = card.child(crate::action_error::field_error(message, cx).px_1());
        }
        card
    }
}

impl Render for NewAgentPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.name.update(cx, |state, cx| state.focus(window, cx));
        }
        let devices = agent_devices(&self.team_id, cx);

        let mut identity = v_flex().w_full().gap_2().child(crate::surface::glass_group_rows(vec![
            self.identity_row(window, cx),
            self.device_row(&devices, cx),
        ]));
        if let Some(message) = self.error("identity").or_else(|| self.error("device")) {
            identity = identity.child(crate::action_error::field_error(message, cx).px_1());
        }
        let mut runner_rows = vec![self.account_row(&devices, cx)];
        runner_rows.extend(self.schedule_rows(cx));

        let create = h_flex().justify_end().child(self.pending.apply(
            Button::new("new-agent-create")
                .primary()
                .cursor_pointer()
                .web_sm()
                .label(CREATE_AGENT)
                .disabled(self.pending.active())
                .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            CREATE_AGENT,
            cx,
        ));

        v_flex().child(
            section(cx)
                .gap_4()
                .child(identity)
                .child(crate::surface::glass_group_rows(runner_rows))
                .child(self.instructions_card(cx))
                .child(create),
        )
    }
}
