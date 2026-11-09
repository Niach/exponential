//! VAPP-228 (X2, fixtures/app-records.json `csvImport`): the Data tab's
//! `Import…` opens this dialog (web `app-csv-import-dialog.tsx`), the app
//! import dialog's pattern: a monospace field for a CSV + `Choose file`; an
//! OWNER's `Author` select first (`Me`, then the header columns), then one
//! labelled select per field (`data_columns` order; the header columns, then
//! `Skip`; auto-matched on a new header); the first rows preview as the Data
//! tab's phone rows (title = the first field, the rest ` · `) each with its
//! issues (the model's plus eUI's `validate_record`), the ready summary,
//! `tooMany` inline; `Import {n}` = ONE `apps.records.batch` (`via:
//! import`) of the ready rows, then the Undo toast titled `copy.imported`.
//! A BAD_REQUEST shows inline, anything else is the failed-action toast.
//! The pure model is `domain::app_records`.

use std::collections::BTreeMap;

use gpui::{
    div, px, size, AnyElement, App, AppContext as _, Div, Entity, InteractiveElement as _, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, Textarea, TextareaState},
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Disableable as _,
};
use serde_json::{json, Value};

use domain::app_records::{
    cell_text, csv_author_column, csv_auto_map, csv_parse, csv_records, csv_summary, data_columns, CsvAuthor,
    CsvMember, CsvRecord, CsvTable, DataSchema, APP_CSV_IMPORT, DATA_TAB,
};

use crate::app_data_tab::RecordFormView;
use crate::controls::{PointerMenuButtonExt as _, WebControl as _};
use crate::native_dialog::{self, DialogContent, DialogSpec};

/// The dialog's width and its height cap (web `sm:max-w-lg`, 85dvh).
const CSV_WIDTH: f32 = 512.;
const CSV_HEIGHT: f32 = 640.;

/// What the dialog imports into.
pub(crate) struct CsvImportTarget {
    pub(crate) app_id: String,
    pub(crate) kind: String,
    pub(crate) schema: DataSchema,
    /// The kind's schema JSON (eUI `validate_record`'s input).
    pub(crate) raw_schema: Value,
    /// The importer (the signed-in user).
    pub(crate) me: Option<String>,
    /// An OWNER's team members for the Author column (`None` = no pick).
    pub(crate) members: Option<Vec<CsvMember>>,
}

/// Open the import dialog (the Data tab's `Import…`).
pub(crate) fn open(window: &mut Window, cx: &mut App, target: CsvImportTarget) {
    let title = APP_CSV_IMPORT.copy.title.replace("{kind}", &target.kind);
    let height = (window.viewport_size().height * 0.85).min(px(CSV_HEIGHT));
    let spec = DialogSpec::new(title, size(px(CSV_WIDTH), height)).resizable(size(px(400.), px(320.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| AppCsvImportView::new(target, window, cx));
        let busy = view.clone();
        DialogContent::new(view).self_scrolling().can_close(move |cx| !busy.read(cx).pending.active())
    });
}

/// Which select a pick lands in.
#[derive(Clone)]
enum ColumnSlot {
    Author,
    Field(String),
}

