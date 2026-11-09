//! VAPP-178 — the app editor's DATA tab (web `app-data-tab.tsx`,
//! fixtures/app-records.json `dataTab`): the app's RECORDS of the head
//! package. Toolbar = the kind picker (`{kind} · {n}`, the first kind on
//! open) + `Add record` (outline, `ui-add`) at the end; body = ONE grouped
//! list used as a table: a muted text-xs header row (the schema columns,
//! `by`, `updated`, an empty menu cell), then the picked kind's rows on the
//! same grid (schema columns share the width, `by` / `updated` / the menu
//! fixed px), each cell one truncated line; the trailing `…` opens Edit /
//! Delete (destructive, asks first); VAPP-187: a row the viewer may not
//! change (eApps R3: the kind's policy under the viewer's app access, eUI's
//! record-policy.json; by default neither its author nor a team owner) keeps
//! its `…` DISABLED, and `by` names the AUTHOR (VAPP-188 `record_byline`:
//! `{writer} for {author}` when an owner wrote it for them). For a team
//! OWNER the form's first field is `copy.author` (the select control over
//! the team's members, `author_options`); Save sends `author` only when the
//! pick differs (`form_author`). Add / Edit = the native dialog's FORM
//! ([`open_form`]): one labelled field per column, Save = `apps.records.
//! upsert` (a BAD_REQUEST shows inline under the fields, anything else is
//! the failed-action toast). The pure model is `domain::app_records`.
//! VAPP-215 (`history`): every add / save / delete toasts `Undo` for
//! `undoSeconds` s ([`toast_record_undo`], `apps.records.undo`); Edit lists
//! the record's History under the fields (`apps.records.history`, read on
//! open: who · when · what), Restore on every older row but deletes.

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
    author_options, cell_text, column_width, data_columns, data_kinds, data_rows, form_data, form_fields, form_input, kind_option, kind_schema, table_min_width,
    form_author, history_rows, record_byline, undo_title, DataControl, DataFormField, DataSchema, FormInput, RecordEvent,
    RecordRowInput, UndoableWrite, APP_CSV_IMPORT, APP_RECORD_HISTORY, DATA_TAB, CsvMember,
};
use domain::apps::APPS;
use eui_core::host::{record_policy, record_refusal, RecordOp as PolicyOp, RecordViewer};
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
    /// VAPP-187: whose record it is (`None` = `created_by`).
    author_id: Option<String>,
    /// #VAPP-229: the app whose tool created it (the By cell's `via <app>`).
    via_app_id: Option<String>,
    /// VAPP-241: the public link that created it (the By cell reads Guest).
    share_name: Option<String>,
    updated_at: String,
}

/// VAPP-187: the viewer (signed-in user id) and their role in the app's
/// team (`None` = not a member), and the head package (eApps R3: each
/// kind's policy).
#[derive(Clone, Default)]
struct Viewer {
    user_id: Option<String>,
    role: Option<String>,
    /// The app's team (VAPP-188: the Author picker's members).
    team_id: Option<String>,
    /// eApps R3: the viewer's app access (`None` = write until fetched).
    access: Option<String>,
    package: Option<std::sync::Arc<Value>>,
}

impl Viewer {
    /// eApps R3: may the viewer `op` (a record of) `kind` under its policy
    /// and their app access (record-policy.json)?
    fn may(&self, kind: &str, op: PolicyOp, record: Option<&Record>) -> bool {
        let policy = record_policy(self.package.as_deref().and_then(|p| p.get("records")).and_then(|r| r.get(kind)));
        let row = record.map(|r| serde_json::json!({"authorId": r.author_id, "createdBy": r.created_by}));
        let viewer = RecordViewer { id: self.user_id.as_deref(), role: self.role.as_deref(), access: self.access.as_deref() };
        record_refusal(&policy, op, &viewer, row.as_ref()).is_none()
    }

}

