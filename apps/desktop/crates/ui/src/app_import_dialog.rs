//! VAPP-121 + VAPP-176 (fixtures/app-export.json IMPORT): the Apps band's
//! `Import…` pill opens this dialog (web `app-import-dialog.tsx`), titled
//! `Import an app`: a monospace field for an app file or a bare package plus
//! `Choose file` (the native open panel); as the text changes the SERVER
//! checks it (`apps.validateImport`, debounced) and its issues list under
//! the field as `path: message` rows; Import (enabled only with none) =
//! `apps.import`, then the editor opens. New app has no dialog any more
//! (VAPP-176: a draft and its editor, [`crate::apps_view::new_app`]).
//! VAPP-234 (fixtures/app-templates.json): the TEMPLATES step above the
//! field: one row per signed template (a click = `apps.importTemplate` at
//! once, then its editor) and the `Include sample records` switch row; a
//! draft's `Start from a template` pill opens the same dialog.

use gpui::{
    div, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, Textarea, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _,
};
use sync::Store;

use api::apps::ImportIssue;
use domain::app_export::{more_issues_label, APP_EXPORT_COPY, APP_EXPORT_LIMITS};
use domain::app_templates::{APP_TEMPLATES, APP_TEMPLATES_COPY, APP_TEMPLATES_TEST_IDS};

use crate::controls::WebControl as _;
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::navigation::{navigate, Screen};

/// The dialog's content size (the field + its issues).
const IMPORT_SIZE: (f32, f32) = (416., 640.);
/// The server check's debounce (the web's 300 ms).
const VALIDATE_DEBOUNCE_MS: u64 = 300;

/// Open the import dialog for `team_id` (the band's `Import…` pill).
pub(crate) fn open(window: &mut Window, cx: &mut App, team_id: String) {
    open_import(window, cx, team_id, String::new());
}

/// The import dialog with `text` in the field (the `app-import` capture
/// hook passes the fixture's capture text).
pub(crate) fn open_import(window: &mut Window, cx: &mut App, team_id: String, text: String) {
    let (width, height) = IMPORT_SIZE;
    let spec = DialogSpec::new(APP_EXPORT_COPY.import_title.clone(), size(px(width), px(height)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            let mut view = AppImportView::new(team_id, window, cx);
            if !text.is_empty() {
                view.import_text.update(cx, |state, cx| state.set_value(text, window, cx));
                view.schedule_validate(0, cx);
            }
            view
        });
        let busy = view.clone();
        let submit = view.clone();
        DialogContent::new(view)
            .can_close(move |cx| !busy.read(cx).pending.active())
            .on_enter(move |window, cx| submit.update(cx, |view, cx| view.run_import(window, cx)))
    });
}

pub(crate) struct AppImportView {
    team_id: String,
    pending: crate::feedback::Pending,
    import_text: Entity<TextareaState>,
    /// The server's issues for the current text; None = unchecked / empty.
    issues: Option<Vec<ImportIssue>>,
    /// The debounced `apps.validateImport` (replacing it cancels).
    validate: Option<Task<()>>,
    import_focused: bool,
    /// VAPP-234: the templates step's switch (off by default).
    with_records: bool,
    /// VAPP-234: the template being imported (every row waits).
    picking: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl AppImportView {
    fn new(team_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let import_text = cx.new(|cx| {
            crate::controls::web_textarea(6, 6, window, cx).placeholder(APP_EXPORT_COPY.import_placeholder.clone())
        });
        let subscriptions = vec![cx.subscribe(&import_text, |this: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.schedule_validate(VALIDATE_DEBOUNCE_MS, cx);
            }
        })];
        Self {
            team_id,
            pending: crate::feedback::Pending::default(),
            import_text,
            issues: None,
            validate: None,
            import_focused: false,
            with_records: false,
            picking: None,
            _subscriptions: subscriptions,
        }
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
        let team_id = self.team_id.clone();
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
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

    /// VAPP-234: a template row = `apps.importTemplate` at once, then the
    /// new app's editor (the import's path).
    fn pick_template(&mut self, template: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.picking.is_some() || self.pending.active() {
            return;
        }
        let team_id = self.team_id.clone();
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("import", "the template", None, cx);
            return;
        };
        let include_records = self.with_records;
        self.picking = Some(template.clone());
        cx.notify();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::apps::import_template(&trpc, &team_id, &template, include_records) })
                .await;
            match result {
                Ok(output) => {
                    let app_id = output.app.id.clone();
                    let apps = window.update(|_, cx| Store::global(cx).collections().apps.clone()).ok();
                    if let Some(apps) = apps {
                        crate::queries::await_row_visible(&apps, &app_id, window).await;
                    }
                    let _ = this.update_in(window, |this, window, cx| {
                        this.picking = None;
                        native_dialog::close_then(window, cx, move |window, cx| {
                            navigate(window, cx, Screen::App { app: app_id });
                        });
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _, cx| {
                        this.picking = None;
                        crate::action_error::failed_action_err("import", "the template", &err, cx);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// VAPP-234: the templates step: the band, one row per template, the
    /// sample-records switch row.
    fn render_templates(&mut self, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let row_hover = cx.theme().list_hover;
        let muted = cx.theme().muted_foreground;
        let busy = self.picking.is_some();
        let rows = APP_TEMPLATES.iter().map(|template| {
            let id = template.id.clone();
            crate::surface::flat_row()
                .id(SharedString::from(format!("{}-{}", APP_TEMPLATES_TEST_IDS.row, template.id)))
                .flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_3()
                .px_4()
                .py(px(10.))
                .text_sm()
                .when(!busy, |row| row.cursor_pointer().hover(move |this| this.bg(row_hover)))
                .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| this.pick_template(id.clone(), window, cx)))
                .child(crate::icons::app_icon(Some(&template.icon), Some(&template.color)).small().flex_shrink_0())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().font_weight(gpui::FontWeight::MEDIUM).child(SharedString::from(template.name.clone())))
                        .child(div().truncate().text_xs().text_color(muted).child(SharedString::from(template.description.clone()))),
                )
        });
        let rows: Vec<gpui::Div> = rows.map(|row| div().child(row)).collect();
        v_flex()
            .gap_3()
            .child(crate::surface::glass_section_header(APP_TEMPLATES_COPY.title.clone(), None, cx))
            .child(crate::surface::glass_group_rows(rows))
            .child(crate::surface::glass_group_rows(vec![crate::surface::glass_toggle_row(
                APP_TEMPLATES_COPY.include_records.clone(),
                None,
                crate::controls::web_switch(SharedString::from(APP_TEMPLATES_TEST_IDS.records.clone()))
                    .checked(self.with_records)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.with_records = *checked;
                        cx.notify();
                    }))
                    .into_any_element(),
                cx,
            )]))
    }

    /// `Import`: `apps.import`, then the new app's editor (Create's path).
    fn run_import(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.pending.active() || !self.issues.as_ref().is_some_and(Vec::is_empty) {
            return;
        }
        let team_id = self.team_id.clone();
        let Some(trpc) = crate::queries::trpc_client(cx) else {
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
            .child(self.render_templates(cx))
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
}

impl Render for AppImportView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.render_import(window, cx)
    }
}

use gpui::{InteractiveElement as _, StatefulInteractiveElement as _, prelude::FluentBuilder as _};
use gpui_component::Sizable as _;
