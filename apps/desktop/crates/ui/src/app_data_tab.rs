//! VAPP-178 — the app editor's DATA tab (web `app-data-tab.tsx`,
//! fixtures/app-records.json `dataTab`): the app's RECORDS of the head
//! package. Toolbar = the kind picker (`{kind} · {n}`, the first kind on
//! open) + `Add record` (outline, `ui-add`) at the end; body = ONE grouped
//! list used as a table: a muted text-xs header row (the schema columns,
//! `by`, `updated`, an empty menu cell), then the picked kind's rows on the
//! same grid (schema columns share the width, `by` / `updated` / the menu
//! fixed px), each cell one truncated line; the trailing `…` opens Edit /
//! Delete (destructive, asks first). Add / Edit = the native dialog's FORM
//! ([`open_form`]): one labelled field per column, Save = `apps.records.
//! upsert` (a BAD_REQUEST shows inline under the fields, anything else is
//! the failed-action toast). The pure model is `domain::app_records`.

use std::collections::HashMap;
use std::sync::LazyLock;

use gpui::{
    div, px, size, AnyElement, App, AppContext as _, ClickEvent, Div, Entity, InteractiveElement as _,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Disableable as _, Icon,
};
use serde_json::Value;
use sync::Store;

use domain::app_records::{
    cell_text, column_width, data_columns, data_kinds, data_rows, form_data, form_fields, form_input, kind_option, kind_schema, table_min_width,
    DataControl, DataFormField, DataSchema, FormInput, RecordRowInput, DATA_TAB,
};
use domain::apps::APPS;
use domain::prompts::{Action, Prompt, Role};

use crate::controls::{glass_input, PointerMenuButtonExt as _, WebControl as _};
use crate::icons::registry;
use crate::native_dialog::{self, DialogContent, DialogSpec};

/// One synced record of the app, its `updated_at` as the web's ISO text
/// (the sort key ×4).
#[derive(Clone)]
struct Record {
    id: String,
    kind: String,
    data: Value,
    created_by: Option<String>,
    updated_at: String,
}

/// The app's head package (the `records` manifest) and its synced records.
fn app_state(app_id: &str, cx: &App) -> (Option<Value>, Vec<Record>) {
    let package = crate::apps_view::find_app(app_id, None, cx)
        .and_then(|entry| entry.row)
        .and_then(|row| row.package)
        .filter(Value::is_object);
    let records = Store::try_global(cx)
        .map(|store| {
            store
                .collections()
                .app_records
                .read(cx)
                .iter()
                .filter(|row| row.app_id == app_id)
                .map(|row| Record {
                    id: row.id.clone(),
                    kind: row.kind.clone(),
                    data: row.data.clone(),
                    created_by: row.created_by.clone(),
                    updated_at: crate::exponential_ui_host::iso_millis(row.updated_at.as_deref().unwrap_or("")),
                })
                .collect()
        })
        .unwrap_or_default();
    (package, records)
}

/// The tab body (an entity: it keeps the picked kind).
pub(crate) struct AppDataTab {
    app_id: String,
    /// The picked kind (`None` / a kind no longer declared = the first).
    kind: Option<String>,
    scroll: ScrollHandle,
    /// The table's sideways scroll.
    table_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

/// A grouped row's side padding (`glass_row_shell`'s 16 + 16).
const TABLE_ROW_PADDING_PX: f32 = 32.;

impl AppDataTab {
    pub(crate) fn new(app_id: String, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        if let Some(collections) = Store::try_global(cx).map(|store| store.collections().clone()) {
            subscriptions.push(cx.observe(&collections.apps, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.app_records, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.users, |_, _, cx| cx.notify()));
        }
        Self { app_id, kind: None, scroll: ScrollHandle::new(), table_scroll: ScrollHandle::new(), _subscriptions: subscriptions }
    }