/// The app's head package (the `records` manifest), its synced records and
/// the viewer.
fn app_state(app_id: &str, access: Option<String>, cx: &App) -> (Option<Value>, Vec<Record>, Viewer) {
    let row = crate::apps_view::find_app(app_id, None, cx).and_then(|entry| entry.row);
    let team_id = row.as_ref().and_then(|row| row.team_id.clone());
    let viewer = Viewer {
        user_id: crate::queries::active_account(cx).map(|account| account.user_id),
        role: team_id
            .as_deref()
            .filter(|_| Store::try_global(cx).is_some())
            .and_then(|team_id| crate::settings::my_membership(cx, team_id))
            .map(|(_, role)| role),
        team_id,
        access,
        package: None,
    };
    // eApps V1 (#VAPP-210): the editor's Data tab works on the HEAD's kinds
    // (the draft while ahead, else the published package).
    let package = row.and_then(|row| row.draft_package.filter(Value::is_object).or(row.package)).filter(Value::is_object);
    let viewer = Viewer { package: package.clone().map(std::sync::Arc::new), ..viewer };
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
                    author_id: row.author_id.clone(),
                    via_app_id: row.via_app_id.clone(),
                    share_name: row.share_name.clone(),
                    updated_at: crate::exponential_ui_host::iso_millis(row.updated_at.as_deref().unwrap_or("")),
                })
                .collect()
        })
        .unwrap_or_default();
    (package, records, viewer)
}

/// VAPP-228: an OWNER's team members for the CSV import's Author column
/// (`None` = not an owner: no Author pick).
fn csv_members(viewer: &Viewer, cx: &App) -> Option<Vec<CsvMember>> {
    if viewer.role.as_deref() != Some("owner") {
        return None;
    }
    let (team_id, store) = (viewer.team_id.as_deref()?, Store::try_global(cx)?);
    let collections = store.collections();
    let users = collections.users.read(cx);
    Some(
        collections
            .team_members
            .read(cx)
            .iter()
            .filter(|member| member.team_id == team_id)
            .map(|member| {
                let user = users.get(&member.user_id);
                CsvMember {
                    id: member.user_id.clone(),
                    name: user.and_then(|u| u.name.clone()).unwrap_or_default(),
                    email: user.and_then(|u| u.email.clone()).unwrap_or_default(),
                }
            })
            .collect(),
    )
}

