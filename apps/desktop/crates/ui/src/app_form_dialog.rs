//! VAPP-110 — New app / Rename (web `app-form-dialog.tsx`, fixtures/apps.json):
//! the board form's IDENTITY row (icon + colour + name, [`crate::board_form`])
//! and the optional description; Create = `apps.create` (the slug derives
//! server-side) and the editor opens; the same dialog renames an existing
//! app (`apps.update`). A failed create / rename = the failed-action toast.

use gpui::{
    div, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Render, Styled,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputState, Textarea, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _,
};
use sync::Store;

use domain::apps::{APPS, APPS_CAPS};

use crate::controls::{glass_input, WebControl as _};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::navigation::{navigate, Screen};

/// The web form's defaults.
const DEFAULT_ICON: &str = "puzzle";
const DEFAULT_COLOR: &str = "#6366f1";

/// What the form edits: a NEW app of `team_id`, or an existing one.
#[derive(Clone, Debug)]
pub(crate) enum AppFormTarget {
    New { team_id: String },
    Rename { app: domain::rows::AppRow },
}

/// Open the form in its own dialog window (the create-board dialog's shape).
pub(crate) fn open(window: &mut Window, cx: &mut App, target: AppFormTarget) {
    let title = match &target {
        AppFormTarget::New { .. } => APPS.copy.new_app.clone(),
        AppFormTarget::Rename { .. } => APPS.copy.menu.rename.clone(),
    };
    let spec = DialogSpec::new(title, size(px(416.), px(320.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| AppFormView::new(target, window, cx));
        let busy = view.clone();
        let submit = view.clone();
        DialogContent::new(view)
            .can_close(move |cx| !busy.read(cx).pending.active())
            .on_enter(move |window, cx| submit.update(cx, |view, cx| view.submit(window, cx)))
    });
}

pub(crate) struct AppFormView {
    target: AppFormTarget,
    name: Entity<InputState>,
    description: Entity<TextareaState>,
    icon: String,
    color: String,
    pending: crate::feedback::Pending,
    focused_once: bool,
}

impl AppFormView {
    fn new(target: AppFormTarget, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let copy = &APPS.copy;
        let (name, description, icon, color) = match &target {
            AppFormTarget::New { .. } => (String::new(), String::new(), DEFAULT_ICON.to_string(), DEFAULT_COLOR.to_string()),
            AppFormTarget::Rename { app } => (
                app.name.clone(),
                app.description.clone().unwrap_or_default(),
                app.icon.clone().unwrap_or_else(|| DEFAULT_ICON.to_string()),
                app.color.clone().unwrap_or_else(|| DEFAULT_COLOR.to_string()),
            ),
        };
        let name = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder(copy.name_placeholder.clone());
            state.set_value(name, window, cx);
            state
        });
        let description = cx.new(|cx| {
            let mut state = crate::controls::web_textarea(3, 8, window, cx)
                .placeholder(copy.description_placeholder.clone());
            state.set_value(description, window, cx);
            state
        });
        Self {
            target,
            name,
            description,
            icon,
            color,
            pending: crate::feedback::Pending::default(),
            focused_once: false,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() || self.pending.active() {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("create", "the app", None, cx);
            return;
        };
        let description: String = self.description.read(cx).value().chars().take(APPS_CAPS.description_max).collect();
        let (input, renaming) = match &self.target {
            AppFormTarget::New { team_id } => (
                api::apps::AppInput {
                    team_id: Some(team_id.clone()),
                    name,
                    icon: self.icon.clone(),
                    color: self.color.clone(),
                    description: Some(description.trim().to_string()).filter(|d| !d.is_empty()),
                    ..Default::default()
                },
                false,
            ),
            AppFormTarget::Rename { app } => (
                api::apps::AppInput {
                    id: Some(app.id.clone()),
                    name,
                    icon: self.icon.clone(),
                    color: self.color.clone(),
                    description: Some(description),
                    ..Default::default()
                },
                true,
            ),
        };
        self.pending.start(cx);
        cx.notify();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    if renaming {
                        api::apps::update(&trpc, &input)
                    } else {
                        api::apps::create(&trpc, &input)
                    }
                })
                .await;
            match result {
                Ok(output) => {
                    let app_id = output.app.id.clone();
                    if !renaming {
                        // The editor opens on the synced row.
                        let apps = window.update(|_, cx| Store::global(cx).collections().apps.clone()).ok();
                        if let Some(apps) = apps {
                            crate::queries::await_row_visible(&apps, &app_id, window).await;
                        }
                    }
                    let _ = this.update_in(window, |this, window, cx| {
                        this.pending.finish();
                        if renaming {
                            native_dialog::close_dialog_window(window, cx);
                        } else {
                            native_dialog::close_then(window, cx, move |window, cx| {
                                navigate(window, cx, Screen::App { app: app_id });
                            });
                        }
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _, cx| {
                        this.pending.finish();
                        crate::action_error::failed_action_err(
                            if renaming { "rename" } else { "create" },
                            "the app",
                            &err,
                            cx,
                        );
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// The board form's identity row: icon picker, colour picker, name.
    fn identity_row(&self, window: &Window, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let icon_view = cx.entity().downgrade();
        let color_view = cx.entity().downgrade();
        crate::surface::glass_row_shell()
            .gap_2()
            .child(crate::board_form::icon_picker(
                "app-form",
                crate::icons::registry::PICKABLE_ICONS,
                Some(self.icon.as_str()),
                Some(&self.color),
                false,
                move |name, _, cx| {
                    let Some(name) = name else { return };
                    let _ = icon_view.update(cx, |this, cx| {
                        this.icon = name.to_string();
                        cx.notify();
                    });
                },
                cx,
            ))
            .child(crate::board_form::color_picker(
                "app-form",
                &self.color,
                move |color, _, cx| {
                    let _ = color_view.update(cx, |this, cx| {
                        this.color = color.to_string();
                        cx.notify();
                    });
                },
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(crate::surface::glass_row_input(glass_input(&self.name, window, cx)).text_left()),
            )
    }
}

impl Render for AppFormView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.name.update(cx, |state, cx| state.focus(window, cx));
        }
        let copy = &APPS.copy;
        let label = match self.target {
            AppFormTarget::New { .. } => copy.create.clone(),
            AppFormTarget::Rename { .. } => copy.rename.clone(),
        };
        let empty = self.name.read(cx).value().trim().is_empty();
        v_flex()
            .size_full()
            .gap_4()
            .child(crate::surface::glass_group_rows(vec![self.identity_row(window, cx)]))
            .child(
                crate::surface::glass_group().child(
                    Textarea::new(&self.description)
                        .appearance(false)
                        .w_full()
                        .px_4()
                        .py_3(),
                ),
            )
            .child(div().flex_1())
            .child(
                h_flex()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .justify_end()
                    .child(self.pending.apply(
                        Button::new("app-form-submit")
                            .primary()
                            .cursor_pointer()
                            .web_sm()
                            .label(label.clone())
                            .disabled(empty && !self.pending.active())
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                        &label,
                        cx,
                    )),
            )
    }
}