pub(crate) struct AppCsvImportView {
    target: CsvImportTarget,
    fields: Vec<String>,
    text: Entity<TextareaState>,
    table: CsvTable,
    mapping: BTreeMap<String, Option<usize>>,
    author_column: Option<usize>,
    /// The rows with every issue (the model's, then the host's check).
    records: Vec<CsvRecord>,
    /// The server's BAD_REQUEST message, inline under the preview.
    error: Option<SharedString>,
    pending: crate::feedback::Pending,
    focused: bool,
    body_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl AppCsvImportView {
    fn new(target: CsvImportTarget, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let text = cx.new(|cx| crate::controls::web_textarea(6, 6, window, cx).placeholder(APP_CSV_IMPORT.copy.placeholder.clone()));
        let subscriptions = vec![cx.subscribe(&text, |this: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.error = None;
                this.refresh_text(cx);
            }
        })];
        let fields = data_columns(&target.schema);
        let mapping = csv_auto_map(&fields, &[]);
        Self {
            target,
            fields,
            text,
            table: CsvTable::default(),
            mapping,
            author_column: None,
            records: Vec::new(),
            error: None,
            pending: crate::feedback::Pending::default(),
            focused: false,
            body_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Re-read the text; a new header = a new first pick.
    fn refresh_text(&mut self, cx: &mut gpui::Context<Self>) {
        let table = csv_parse(&self.text.read(cx).value().to_string());
        if table.header != self.table.header {
            self.mapping = csv_auto_map(&self.fields, &table.header);
            self.author_column = self.target.members.as_ref().and_then(|_| csv_author_column(&table.header));
        }
        self.table = table;
        self.refresh_records();
        cx.notify();
    }

    /// The rows → records; the host's schema check on a row with no issues.
    fn refresh_records(&mut self) {
        let author = match (&self.target.members, &self.target.me) {
            (Some(members), Some(me)) => {
                Some(CsvAuthor { column: self.author_column, importer: me.clone(), members: members.clone() })
            }
            _ => None,
        };
        let copy = &APP_CSV_IMPORT.copy;
        self.records = csv_records(&self.target.schema, &self.table.header, &self.table.rows, &self.mapping, author.as_ref())
            .into_iter()
            .map(|mut record| {
                if record.issues.is_empty() {
                    let row = record.row.to_string();
                    record.issues = eui_core::host::validate_record(&self.target.raw_schema, &Value::Object(record.data.clone()))
                        .into_iter()
                        .map(|issue| {
                            let path = issue.path.strip_prefix('/').unwrap_or(&issue.path);
                            copy.issue.replace("{row}", &row).replace("{field}", path).replace("{message}", &issue.message)
                        })
                        .collect();
                }
                record
            })
            .collect();
    }

    fn ready(&self) -> Vec<&CsvRecord> {
        self.records.iter().filter(|r| r.issues.is_empty()).collect()
    }

    fn too_many(&self) -> bool {
        self.records.len() > APP_CSV_IMPORT.max_rows
    }

    /// `Choose file`: the native open panel; the file's text fills the field.
    fn choose_file(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let receiver = crate::file_picker::prompt_for_paths(cx, gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(APP_CSV_IMPORT.copy.choose_file.clone().into()),
        });
        cx.spawn_in(window, async move |this, window| {
            let Ok(Ok(Some(paths))) = receiver.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let result = window.background_executor().spawn(async move { std::fs::read_to_string(&path) }).await;
            let _ = this.update_in(window, |this, window, cx| match result {
                Ok(text) => {
                    this.text.update(cx, |state, cx| state.set_value(text, window, cx));
                    this.error = None;
                    this.refresh_text(cx);
                }
                Err(error) => crate::action_error::failed_action("open", "the file", Some(error.to_string()), cx),
            });
        })
        .detach();
    }

