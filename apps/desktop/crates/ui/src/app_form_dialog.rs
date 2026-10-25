//! VAPP-110 — New app / Rename (web `app-form-dialog.tsx`, fixtures/apps.json):
//! the board form's IDENTITY row (icon + colour + name, [`crate::board_form`])
//! and the optional description; Create = `apps.create` (the slug derives
//! server-side) and the editor opens; the same dialog renames an existing
//! app (`apps.update`). A failed create / rename = the failed-action toast.
//!
//! VAPP-121 (fixtures/app-export.json): New app's ghost `Import…` switches
//! the dialog to the IMPORT step (`Import an app`): a monospace field for an
//! app file or a bare package plus `Choose file` (the native open panel); as
//! the text changes the SERVER checks it (`apps.validateImport`, debounced)
//! and its issues list under the field as `path: message` rows; Import
//! (enabled only with none) = `apps.import`, then the editor opens.

use gpui::{
    div, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _,
};
use sync::Store;

use api::apps::ImportIssue;
use domain::app_export::{more_issues_label, APP_EXPORT_COPY, APP_EXPORT_LIMITS};
use domain::apps::{APPS, APPS_CAPS};

use crate::controls::{glass_input, WebControl as _};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::navigation::{navigate, Screen};

/// The web form's defaults.
const DEFAULT_ICON: &str = "puzzle";
const DEFAULT_COLOR: &str = "#6366f1";

/// The form's content size, and the import step's (the field + its issues).
const FORM_SIZE: (f32, f32) = (416., 320.);
const IMPORT_SIZE: (f32, f32) = (416., 380.);
/// The server check's debounce (the web's 300 ms).
const VALIDATE_DEBOUNCE_MS: u64 = 300;

/// What the form edits: a NEW app of `team_id`, or an existing one.
#[derive(Clone, Debug)]
pub(crate) enum AppFormTarget {
    New { team_id: String },
    Rename { app: domain::rows::AppRow },
}

/// Open the form in its own dialog window (the create-board dialog's shape).
pub(crate) fn open(window: &mut Window, cx: &mut App, target: AppFormTarget) {
    open_with(window, cx, target, None);
}

/// VAPP-121: New app opened straight on its import step, `text` in the
/// field (the `app-import` capture hook).
pub(crate) fn open_import(window: &mut Window, cx: &mut App, team_id: String, text: String) {
    open_with(window, cx, AppFormTarget::New { team_id }, Some(text));
}