/// The tab body (an entity: it keeps the picked kind).
pub(crate) struct AppDataTab {
    app_id: String,
    /// The picked kind (`None` / a kind no longer declared = the first).
    kind: Option<String>,
    scroll: ScrollHandle,
    /// The table's sideways scroll.
    table_scroll: ScrollHandle,
    /// eApps R3: the viewer's app access (`appAccess.mine`; `None` until known = write).
    access: Option<String>,
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
            subscriptions.push(cx.observe(&collections.team_members, |_, _, cx| cx.notify()));
        }
        if let Some(trpc) = crate::queries::trpc_client(cx) {
            let id = app_id.clone();
            cx.spawn(async move |this, cx| {
                let result = cx.background_executor().spawn(async move { api::app_access::mine(&trpc, &id) }).await;
                match result {
                    Ok(out) => {
                        let _ = this.update(cx, |this, cx| {
                            this.access = Some(out.level);
                            cx.notify();
                        });
                    }
                    Err(err) => log::warn!("[ui] appAccess.mine failed: {err}"),
                }
            })
            .detach();
        }
        Self { app_id, kind: None, scroll: ScrollHandle::new(), table_scroll: ScrollHandle::new(), access: None, _subscriptions: subscriptions }
    }

    /// eApps R3: the viewer's app access (`None` until known = write).
    pub(crate) fn access(&self) -> Option<&str> {
        self.access.as_deref()
    }

    fn toolbar(
        &self,
        kinds: &[(String, usize)],
        picked: &str,
        schema: &DataSchema,
        raw_schema: Value,
        viewer: &Viewer,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let count = kinds.iter().find(|(k, _)| k == picked).map_or(0, |(_, n)| *n);
        let view = cx.entity().downgrade();
        let options: Vec<(String, usize)> = kinds.to_vec();
        let current = picked.to_string();
        let picker = Button::new(SharedString::from(DATA_TAB.test_ids.kind.clone()))
            .accessibility_label(kind_option(picked, count))
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
        let may_add = viewer.may(picked, PolicyOp::Create, None);
        // VAPP-228: `Import…` (CSV) only while the viewer may create.
        let import = may_add.then(|| {
            let (app_id, kind, schema) = (self.app_id.clone(), picked.to_string(), schema.clone());
            let (me, members) = (viewer.user_id.clone(), csv_members(viewer, cx));
            Button::new(SharedString::from(APP_CSV_IMPORT.test_ids.button.clone()))
                .outline()
                .web_sm()
                .cursor_pointer()
                .icon(Icon::new(registry::UI_FILE))
                .label(APP_CSV_IMPORT.copy.import_button.clone())
                .on_click(move |_, window, cx| {
                    let target = crate::app_csv_import_dialog::CsvImportTarget {
                        app_id: app_id.clone(),
                        kind: kind.clone(),
                        schema: schema.clone(),
                        raw_schema: raw_schema.clone(),
                        me: me.clone(),
                        members: members.clone(),
                    };
                    crate::app_csv_import_dialog::open(window, cx, target)
                })
        });
        let (app_id, kind, schema, viewer) = (self.app_id.clone(), picked.to_string(), schema.clone(), viewer.clone());
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
                    .disabled(!may_add)
                    .on_click(move |_, window, cx| {
                        open_form(window, cx, app_id.clone(), kind.clone(), schema.clone(), None, &viewer)
                    }),
            )
            .children(import)
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

    fn row_menu(&self, index: usize, record: &Record, schema: &DataSchema, viewer: &Viewer, cx: &App) -> AnyElement {
        // eApps R3: each item only when its op is allowed (record-policy.json).
        let (may_update, may_delete) = (viewer.may(&record.kind, PolicyOp::Update, Some(record)), viewer.may(&record.kind, PolicyOp::Delete, Some(record)));
        let allowed = may_update || may_delete;
        let (app_id, viewer, edited) = (self.app_id.clone(), viewer.clone(), record.clone());
        let schema = schema.clone();
        let button = crate::controls::ghost_icon_button(
            SharedString::from(format!("{}-{index}", DATA_TAB.test_ids.menu)),
            Icon::new(registry::UI_MORE),
            cx,
        )
        .accessibility_label(format!("{} / {}", DATA_TAB.copy.edit, DATA_TAB.copy.delete));
        // VAPP-187: not the viewer's to change = the same `…`, disabled, no menu.
        if !allowed {
            return button.disabled(true).into_any_element();
        }
        button
        .pointer_menu(move |menu, _window, cx| {
            let (edit_app, edit_schema, edit_viewer, edit_record) =
                (app_id.clone(), schema.clone(), viewer.clone(), edited.clone());
            let (delete_app, delete_id) = (app_id.clone(), edited.id.clone());
            let menu = if !may_update {
                menu
            } else {
                menu.item(
                crate::controls::pointer_label_item(DATA_TAB.copy.edit.clone(), false)
                    .icon(Icon::new(registry::UI_EDIT))
                    .on_click(move |_: &ClickEvent, window, cx| {
                        open_form(
                            window,
                            cx,
                            edit_app.clone(),
                            edit_record.kind.clone(),
                            edit_schema.clone(),
                            Some(&edit_record),
                            &edit_viewer,
                        )
                    }),
            )
            };
            if !may_delete {
                return menu;
            }
            menu.item(
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
        let _span = tracing::trace_span!("render", view = "AppDataTab").entered();
        let (package, records, viewer) = app_state(&self.app_id, self.access.clone(), cx);
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
                    author_id: r.author_id.as_deref(),
                    created_at: "",
                    updated_at: &r.updated_at,
                })
                .collect();
            let ordered: Vec<&Record> = data_rows(&inputs, &picked)
                .into_iter()
                .filter_map(|row| records.iter().find(|r| r.id == row.id))
                .collect();
            let raw_schema = package
                .as_ref()
                .and_then(|p| p.get("records")?.get(&picked)?.get("schema").cloned())
                .unwrap_or_else(|| serde_json::json!({}));
            let toolbar = self.toolbar(&kinds, &picked, &schema, raw_schema, &viewer, cx);
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
                    let by = record_byline(
                        record.author_id.as_deref(),
                        record.created_by.as_deref(),
                        |id| users.as_ref()?.read(cx).get(id)?.name.clone().filter(|name| !name.trim().is_empty()),
                        &APPS.copy.by_agent,
                    );
                    // VAPP-241 (app-share.json `bylineCases`): a guest's record.
                    let by = domain::app_share::share_byline(record.share_name.as_deref(), &by);
                    // #VAPP-229 (via-app.json): `Ana · via Timesheet` while
                    // the creating app syncs; one truncated line.
                    let by = domain::via_app::with_via_app(
                        &by,
                        crate::comments::synced_app_name(record.via_app_id.as_deref(), cx).as_deref(),
                    );
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
                                self.row_menu(index, record, &schema, &viewer, cx),
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
                    .spawn({
                        let app_id = app_id.clone();
                        async move { api::apps::records_delete(&trpc, &app_id, &record_id, Some("editor"), None) }
                    })
                    .await;
                let _ = cx.update(|cx| match result {
                    Ok(out) => toast_record_undo(&app_id, UndoableWrite::Delete, out.event_id.into_iter().collect(), "editor", cx),
                    Err(err) => crate::action_error::failed_action_err("delete", "the record", &err, cx),
                });
            })
            .detach();
            true
        },
    );
    native_dialog::open_alert(window, cx, spec);
}