    /// `Import {n}`: ONE `apps.records.batch` of the ready rows, in order.
    fn run_import(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let ready = self.ready();
        if self.pending.active() || self.too_many() || ready.is_empty() {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("import", "the records", None, cx);
            return;
        };
        let kind = &self.target.kind;
        let writes: Vec<Value> = ready
            .iter()
            .map(|record| {
                let mut write = json!({"op": "upsert", "kind": kind, "data": Value::Object(record.data.clone())});
                if let Some(author) = &record.author {
                    write["author"] = json!(author);
                }
                write
            })
            .collect();
        let app_id = self.target.app_id.clone();
        let input = json!({"id": app_id, "writes": writes, "via": "import"});
        self.pending.start(cx);
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { trpc.mutation::<_, api::apps::RecordsBatchOutput>("apps.records.batch", &input) })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.pending.finish();
                match result {
                    Ok(out) => {
                        native_dialog::close_dialog_window(window, cx);
                        let title = APP_CSV_IMPORT.copy.imported.replace("{n}", &out.ids.len().to_string());
                        crate::app_data_tab::toast_record_undo_titled(&app_id, title, out.event_ids, "editor", cx);
                    }
                    Err(err @ api::ApiError::Http { status: 400, .. }) => {
                        this.error = Some(crate::settings::form_error(&err, "the rows are invalid").into());
                    }
                    Err(err) => crate::action_error::failed_action_err("import", "the records", &err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A column select: `none` (`Me` first for the Author, `Skip` last for
    /// a field), the header names in order.
    fn column_select(&self, id: SharedString, slot: ColumnSlot, cx: &mut gpui::Context<Self>) -> AnyElement {
        let (value, none, none_first) = match &slot {
            ColumnSlot::Author => (self.author_column, APP_CSV_IMPORT.copy.me.clone(), true),
            ColumnSlot::Field(name) => (self.mapping.get(name).copied().flatten(), APP_CSV_IMPORT.copy.skip.clone(), false),
        };
        let mut options: Vec<(Option<usize>, String)> = self.table.header.iter().cloned().enumerate().map(|(i, name)| (Some(i), name)).collect();
        if none_first {
            options.insert(0, (None, none));
        } else {
            options.push((None, none));
        }
        let picked = options.iter().find(|(v, _)| *v == value).map(|(_, label)| label.clone()).unwrap_or_default();
        let view = cx.entity().downgrade();
        crate::trigger_editor::picker_trigger(id, picked, cx).pointer_menu(move |mut menu, _window, _cx| {
            menu = crate::pickers::single_select_menu(menu);
            for (option, label) in &options {
                let (view, option, slot) = (view.clone(), *option, slot.clone());
                menu = menu.item(
                    crate::controls::pointer_label_item(SharedString::from(label.clone()), false)
                        .checked(option == value)
                        .on_click(move |_, _, cx| {
                            let _ = view.update(cx, |this, cx| {
                                match &slot {
                                    ColumnSlot::Author => this.author_column = option,
                                    ColumnSlot::Field(name) => {
                                        this.mapping.insert(name.clone(), option);
                                    }
                                }
                                this.refresh_records();
                                cx.notify();
                            });
                        }),
                );
            }
            menu
        })
        .into_any_element()
    }

    /// The mapping: an owner's Author first, then one row per field.
    fn render_map(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let mut rows: Vec<Div> = Vec::new();
        if self.target.members.is_some() {
            let control = self.column_select(SharedString::from("app-csv-author-select"), ColumnSlot::Author, cx);
            rows.push(
                crate::surface::glass_row_shell()
                    .child(RecordFormView::label_text(DATA_TAB.copy.author.clone(), true, cx))
                    .child(
                        h_flex()
                            .id(SharedString::from(APP_CSV_IMPORT.test_ids.author.clone()))
                            .flex_1()
                            .min_w_0()
                            .justify_end()
                            .child(control),
                    ),
            );
        }
        for name in &self.fields {
            let required = self.target.schema.required.contains(name);
            let control = self.column_select(SharedString::from(format!("app-csv-field-{name}")), ColumnSlot::Field(name.clone()), cx);
            rows.push(
                crate::surface::glass_row_shell()
                    .child(RecordFormView::label_text(name.clone(), required, cx))
                    .child(h_flex().flex_1().min_w_0().justify_end().child(control)),
            );
        }
        crate::surface::glass_group_rows(rows)
            .id(SharedString::from(APP_CSV_IMPORT.test_ids.map.clone()))
    }

    /// The first rows as the Data tab's phone rows, each with its issues.
    fn render_preview(&self, cx: &App) -> impl IntoElement {
        let spec = &*APP_CSV_IMPORT;
        let (muted, danger) = (cx.theme().muted_foreground, cx.theme().danger);
        let rows = self
            .records
            .iter()
            .take(spec.preview_rows)
            .map(|record| {
                let title = self.fields.first().map(|name| cell_text(record.data.get(name))).unwrap_or_default();
                let subtitle = self
                    .fields
                    .iter()
                    .skip(1)
                    .map(|name| cell_text(record.data.get(name)))
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                let title_el = if title.is_empty() {
                    div().min_w_0().truncate().text_color(muted).child(DATA_TAB.copy.empty_title.clone())
                } else {
                    div().min_w_0().truncate().font_weight(gpui::FontWeight::MEDIUM).child(title)
                };
                let issues = (!record.issues.is_empty()).then(|| {
                    v_flex()
                        .mt_1()
                        .gap_0p5()
                        .text_xs()
                        .font_family(theme::terminal::FONT_FAMILY)
                        .text_color(danger)
                        .children(record.issues.iter().take(spec.issues_shown).map(|issue| div().child(SharedString::from(issue.clone()))))
                        .children((record.issues.len() > spec.issues_shown).then(|| {
                            div().child(spec.copy.more_issues.replace("{n}", &(record.issues.len() - spec.issues_shown).to_string()))
                        }))
                });
                crate::surface::glass_row_shell().py_2().text_sm().child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(title_el)
                        .children((!subtitle.is_empty()).then(|| div().mt_0p5().min_w_0().truncate().text_xs().text_color(muted).child(subtitle)))
                        .children(issues),
                )
            })
            .collect();
        v_flex()
            .id(SharedString::from(spec.test_ids.preview.clone()))
            .w_full()
            .gap_1p5()
            .child(crate::surface::glass_group_rows(rows))
            .child(div().px_1().text_xs().text_color(muted).child(csv_summary(&self.records)))
            .children(self.too_many().then(|| {
                div().px_1().text_sm().text_color(danger).child(spec.copy.too_many.replace("{max}", &spec.max_rows.to_string()))
            }))
    }

    fn footer(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let ready = self.ready().len();
        let label = APP_CSV_IMPORT.copy.import_action.replace("{n}", &ready.to_string());
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().flex_1())
            .child(
                Button::new("app-csv-cancel")
                    .outline()
                    .cursor_pointer()
                    .web_sm()
                    .label(DATA_TAB.copy.cancel.clone())
                    .disabled(self.pending.active())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !this.pending.active() {
                            native_dialog::close_dialog_window(window, cx);
                        }
                    })),
            )
            .child(self.pending.apply(
                Button::new(SharedString::from(APP_CSV_IMPORT.test_ids.confirm.clone()))
                    .primary()
                    .cursor_pointer()
                    .web_sm()
                    .label(label.clone())
                    .disabled((self.too_many() || ready == 0) && !self.pending.active())
                    .on_click(cx.listener(|this, _, window, cx| this.run_import(window, cx))),
                &label,
                cx,
            ))
    }
}

