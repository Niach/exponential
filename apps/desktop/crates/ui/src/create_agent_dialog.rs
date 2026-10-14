//! The "New agent" dialog (EXP-1294 — the create-board dialog's twin):
//! ONE glass group holding the board form's identity row (icon picker,
//! colour picker, name) and the Device row (`fixtures/agent-settings.json`:
//! the device is required, the rest of the page is edited after). Validation
//! is the fixture's `validateAgentForm` (`domain::agent_settings`), shown
//! inline under the group.
//!
//! Submit → `sessions.send({teamId, deviceId, agent: {name, icon, color,
//! instructions: "", triggers: []}})`; the close waits for the new agent root
//! in the synced sessions collection, then the opener lands on its settings
//! page (`SettingsSection::Agent`). A failed create is the "Couldn't create
//! the agent" toast (FEED-79).

use std::rc::Rc;

use gpui::{
    div, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _,
};
use sync::Store;

use domain::agent_settings::{validate_agent_form, AgentForm, AGENT_SETTINGS};

use crate::controls::{glass_input, WebControl as _};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::navigation::{navigate, Screen};
use crate::queries;
use crate::settings::agent_detail::{agent_devices, device_picker_control, field_label, seed_device};
use crate::settings::SettingsSection;

/// Open the dialog for a team (the settings nav's "New agent").
pub(crate) fn open(window: &mut Window, cx: &mut App, team_id: String) {
    let height = (window.viewport_size().height * 0.85).min(px(320.));
    let spec = DialogSpec::new(AGENT_SETTINGS.copy.new_agent.clone(), size(px(416.), height));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| CreateAgentDialogView::new(team_id, window, cx));
        let busy = view.clone();
        let submit = view.clone();
        DialogContent::new(view)
            .can_close(move |cx| !busy.read(cx).pending.active())
            .on_enter(move |window, cx| {
                submit.update(cx, |view, cx| view.submit(window, cx));
            })
    });
}

struct CreateAgentDialogView {
    team_id: String,
    name: Entity<InputState>,
    /// Curated icon concept (contract `boardIcon`), sent as `icon`.
    icon: String,
    color: String,
    /// The picked machine's steer id; seeded to the caller's default (or
    /// only) machine once the devices shape has it.
    device_id: Option<String>,
    pending: crate::feedback::Pending,
    /// The fixture's first validation message (INVALID INPUT).
    error: Option<SharedString>,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl CreateAgentDialogView {
    fn new(team_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name = cx.new(|cx| {
            InputState::new(window, cx).placeholder(AGENT_SETTINGS.copy.name_placeholder.clone())
        });
        let mut subscriptions = vec![cx.subscribe_in(
            &name,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    this.error = None;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            },
        )];
        // The devices shape can land AFTER the dialog opened: the seed re-runs
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
            icon: defaults.icon.clone(),
            color: defaults.color.clone(),
            device_id: None,
            pending: crate::feedback::Pending::default(),
            error: None,
            focused_once: false,
            _subscriptions: subscriptions,
        };
        this.seed_device(cx);
        this
    }

    fn seed_device(&mut self, cx: &App) {
        if self.device_id.is_none() {
            self.device_id = seed_device(&agent_devices(&self.team_id, cx));
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.pending.active() {
            return;
        }
        let form = AgentForm {
            name: self.name.read(cx).value().to_string(),
            device_id: self.device_id.clone(),
            instructions: String::new(),
        };
        if let Some(error) = validate_agent_form(&form).into_iter().next() {
            self.error = Some(error.message.into());
            cx.notify();
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            crate::action_error::failed_action("create", "the agent", Some("Not signed in.".into()), cx);
            return;
        };
        self.error = None;
        self.pending.start(cx);
        cx.notify();

        let input = api::sessions::SendInput {
            team_id: Some(self.team_id.clone()),
            device_id: form.device_id,
            agent: Some(api::sessions::AgentInput {
                name: form.name.trim().to_string(),
                icon: self.icon.clone(),
                color: self.color.clone(),
                instructions: String::new(),
                triggers: Vec::new(),
            }),
            ..Default::default()
        };
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::sessions::send(&trpc, &input) })
                .await;
            match result {
                Ok(output) => {
                    let agent_id = output.session_id;
                    let sessions = window
                        .update(|_, cx| Store::global(cx).collections().sessions.clone())
                        .ok();
                    if let Some(sessions) = sessions {
                        queries::await_row_visible(&sessions, &agent_id, window).await;
                    }
                    let _ = this.update_in(window, |_, window, cx| {
                        native_dialog::close_then(window, cx, move |window, cx| {
                            crate::sidebar::select_settings_section(
                                window,
                                cx,
                                SettingsSection::Agent(agent_id),
                            );
                            navigate(window, cx, Screen::Settings);
                        });
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
            "create-agent",
            crate::icons::registry::PICKABLE_ICONS,
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
            "create-agent",
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

    fn device_row(&self, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let view = cx.entity().downgrade();
        let control = device_picker_control(
            "create-agent",
            &agent_devices(&self.team_id, cx),
            self.device_id.clone(),
            Rc::new(move |device_id: String, _: &mut Window, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.device_id = Some(device_id);
                    this.error = None;
                    cx.notify();
                });
            }),
            cx,
        );
        crate::surface::glass_picker_row(field_label("device"), None, control, cx)
    }
}

impl Render for CreateAgentDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.name.update(cx, |state, cx| state.focus(window, cx));
        }
        let name_empty = self.name.read(cx).value().trim().is_empty();
        let disabled = name_empty || self.device_id.is_none();

        let mut form = v_flex().gap_2().child(crate::surface::glass_group_rows(vec![
            self.identity_row(window, cx),
            self.device_row(cx),
        ]));
        if let Some(message) = self.error.clone() {
            form = form.child(crate::action_error::field_error(message, cx).px_1());
        }
        let label = AGENT_SETTINGS.copy.new_agent.clone();
        let footer = h_flex().flex_shrink_0().justify_end().child(self.pending.apply(
            Button::new("create-agent-submit")
                .primary()
                .cursor_pointer()
                .web_sm()
                .label(label.clone())
                .disabled(disabled && !self.pending.active())
                .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            &label,
            cx,
        ));
        v_flex()
            .gap_4()
            .child(form)
            .child(footer.pt_3().border_t_1().border_color(cx.theme().border))
    }
}