/// VAPP-215: a UI records write's Undo toast (web `toastRecordUndo`): the
/// success toast, title `undo_title`, the toast action `copy.undo`, for
/// `undoSeconds` s. No events (a batch that changed nothing) = no toast.
pub(crate) fn toast_record_undo(app_id: &str, write: UndoableWrite, event_ids: Vec<String>, via: &'static str, cx: &mut App) {
    toast_record_undo_titled(app_id, undo_title(write), event_ids, via, cx)
}

/// [`toast_record_undo`] under its own title (VAPP-228: the CSV import's
/// `copy.imported`, web `toastRecordUndo`'s `title`).
pub(crate) fn toast_record_undo_titled(
    app_id: &str,
    title: impl Into<SharedString>,
    event_ids: Vec<String>,
    via: &'static str,
    cx: &mut App,
) {
    if event_ids.is_empty() {
        return;
    }
    let app_id = app_id.to_string();
    let toast = crate::toast::Toast::success(title)
        .duration(std::time::Duration::from_secs(APP_RECORD_HISTORY.undo_seconds))
        .action(APP_RECORD_HISTORY.copy.undo.clone(), move |_, cx| {
            undo_record_write(app_id.clone(), event_ids.clone(), via, cx)
        });
    crate::toast::show_in_active_window(toast, cx);
}

/// Run the undo (`apps.records.undo`); a refusal = the failed-action toast
/// (`Couldn't undo the change`).
fn undo_record_write(app_id: String, event_ids: Vec<String>, via: &'static str, cx: &mut App) {
    let copy = &APP_RECORD_HISTORY.copy;
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action(&copy.undo_verb, &copy.undo_thing, None, cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::apps::records_undo(&trpc, &app_id, &event_ids, via) })
            .await;
        if let Err(err) = result {
            let copy = &APP_RECORD_HISTORY.copy;
            let _ = cx.update(|cx| crate::action_error::failed_action_err(&copy.undo_verb, &copy.undo_thing, &err, cx));
        }
    })
    .detach();
}

/// The form's width and its height per field (a grouped row).
const FORM_WIDTH: f32 = 460.;
const FORM_ROW_PX: f32 = 46.;
const FORM_CHROME_PX: f32 = 120.;
/// VAPP-215: Edit's History section (the head + a few rows).
const FORM_HISTORY_PX: f32 = 180.;