impl Render for AppCsvImportView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AppCsvImportView").entered();
        if !self.focused {
            self.focused = true;
            self.text.update(cx, |state, cx| state.focus(window, cx));
        }
        let copy = &APP_CSV_IMPORT.copy;
        let test_ids = &APP_CSV_IMPORT.test_ids;
        let has_text = !self.text.read(cx).value().trim().is_empty();
        let mut body = v_flex()
            .w_full()
            .gap_3()
            .child(
                crate::surface::glass_group().id(SharedString::from(test_ids.text.clone())).child(
                    Textarea::new(&self.text)
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
                    Button::new(SharedString::from(test_ids.file.clone()))
                        .outline()
                        .cursor_pointer()
                        .web_sm()
                        .label(copy.choose_file.clone())
                        .on_click(cx.listener(|this, _, window, cx| this.choose_file(window, cx))),
                ),
            );
        if !self.table.header.is_empty() {
            body = body.child(self.render_map(cx));
        }
        if has_text && self.records.is_empty() {
            body = body.child(div().px_1().text_sm().text_color(cx.theme().muted_foreground).child(copy.no_rows.clone()));
        }
        if !self.records.is_empty() {
            body = body.child(self.render_preview(cx));
        }
        if let Some(message) = self.error.clone() {
            body = body.child(crate::action_error::field_error(message, cx).id(SharedString::from(test_ids.error.clone())).px_1());
        }
        let body_scroll = self.body_scroll.clone();
        v_flex()
            .id(SharedString::from(test_ids.dialog.clone()))
            .size_full()
            .gap_3()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(v_flex().id("app-csv-scroll").size_full().overflow_y_scroll().track_scroll(&body_scroll).child(body))
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
            .child(self.footer(cx))
    }
}