fn open_with(window: &mut Window, cx: &mut App, target: AppFormTarget, import: Option<String>) {
    let title = match (&target, &import) {
        (_, Some(_)) => APP_EXPORT_COPY.import_title.clone(),
        (AppFormTarget::New { .. }, None) => APPS.copy.new_app.clone(),
        (AppFormTarget::Rename { .. }, None) => APPS.copy.menu.rename.clone(),
    };
    let (width, height) = if import.is_some() { IMPORT_SIZE } else { FORM_SIZE };
    let spec = DialogSpec::new(title, size(px(width), px(height)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            let mut view = AppFormView::new(target, window, cx);
            if let Some(text) = import {
                view.importing = true;
                view.import_text.update(cx, |state, cx| state.set_value(text, window, cx));
                view.schedule_validate(0, cx);
            }
            view
        });
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
    /// VAPP-121: the import step is up (New app only).
    importing: bool,
    import_text: Entity<TextareaState>,
    /// The server's issues for the current text; None = unchecked / empty.
    issues: Option<Vec<ImportIssue>>,
    /// The debounced `apps.validateImport` (replacing it cancels).
    validate: Option<Task<()>>,
    import_focused: bool,
    _subscriptions: Vec<Subscription>,
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
        let import_text = cx.new(|cx| {
            crate::controls::web_textarea(8, 8, window, cx).placeholder(APP_EXPORT_COPY.import_placeholder.clone())
        });
        let subscriptions = vec![cx.subscribe(&import_text, |this: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.schedule_validate(VALIDATE_DEBOUNCE_MS, cx);
            }
        })];
        Self {
            target,
            name,
            description,
            icon,
            color,
            pending: crate::feedback::Pending::default(),
            focused_once: false,
            importing: false,
            import_text,
            issues: None,
            validate: None,
            import_focused: false,
            _subscriptions: subscriptions,
        }
    }

    fn team_id(&self) -> Option<String> {
        match &self.target {
            AppFormTarget::New { team_id } => Some(team_id.clone()),
            AppFormTarget::Rename { .. } => None,
        }
    }

    /// `Import…`: the same dialog switches to the import step.
    fn start_import(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.importing || self.team_id().is_none() {
            return;
        }
        self.importing = true;
        native_dialog::set_dialog_title(window, cx, APP_EXPORT_COPY.import_title.clone());
        let (width, height) = IMPORT_SIZE;
        native_dialog::resize_dialog_keeping_top(
            window,
            cx,
            size(px(width), px(height) + crate::title_bar::TITLE_BAR_HEIGHT),
        );
        cx.notify();
    }

    /// The server is the one authority: check the text after `delay_ms`
    /// (a newer keystroke replaces the pending check).
    fn schedule_validate(&mut self, delay_ms: u64, cx: &mut gpui::Context<Self>) {
        let text = self.import_text.read(cx).value().to_string();
        if text.trim().is_empty() {
            self.validate = None;
            self.issues = None;
            cx.notify();
            return;
        }
        let (Some(team_id), Some(trpc)) = (self.team_id(), crate::queries::trpc_client(cx)) else { return };
        self.validate = Some(cx.spawn(async move |this, cx| {
            if delay_ms > 0 {
                cx.background_executor().timer(std::time::Duration::from_millis(delay_ms)).await;
            }
            let checked = text.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::apps::validate_import(&trpc, &team_id, &checked) })
                .await;
            let _ = this.update(cx, |this, cx| {
                // A stale answer for older text never lands.
                if this.import_text.read(cx).value().to_string() != text {
                    return;
                }
                this.issues = Some(match result {
                    Ok(output) => output.issues,
                    Err(_) => vec![ImportIssue { path: "/".into(), message: "could not be checked".into() }],
                });
                cx.notify();
            });
        }));
    }

    /// `Choose file`: the native open panel; the file's text fills the field.
    fn choose_file(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let receiver = crate::file_picker::prompt_for_paths(cx, gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(APP_EXPORT_COPY.choose_file.clone().into()),
        });
        cx.spawn_in(window, async move |this, window| {
            let Ok(Ok(Some(paths))) = receiver.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let result = window
                .background_executor()
                .spawn(async move { std::fs::read_to_string(&path) })
                .await;
            let _ = this.update_in(window, |this, window, cx| match result {
                Ok(text) => {
                    this.import_text.update(cx, |state, cx| state.set_value(text, window, cx));
                    this.schedule_validate(0, cx);
                }
                Err(error) => {
                    crate::action_error::failed_action("open", "the file", Some(error.to_string()), cx)
                }
            });
        })
        .detach();
    }

    /// `Import`: `apps.import`, then the new app's editor (Create's path).
    fn run_import(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.pending.active() || !self.issues.as_ref().is_some_and(Vec::is_empty) {
            return;
        }
        let (Some(team_id), Some(trpc)) = (self.team_id(), crate::queries::trpc_client(cx)) else {
            crate::action_error::failed_action("import", "the app", None, cx);
            return;
        };
        let text = self.import_text.read(cx).value().to_string();
        self.pending.start(cx);
        cx.notify();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::apps::import(&trpc, &team_id, &text) })
                .await;
            match result {
                Ok(output) => {
                    let app_id = output.app.id.clone();
                    let apps = window.update(|_, cx| Store::global(cx).collections().apps.clone()).ok();
                    if let Some(apps) = apps {
                        crate::queries::await_row_visible(&apps, &app_id, window).await;
                    }
                    let _ = this.update_in(window, |this, window, cx| {
                        this.pending.finish();
                        native_dialog::close_then(window, cx, move |window, cx| {
                            navigate(window, cx, Screen::App { app: app_id });
                        });
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _, cx| {
                        this.pending.finish();
                        crate::action_error::failed_action_err("import", "the app", &err, cx);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// The import step: the field, Choose file, the server's issues, Import.
    fn render_import(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> gpui::Div {
        if !self.import_focused {
            self.import_focused = true;
            self.import_text.update(cx, |state, cx| state.focus(window, cx));
        }
        let danger = cx.theme().danger;
        let issues = self.issues.clone().unwrap_or_default();
        let shown = APP_EXPORT_LIMITS.issues_shown;
        let issue_rows = (!issues.is_empty()).then(|| {
            v_flex()
                .id("app-import-issues")
                .gap_1()
                .px_1()
                .text_xs()
                .font_family(theme::terminal::FONT_FAMILY)
                .text_color(danger)
                .children(
                    issues
                        .iter()
                        .take(shown)
                        .map(|issue| div().child(SharedString::from(format!("{}: {}", issue.path, issue.message)))),
                )
                .children(more_issues_label(issues.len()).map(|more| div().child(SharedString::from(more))))
        });
        let label = APP_EXPORT_COPY.import_action.clone();
        let ready = self.issues.as_ref().is_some_and(Vec::is_empty);
        v_flex()
            .size_full()
            .gap_3()
            .child(
                crate::surface::glass_group().child(
                    Textarea::new(&self.import_text)
                        .appearance(false)
                        .w_full()
                        .px_4()
                        .py_3()
                        .text_xs()
                        .font_family(theme::terminal::FONT_FAMILY),
                ),
            )
            .child(
                h_flex().child(
                    Button::new("app-import-file")
                        .outline()
                        .cursor_pointer()
                        .web_sm()
                        .label(APP_EXPORT_COPY.choose_file.clone())
                        .on_click(cx.listener(|this, _, window, cx| this.choose_file(window, cx))),
                ),
            )
            .children(issue_rows)
            .child(div().flex_1())
            .child(
                h_flex()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .justify_end()
                    .child(self.pending.apply(
                        Button::new("app-import-confirm")
                            .primary()
                            .cursor_pointer()
                            .web_sm()
                            .label(label.clone())
                            .disabled(!ready && !self.pending.active())
                            .on_click(cx.listener(|this, _, window, cx| this.run_import(window, cx))),
                        &label,
                        cx,
                    )),
            )
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.importing {
            self.run_import(window, cx);
            return;
        }
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
        if self.importing {
            return self.render_import(window, cx);
        }
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
            // N-14: THE row host: an open icon grid draws under the whole
            // identity row, so no sibling moves (picker-presentation.json).
            .child(crate::surface::glass_group_rows(vec![div().w_full().child(
                crate::picker::InlinePickerRow::new("app-form-identity", self.identity_row(window, cx)),
            )]))
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
                    // VAPP-121: `Import…` left of Create, create mode only.
                    .when(matches!(self.target, AppFormTarget::New { .. }), |row| {
                        row.child(
                            Button::new("app-import")
                                .ghost()
                                .cursor_pointer()
                                .web_sm()
                                .label(APP_EXPORT_COPY.import.clone())
                                .on_click(cx.listener(|this, _, window, cx| this.start_import(window, cx))),
                        )
                        .child(div().flex_1())
                    })
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

use gpui::prelude::FluentBuilder as _;
use gpui::InteractiveElement as _;