/// VAPP-188: the owner's Author field: the team's members by name, the
/// record (`None` = create) and the pick.
struct AuthorField {
    viewer: Option<String>,
    /// The edited row's `(author_id, created_by)`.
    record: Option<(Option<String>, Option<String>)>,
    /// `(user id, display name)`, `author_options` order.
    options: Vec<(String, String)>,
    picked: String,
}

impl AuthorField {
    /// `Some` for a team OWNER only (non-owners never name an author).
    fn new(viewer: &Viewer, record: Option<&Record>, cx: &App) -> Option<Self> {
        if viewer.role.as_deref() != Some("owner") {
            return None;
        }
        let (team_id, store) = (viewer.team_id.as_deref()?, Store::try_global(cx)?);
        let collections = store.collections();
        let users = collections.users.read(cx);
        let members = collections
            .team_members
            .read(cx)
            .iter()
            .filter(|member| member.team_id == team_id)
            .map(|member| {
                let name = users
                    .get(&member.user_id)
                    .map(crate::issue_draft::display_name)
                    .unwrap_or_else(|| domain::member_fallback_label(&member.user_id));
                (member.user_id.clone(), name)
            })
            .collect();
        let record = record.map(|r| (r.author_id.clone(), r.created_by.clone()));
        let picked = form_author(viewer.user_id.as_deref(), Self::pair(&record), None).default.unwrap_or_default().to_string();
        Some(Self { viewer: viewer.user_id.clone(), record, options: author_options(members), picked })
    }

    fn pair(record: &Option<(Option<String>, Option<String>)>) -> Option<(Option<&str>, Option<&str>)> {
        record.as_ref().map(|(author_id, created_by)| (author_id.as_deref(), created_by.as_deref()))
    }

    /// What Save sends as `author` (`None` = omit).
    fn send(&self) -> Option<String> {
        let picked = Some(self.picked.as_str()).filter(|p| !p.is_empty());
        form_author(self.viewer.as_deref(), Self::pair(&self.record), picked).send.map(str::to_string)
    }
}