    fn toolbar(&self, kinds: &[(String, usize)], picked: &str, schema: &DataSchema, cx: &mut gpui::Context<Self>) -> Div {
        let count = kinds.iter().find(|(k, _)| k == picked).map_or(0, |(_, n)| *n);
        let view = cx.entity().downgrade();
        let options: Vec<(String, usize)> = kinds.to_vec();
        let current = picked.to_string();
        let picker = Button::new(SharedString::from(DATA_TAB.test_ids.kind.clone()))
            .outline()
            .web_sm()
            .cursor_pointer()
            .child(div().min_w_0().truncate().child(kind_option(picked, count)))
            .child(Icon::new(registry::UI_CHEVRON_DOWN).size(px(14.)).text_color(cx.theme().muted_foreground))
            .pointer_menu(move |mut menu, _window, _cx| {
                menu = crate::pickers::single_select_menu(menu);
                for (kind, n) in &options {
                    let view = view.clone();
                    let value = kind.clone();
                    menu = menu.item(
                        crate::controls::pointer_label_item(kind_option(kind, *n), false)
                            .checked(*kind == current)
                            .on_click(move |_, _, cx| {
                                let value = value.clone();
                                let _ = view.update(cx, |this, cx| {
                                    this.kind = Some(value);
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            });
        let (app_id, kind, schema) = (self.app_id.clone(), picked.to_string(), schema.clone());
        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(picker)
            .child(div().flex_1())
            .child(
                Button::new(SharedString::from(DATA_TAB.test_ids.add.clone()))
                    .outline()
                    .web_sm()
                    .cursor_pointer()
                    .icon(Icon::new(registry::UI_ADD))
                    .label(DATA_TAB.copy.add_record.clone())
                    .on_click(move |_, window, cx| {
                        open_form(window, cx, app_id.clone(), kind.clone(), schema.clone(), None)
                    }),
            )
    }

    /// One row of the table grid: the schema cells share the width, `by`,
    /// `updated` and the menu cell are fixed.
    /// `widths` = each schema column's fixed px (`column_width`), `None`
    /// = it shares the rest (floor `columns.minWidth`).
    fn grid(widths: &[Option<f32>], columns: Vec<AnyElement>, by: AnyElement, updated: AnyElement, menu: AnyElement) -> Div {
        let fixed = &DATA_TAB.columns;
        h_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(fixed.gap))
            .children(columns.into_iter().zip(widths.iter()).map(|(cell, width)| {
                let slot = match width {
                    Some(width) => div().flex_shrink_0().w(px(*width)),
                    None => div().flex_1().min_w(px(fixed.min_width)),
                };
                slot.overflow_hidden().child(cell)
            }))
            .child(div().flex_shrink_0().w(px(fixed.by_width)).min_w_0().overflow_hidden().child(by))
            .child(div().flex_shrink_0().w(px(fixed.updated_width)).min_w_0().overflow_hidden().child(updated))
            .child(div().flex_shrink_0().w(px(fixed.menu_width)).flex().justify_end().child(menu))
    }

    fn cell(text: impl Into<SharedString>) -> AnyElement {
        div().w_full().min_w_0().truncate().child(text.into()).into_any_element()
    }

    fn row_menu(&self, index: usize, record: &Record, schema: &DataSchema, cx: &App) -> AnyElement {
        let (app_id, record_id, kind, data) =
            (self.app_id.clone(), record.id.clone(), record.kind.clone(), record.data.clone());
        let schema = schema.clone();
        crate::controls::ghost_icon_button(
            SharedString::from(format!("{}-{index}", DATA_TAB.test_ids.menu)),
            Icon::new(registry::UI_MORE),
            cx,
        )
        .pointer_menu(move |menu, _window, cx| {
            let (edit_app, edit_kind, edit_schema, edit_id, edit_data) =
                (app_id.clone(), kind.clone(), schema.clone(), record_id.clone(), data.clone());
            let (delete_app, delete_id) = (app_id.clone(), record_id.clone());
            menu.item(
                crate::controls::pointer_label_item(DATA_TAB.copy.edit.clone(), false)
                    .icon(Icon::new(registry::UI_EDIT))
                    .on_click(move |_: &ClickEvent, window, cx| {
                        open_form(
                            window,
                            cx,
                            edit_app.clone(),
                            edit_kind.clone(),
                            edit_schema.clone(),
                            Some((edit_id.clone(), edit_data.clone())),
                        )
                    }),
            )
            .item(
                crate::controls::danger_menu_item(DATA_TAB.copy.delete.clone(), Icon::new(registry::UI_DELETE), cx)
                    .on_click(move |_: &ClickEvent, window, cx| {
                        confirm_delete(delete_app.clone(), delete_id.clone(), window, cx)
                    }),
            )
        })
        .into_any_element()
    }
}

impl Render for AppDataTab {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let (package, records) = app_state(&self.app_id, cx);
        let kinds = data_kinds(package.as_ref(), records.iter().map(|r| r.kind.as_str()));
        let body = if kinds.is_empty() {
            crate::controls::empty_state_list(Icon::new(registry::NAV_APPS), DATA_TAB.copy.no_kinds.clone(), cx)
                .into_any_element()
        } else {
            let picked = self
                .kind
                .clone()
                .filter(|kind| kinds.iter().any(|(k, _)| k == kind))
                .unwrap_or_else(|| kinds[0].0.clone());
            let schema = kind_schema(package.as_ref(), &picked).unwrap_or_default();
            let columns = data_columns(&schema);
            let widths: Vec<Option<f32>> =
                columns.iter().map(|name| schema.properties.get(name).and_then(column_width)).collect();
            let inputs: Vec<RecordRowInput> = records
                .iter()
                .map(|r| RecordRowInput {
                    id: &r.id,
                    kind: &r.kind,
                    data: &r.data,
                    created_by: r.created_by.as_deref(),
                    created_at: "",
                    updated_at: &r.updated_at,
                })
                .collect();
            let ordered: Vec<&Record> = data_rows(&inputs, &picked)
                .into_iter()
                .filter_map(|row| records.iter().find(|r| r.id == row.id))
                .collect();
            let toolbar = self.toolbar(&kinds, &picked, &schema, cx);
            let list = if ordered.is_empty() {
                crate::controls::empty_state_list(Icon::new(registry::NAV_APPS), DATA_TAB.copy.empty.clone(), cx)
                    .into_any_element()
            } else {
                let muted = cx.theme().muted_foreground;
                let users = Store::try_global(cx).map(|store| store.collections().users.clone());
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let header = crate::surface::glass_row_shell().py_2().text_xs().text_color(muted).child(Self::grid(
                    &widths,
                    columns.iter().map(|name| Self::cell(name.clone())).collect(),
                    Self::cell(DATA_TAB.copy.by.clone()),
                    Self::cell(DATA_TAB.copy.updated.clone()),
                    div().into_any_element(),
                ));
                let mut rows = vec![header];
                for (index, record) in ordered.iter().enumerate() {
                    let author = record
                        .created_by
                        .as_deref()
                        .and_then(|id| users.as_ref()?.read(cx).get(id)?.name.clone())
                        .filter(|name| !name.trim().is_empty());
                    let by = author.unwrap_or_else(|| APPS.copy.by_agent.clone());
                    let updated = crate::comments::relative_time(&record.updated_at, now);
                    let cells = columns
                        .iter()
                        .map(|name| Self::cell(cell_text(record.data.as_object().and_then(|o| o.get(name)))))
                        .collect();
                    rows.push(
                        crate::surface::glass_row_shell().py_1p5().text_sm().child(
                            Self::grid(
                                &widths,
                                cells,
                                div().w_full().min_w_0().truncate().text_color(muted).child(by).into_any_element(),
                                div().w_full().min_w_0().truncate().text_color(muted).child(updated).into_any_element(),
                                self.row_menu(index, record, &schema, cx),
                            )
                            .id(SharedString::from(format!("{}-{}", DATA_TAB.test_ids.row, record.id))),
                        ),
                    );
                }
                // A table wider than the pane scrolls sideways, header and
                // rows together, so the `…` stays reachable.
                let min_width = table_min_width(&widths) + TABLE_ROW_PADDING_PX;
                div()
                    .relative()
                    .w_full()
                    .child(
                        div()
                            .id("app-data-table")
                            .w_full()
                            .overflow_x_scroll()
                            .restrict_scroll_to_axis()
                            .track_scroll(&self.table_scroll)
                            .child(crate::surface::glass_group_rows(rows).w_full().min_w(px(min_width))),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(Scrollbar::new(&self.table_scroll).axis(ScrollbarAxis::Horizontal)),
                    )
                    .into_any_element()
            };
            v_flex().w_full().gap_3().child(toolbar).child(list).into_any_element()
        };
        crate::scroll_pane::v_scroll_pane(
            "app-data-scroll",
            &self.scroll,
            div().id(SharedString::from(APPS.test_ids.data.clone())).w_full().p_4().child(body),
        )
    }
}

/// The delete confirm's two answers (`copy.cancel`, `copy.delete`).
static DELETE_ACTIONS: LazyLock<[Action; 2]> = LazyLock::new(|| {
    [
        Action { id: "cancel", label: DATA_TAB.copy.cancel.as_str(), role: Role::Cancel },
        Action { id: "delete", label: DATA_TAB.copy.delete.as_str(), role: Role::Destructive },
    ]
});

/// `Delete this record?` / `copy.deleteBody`, Return cancels.
fn delete_prompt() -> Prompt {
    Prompt {
        id: "delete-app-record",
        title: DATA_TAB.copy.delete_title.clone(),
        body: Some(DATA_TAB.copy.delete_body.clone()),
        actions: &*DELETE_ACTIONS,
        focus: "cancel",
    }
}

/// Delete asks first; a refused delete = the failed-action toast.
fn confirm_delete(app_id: String, record_id: String, window: &mut Window, cx: &mut App) {
    let spec = native_dialog::AlertSpec::from_prompt(DATA_TAB.copy.delete.clone(), &delete_prompt()).on_ok(
        move |_, cx| {
            let Some(trpc) = crate::queries::trpc_client(cx) else {
                crate::action_error::failed_action("delete", "the record", None, cx);
                return true;
            };
            let (app_id, record_id) = (app_id.clone(), record_id.clone());
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::apps::records_delete(&trpc, &app_id, &record_id) })
                    .await;
                if let Err(err) = result {
                    let _ = cx.update(|cx| crate::action_error::failed_action_err("delete", "the record", &err, cx));
                }
            })
            .detach();
            true
        },
    );
    native_dialog::open_alert(window, cx, spec);
}

/// The form's width and its height per field (a grouped row).
const FORM_WIDTH: f32 = 460.;
const FORM_ROW_PX: f32 = 46.;
const FORM_CHROME_PX: f32 = 120.;

/// Open the Add (`editing` = None) or Edit form for a `kind` record.
pub(crate) fn open_form(
    window: &mut Window,
    cx: &mut App,
    app_id: String,
    kind: String,
    schema: DataSchema,
    editing: Option<(String, Value)>,
) {
    let copy = &DATA_TAB.copy;
    let title = if editing.is_some() { &copy.edit_title } else { &copy.new_title }.replace("{kind}", &kind);
    let fields = form_fields(&schema).len().max(1) as f32;
    let height = (window.viewport_size().height * 0.85).min(px(FORM_CHROME_PX + FORM_ROW_PX * fields));
    let spec = DialogSpec::new(title, size(px(FORM_WIDTH), height)).resizable(size(px(400.), px(240.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| RecordFormView::new(app_id, kind, schema, editing, window, cx));
        let busy = view.clone();
        DialogContent::new(view).self_scrolling().can_close(move |cx| !busy.read(cx).busy)
    });
}

/// One field's live input.
enum FieldState {
    Text(Entity<InputState>),
    Switch(bool),
    Select(String),
}

pub(crate) struct RecordFormView {
    app_id: String,
    kind: String,
    schema: DataSchema,
    /// The record being edited (`None` = create).
    record_id: Option<String>,
    fields: Vec<(DataFormField, FieldState)>,
    busy: bool,
    /// The server's BAD_REQUEST message, inline under the fields.
    error: Option<SharedString>,
    body_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl RecordFormView {
    fn new(
        app_id: String,
        kind: String,
        schema: DataSchema,
        editing: Option<(String, Value)>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let data = editing.as_ref().and_then(|(_, data)| data.as_object().cloned()).unwrap_or_default();
        let mut subscriptions = Vec::new();
        let fields = form_fields(&schema)
            .into_iter()
            .map(|field| {
                let input = form_input(field.control, data.get(&field.name));
                let state = match (field.control, input) {
                    (DataControl::Switch, input) => FieldState::Switch(input == FormInput::Bool(true)),
                    (DataControl::Select, FormInput::Text(text)) => FieldState::Select(text),
                    (control, input) => {
                        let text = match input {
                            FormInput::Text(text) => text,
                            FormInput::Bool(_) => String::new(),
                        };
                        let placeholder = match control {
                            DataControl::List => Some(DATA_TAB.copy.list_hint.clone()),
                            _ => field.placeholder.clone(),
                        };
                        let state = cx.new(|cx| {
                            let mut state = InputState::new(window, cx);
                            if let Some(placeholder) = placeholder {
                                state = state.placeholder(placeholder);
                            }
                            if !text.is_empty() {
                                state.set_value(text, window, cx);
                            }
                            state
                        });
                        subscriptions.push(cx.subscribe(&state, |this: &mut Self, _, event: &InputEvent, cx| {
                            if matches!(event, InputEvent::Change) && this.error.is_some() {
                                this.error = None;
                                cx.notify();
                            }
                        }));
                        FieldState::Text(state)
                    }
                };
                (field, state)
            })
            .collect();
        Self {
            app_id,
            kind,
            schema,
            record_id: editing.map(|(id, _)| id),
            fields,
            busy: false,
            error: None,
            body_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    fn inputs(&self, cx: &App) -> HashMap<String, FormInput> {
        self.fields
            .iter()
            .map(|(field, state)| {
                let input = match state {
                    FieldState::Text(input) => FormInput::Text(input.read(cx).value().to_string()),
                    FieldState::Switch(on) => FormInput::Bool(*on),
                    FieldState::Select(value) => FormInput::Text(value.clone()),
                };
                (field.name.clone(), input)
            })
            .collect()
    }

    fn save(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.busy {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("save", "the record", None, cx);
            return;
        };
        let data = form_data(&self.schema, &self.inputs(cx), self.record_id.is_some());
        let input = api::apps::RecordUpsertInput {
            id: self.app_id.clone(),
            kind: self.kind.clone(),
            data: Value::Object(data),
            record_id: self.record_id.clone(),
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::apps::records_upsert(&trpc, &input) })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(_) => native_dialog::close_dialog_window(window, cx),
                    // The schema's issues stay under the form (invalid input).
                    Err(err @ api::ApiError::Http { status: 400, .. }) => {
                        this.error = Some(
                            crate::settings::form_error(&err, "That didn\u{2019}t go through. Try again.").into(),
                        );
                    }
                    Err(err) => crate::action_error::failed_action_err("save", "the record", &err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The field's label: the name, `optional` muted after an optional one.
    fn label(field: &DataFormField, cx: &App) -> Div {
        h_flex()
            .flex_shrink_0()
            .items_baseline()
            .gap_1p5()
            .text_sm()
            .text_color(cx.theme().foreground)
            .child(field.name.clone())
            .when(!field.required, |label| {
                label.child(
                    div().text_xs().text_color(cx.theme().muted_foreground).child(DATA_TAB.copy.optional.clone()),
                )
            })
    }

    fn select(&self, index: usize, field: &DataFormField, value: &str, cx: &mut gpui::Context<Self>) -> AnyElement {
        let none = DATA_TAB.copy.none.clone();
        let mut options: Vec<(String, String)> = Vec::new();
        if !field.required {
            options.push((String::new(), none.clone()));
        }
        options.extend(field.options.clone().unwrap_or_default().into_iter().map(|o| (o.clone(), o)));
        let picked = options
            .iter()
            .find(|(v, _)| v == value)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| if value.is_empty() { none } else { value.to_string() });
        let current = value.to_string();
        let view = cx.entity().downgrade();
        crate::trigger_editor::picker_trigger(SharedString::from(format!("app-data-field-{index}")), picked, cx)
            .pointer_menu(move |mut menu, _window, _cx| {
                menu = crate::pickers::single_select_menu(menu);
                for (value, label) in &options {
                    let view = view.clone();
                    let value = value.clone();
                    menu = menu.item(
                        crate::controls::pointer_label_item(SharedString::from(label.clone()), false)
                            .checked(value == current)
                            .on_click(move |_, _, cx| {
                                let value = value.clone();
                                let _ = view.update(cx, |this, cx| {
                                    if let Some((_, FieldState::Select(slot))) = this.fields.get_mut(index) {
                                        *slot = value;
                                    }
                                    this.error = None;
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    fn footer(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().flex_1())
            .child(
                Button::new("app-data-cancel")
                    .outline()
                    .cursor_pointer()
                    .web_sm()
                    .label(DATA_TAB.copy.cancel.clone())
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !this.busy {
                            native_dialog::close_dialog_window(window, cx);
                        }
                    })),
            )
            .child(
                Button::new(SharedString::from(DATA_TAB.test_ids.save.clone()))
                    .primary()
                    .cursor_pointer()
                    .web_sm()
                    .label(DATA_TAB.copy.save.clone())
                    .loading(self.busy)
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
            )
    }
}

impl Render for RecordFormView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let mut rows: Vec<Div> = Vec::new();
        for index in 0..self.fields.len() {
            let (field, state) = &self.fields[index];
            let field = field.clone();
            let control: AnyElement = match state {
                FieldState::Text(input) => div()
                    .flex_1()
                    .min_w_0()
                    .child(crate::surface::glass_row_input(glass_input(input, window, cx)))
                    .into_any_element(),
                FieldState::Switch(on) => h_flex()
                    .flex_1()
                    .justify_end()
                    .child(
                        crate::controls::web_switch(SharedString::from(format!("app-data-field-{index}")))
                            .checked(*on)
                            .on_click(cx.listener(move |this, on: &bool, _, cx| {
                                if let Some((_, FieldState::Switch(slot))) = this.fields.get_mut(index) {
                                    *slot = *on;
                                }
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
                FieldState::Select(value) => {
                    let value = value.clone();
                    h_flex().flex_1().min_w_0().justify_end().child(self.select(index, &field, &value, cx)).into_any_element()
                }
            };
            rows.push(crate::surface::glass_row_shell().child(Self::label(&field, cx)).child(control));
        }
        let mut body = v_flex().w_full().gap_2().child(crate::surface::glass_group_rows(rows));
        if let Some(message) = self.error.clone() {
            body = body.child(
                crate::action_error::field_error(message, cx).id(SharedString::from(DATA_TAB.test_ids.error.clone())).px_1(),
            );
        }
        let body_scroll = self.body_scroll.clone();
        v_flex()
            .id(SharedString::from(DATA_TAB.test_ids.form.clone()))
            .size_full()
            .gap_3()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .id("app-data-form-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&body_scroll)
                            .child(body),
                    )
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

use gpui::prelude::FluentBuilder as _;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_delete_prompt_reads_the_fixture() {
        let prompt = delete_prompt();
        assert_eq!(prompt.title, "Delete this record?");
        assert_eq!(prompt.focused().role, Role::Cancel);
        assert_eq!(DELETE_ACTIONS[1].label, "Delete");
    }
}