/// Open the Add (`editing` = None) or Edit form for a `kind` record.
fn open_form(
    window: &mut Window,
    cx: &mut App,
    app_id: String,
    kind: String,
    schema: DataSchema,
    editing: Option<&Record>,
    viewer: &Viewer,
) {
    let copy = &DATA_TAB.copy;
    let title = if editing.is_some() { &copy.edit_title } else { &copy.new_title }.replace("{kind}", &kind);
    let author = AuthorField::new(viewer, editing, cx);
    let editing = editing.map(|record| (record.id.clone(), record.data.clone()));
    let fields = (form_fields(&schema).len() + usize::from(author.is_some())).max(1) as f32;
    let history = if editing.is_some() { FORM_HISTORY_PX } else { 0. };
    let height = (window.viewport_size().height * 0.85).min(px(FORM_CHROME_PX + FORM_ROW_PX * fields + history));
    let spec = DialogSpec::new(title, size(px(FORM_WIDTH), height)).resizable(size(px(400.), px(240.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| RecordFormView::new(app_id, kind, schema, editing, author, window, cx));
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

/// Which select a pick lands in.
#[derive(Clone, Copy)]
enum SelectSlot {
    Field(usize),
    /// VAPP-188: the owner's Author field.
    Author,
}

pub(crate) struct RecordFormView {
    app_id: String,
    kind: String,
    schema: DataSchema,
    /// The record being edited (`None` = create).
    record_id: Option<String>,
    fields: Vec<(DataFormField, FieldState)>,
    /// VAPP-188: the owner's Author field (`None` = not an owner).
    author: Option<AuthorField>,
    busy: bool,
    /// The server's BAD_REQUEST message, inline under the fields.
    error: Option<SharedString>,
    /// VAPP-215: the edited record's events, newest first (`None` = loading).
    history: Option<Vec<RecordEvent>>,
    /// The event a Restore is writing.
    restoring: Option<String>,
    body_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl RecordFormView {
    fn new(
        app_id: String,
        kind: String,
        schema: DataSchema,
        editing: Option<(String, Value)>,
        author: Option<AuthorField>,
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
        let record_id = editing.map(|(id, _)| id);
        if let Some(record_id) = record_id.clone() {
            Self::load_history(app_id.clone(), record_id, cx);
        }
        Self {
            app_id,
            kind,
            schema,
            record_id,
            fields,
            author,
            busy: false,
            error: None,
            history: None,
            restoring: None,
            body_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// VAPP-215: read the record's History once on open (a failure = none).
    fn load_history(app_id: String, record_id: String, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.history = Some(Vec::new());
                    cx.notify();
                });
            })
            .detach();
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::apps::records_history(&trpc, &app_id, &record_id) })
                .await;
            let events = match result {
                Ok(out) => out.events.iter().filter_map(RecordEvent::from_value).collect(),
                Err(err) => {
                    log::warn!("[ui] apps.records.history failed: {err}");
                    Vec::new()
                }
            };
            let _ = this.update(cx, |this, cx| {
                this.history = Some(events);
                cx.notify();
            });
        })
        .detach();
    }

    /// VAPP-215: History's Restore (`apps.records.restore`): the record as
    /// that event left it; then the form closes and toasts Undo.
    fn restore(&mut self, event_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.restoring.is_some() || self.busy {
            return;
        }
        let (Some(trpc), Some(record_id)) = (crate::queries::trpc_client(cx), self.record_id.clone()) else {
            crate::action_error::failed_action("restore", "the record", None, cx);
            return;
        };
        let app_id = self.app_id.clone();
        self.restoring = Some(event_id.clone());
        cx.notify();
        cx.spawn_in(window, async move |this, window| {
            let call_app = app_id.clone();
            let result = window
                .background_executor()
                .spawn(async move { api::apps::records_restore(&trpc, &call_app, &record_id, &event_id, "editor") })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.restoring = None;
                match result {
                    Ok(out) => {
                        native_dialog::close_dialog_window(window, cx);
                        toast_record_undo(&app_id, UndoableWrite::Update, out.event_ids, "editor", cx);
                    }
                    Err(err) => crate::action_error::failed_action_err("restore", "the record", &err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// VAPP-215: the History section (web `RecordHistory`): the muted list
    /// head over the grouped list, a row = the label over a muted `who ·
    /// relative time`, a trailing ghost `Restore` when restorable; no
    /// events = the list-size empty state.
    fn render_history(&self, cx: &mut gpui::Context<Self>) -> Div {
        let copy = &APP_RECORD_HISTORY.copy;
        let header = crate::surface::glass_section_header_loading(copy.history.clone(), self.history.is_none(), None, cx);
        let section = v_flex().id(SharedString::from(DATA_TAB.test_ids.history.clone())).w_full().gap_1p5().pt_2().child(header);
        let Some(events) = &self.history else {
            return div().w_full().child(section);
        };
        let users = Store::try_global(cx).map(|store| store.collections().users.clone());
        let rows = history_rows(
            events,
            |id| users.as_ref()?.read(cx).get(id)?.name.clone().filter(|name| !name.trim().is_empty()),
            &APPS.copy.by_agent,
        );
        if rows.is_empty() {
            return div().w_full().child(
                section.child(crate::controls::empty_state_list(Icon::new(registry::UI_HISTORY), copy.empty.clone(), cx)),
            );
        }
        let muted = cx.theme().muted_foreground;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let rows: Vec<Div> = rows
            .into_iter()
            .map(|row| {
                let when = crate::comments::relative_time(&row.at, now);
                let caption = if when.is_empty() { row.who.clone() } else { format!("{} · {when}", row.who) };
                let restore = row.restorable.then(|| {
                    let event_id = row.id.clone();
                    Button::new(SharedString::from(format!("{}-{}", DATA_TAB.test_ids.restore, row.id)))
                        .ghost()
                        .web_sm()
                        .cursor_pointer()
                        .label(copy.restore.clone())
                        .loading(self.restoring.as_deref() == Some(row.id.as_str()))
                        .disabled(self.restoring.is_some())
                        .on_click(cx.listener(move |this, _, window, cx| this.restore(event_id.clone(), window, cx)))
                });
                crate::surface::glass_row_shell().py_2().child(
                    h_flex()
                        .id(SharedString::from(format!("{}-{}", DATA_TAB.test_ids.history_row, row.id)))
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap_3()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_0p5()
                                .child(div().min_w_0().truncate().text_sm().child(row.label))
                                .child(div().min_w_0().truncate().text_xs().text_color(muted).child(caption)),
                        )
                        .children(restore),
                )
            })
            .collect();
        div().w_full().child(section.child(crate::surface::glass_group_rows(rows)))
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
            author: self.author.as_ref().and_then(AuthorField::send),
            via: Some("editor".to_string()),
            new_id: None,
            write_id: None,
            via_app: None,
        };
        let app_id = self.app_id.clone();
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
                    Ok(out) => {
                        native_dialog::close_dialog_window(window, cx);
                        let write = if out.created { UndoableWrite::Create } else { UndoableWrite::Update };
                        toast_record_undo(&app_id, write, out.event_id.into_iter().collect(), "editor", cx);
                    }
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
        Self::label_text(field.name.clone(), field.required, cx)
    }

    pub(crate) fn label_text(name: String, required: bool, cx: &App) -> Div {
        h_flex()
            .flex_shrink_0()
            .items_baseline()
            .gap_1p5()
            .text_sm()
            .text_color(cx.theme().foreground)
            .child(name)
            .when(!required, |label| {
                label.child(
                    div().text_xs().text_color(cx.theme().muted_foreground).child(DATA_TAB.copy.optional.clone()),
                )
            })
    }

    fn select(&self, index: usize, field: &DataFormField, value: &str, cx: &mut gpui::Context<Self>) -> AnyElement {
        let mut options: Vec<(String, String)> = Vec::new();
        if !field.required {
            options.push((String::new(), DATA_TAB.copy.none.clone()));
        }
        options.extend(field.options.clone().unwrap_or_default().into_iter().map(|o| (o.clone(), o)));
        Self::select_control(SharedString::from(format!("app-data-field-{index}")), options, value, SelectSlot::Field(index), cx)
    }

    /// The form's select (a `select` field, VAPP-188 the Author field):
    /// `(value, label)` options, the pick lands in `slot`.
    fn select_control(
        id: SharedString,
        options: Vec<(String, String)>,
        value: &str,
        slot: SelectSlot,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let none = DATA_TAB.copy.none.clone();
        let picked = options
            .iter()
            .find(|(v, _)| v == value)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| if value.is_empty() { none } else { value.to_string() });
        let current = value.to_string();
        let view = cx.entity().downgrade();
        crate::trigger_editor::picker_trigger(id, picked, cx)
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
                                    match slot {
                                        SelectSlot::Field(index) => {
                                            if let Some((_, FieldState::Select(field))) = this.fields.get_mut(index) {
                                                *field = value;
                                            }
                                        }
                                        SelectSlot::Author => {
                                            if let Some(author) = this.author.as_mut() {
                                                author.picked = value;
                                            }
                                        }
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
        let _span = tracing::trace_span!("render", view = "RecordFormView").entered();
        let mut rows: Vec<Div> = Vec::new();
        // VAPP-188: an owner's FIRST field = Author (required, no None).
        if let Some(author) = &self.author {
            let (options, picked) = (author.options.clone(), author.picked.clone());
            let control = Self::select_control(SharedString::from("app-data-field-author"), options, &picked, SelectSlot::Author, cx);
            rows.push(
                crate::surface::glass_row_shell()
                    .child(Self::label_text(DATA_TAB.copy.author.clone(), true, cx))
                    .child(h_flex().flex_1().min_w_0().justify_end().child(control)),
            );
        }
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
        if self.record_id.is_some() {
            body = body.child(self.render_history(cx));
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
