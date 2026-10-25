//! VAPP-121 — Export an app (web `app-export-dialog.tsx`,
//! fixtures/app-export.json): ONE dialog titled `Export {name}`, a single
//! "Include records" switch row (only when the app has synced records, off by
//! default) and the Export button, which builds `<slug>.app.json` from the
//! synced app row + records ([`domain::app_export::build_app_export`]) and
//! saves it through the native save panel. Opened from the apps list row's
//! `…` menu, the editor header and the app's settings page; no version yet =
//! no Export. A failed save = the failed-action toast.

use gpui::{
    div, px, size, App, AppContext as _, IntoElement, ParentElement, Render, Styled, Subscription,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Disableable as _,
};
use sync::Store;

use domain::app_export::{
    app_export_file_name, app_export_text, app_export_title, build_app_export, ExportApp, ExportRecord,
    APP_EXPORT_COPY,
};
use domain::rows::AppRow;

use crate::controls::WebControl as _;
use crate::native_dialog::{self, DialogContent, DialogSpec};

/// The synced app row (trashed rows hidden).
fn synced_row(app_id: &str, cx: &App) -> Option<AppRow> {
    crate::apps_view::find_app(app_id, None, cx).and_then(|entry| entry.row)
}

/// The app's synced records, as the export reads them.
fn app_records(app_id: &str, cx: &App) -> Vec<ExportRecord> {
    let Some(store) = Store::try_global(cx) else { return Vec::new() };
    store
        .collections()
        .app_records
        .read(cx)
        .iter()
        .filter(|record| record.app_id == app_id)
        .map(|record| ExportRecord {
            id: record.id.clone(),
            kind: record.kind.clone(),
            data: record.data.clone(),
            created_at: record.created_at.clone().unwrap_or_default(),
        })
        .collect()
}

/// The export file's text for a synced app row; None without a package.
pub(crate) fn export_text(row: &AppRow, records: &[ExportRecord], include_records: bool) -> Option<String> {
    let package = row.package.as_ref().filter(|p| !p.is_null())?;
    let app = ExportApp {
        name: row.name.clone(),
        icon: row.icon.clone(),
        color: row.color.clone(),
        description: row.description.clone(),
    };
    Some(app_export_text(&build_app_export(&app, package, records, include_records)))
}

/// Open the Export dialog for `app_id` (`include_records` = the switch's
/// starting state: off from every entry point, on for the capture hook).
pub(crate) fn open(window: &mut Window, cx: &mut App, app_id: String, include_records: bool) {
    let Some(row) = synced_row(&app_id, cx) else { return };
    let has_records = !app_records(&app_id, cx).is_empty();
    let height = if has_records { 150. } else { 96. };
    let spec = DialogSpec::new(app_export_title(&row.name), size(px(416.), px(height)));
    native_dialog::open_dialog_window(window, cx, spec, move |_, cx| {
        let view = cx.new(|cx| AppExportView::new(app_id, include_records, cx));
        let submit = view.clone();
        DialogContent::new(view).on_enter(move |window, cx| submit.update(cx, |view, cx| view.export(window, cx)))
    });
}

pub(crate) struct AppExportView {
    app_id: String,
    include_records: bool,
    saving: bool,
    _subscriptions: Vec<Subscription>,
}

impl AppExportView {
    fn new(app_id: String, include_records: bool, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        if let Some(collections) = Store::try_global(cx).map(|store| store.collections().clone()) {
            subscriptions.push(cx.observe(&collections.apps, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.app_records, |_, _, cx| cx.notify()));
        }
        Self { app_id, include_records, saving: false, _subscriptions: subscriptions }
    }

    fn export(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.saving {
            return;
        }
        let Some(row) = synced_row(&self.app_id, cx) else { return };
        let records = app_records(&self.app_id, cx);
        let Some(text) = export_text(&row, &records, self.include_records) else {
            crate::action_error::failed_action("export", "the app", None, cx);
            return;
        };
        let name = app_export_file_name(&row.slug);
        let directory = dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let receiver = crate::file_picker::prompt_for_new_path(cx, &directory, Some(&name));
        self.saving = true;
        cx.notify();
        cx.spawn_in(window, async move |this, window| {
            // Receiver error = dialog dismissed/unsupported; None = cancelled.
            let Ok(Ok(Some(path))) = receiver.await else {
                let _ = this.update(window, |this, cx| {
                    this.saving = false;
                    cx.notify();
                });
                return;
            };
            let result = window
                .background_executor()
                .spawn(async move { std::fs::write(&path, text) })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(()) => native_dialog::close_dialog_window(window, cx),
                    Err(error) => {
                        log::warn!("[ui] app export failed: {error}");
                        crate::action_error::failed_action("export", "the app", Some(error.to_string()), cx);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

impl Render for AppExportView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let row = synced_row(&self.app_id, cx);
        let has_records = !app_records(&self.app_id, cx).is_empty();
        let has_package = row.as_ref().and_then(|row| row.package.as_ref()).is_some_and(|p| !p.is_null());
        let label = APP_EXPORT_COPY.export.clone();
        v_flex()
            .size_full()
            .gap_4()
            .when(has_records, |this| {
                this.child(crate::surface::glass_group_rows(vec![crate::surface::glass_toggle_row(
                    APP_EXPORT_COPY.include_records.clone(),
                    None,
                    crate::controls::web_switch("app-export-records")
                        .checked(self.include_records)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.include_records = *checked;
                            cx.notify();
                        }))
                        .into_any_element(),
                    cx,
                )]))
            })
            .child(div().flex_1())
            .child(
                h_flex()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .justify_end()
                    .child(
                        Button::new("app-export-confirm")
                            .primary()
                            .cursor_pointer()
                            .web_sm()
                            .label(label)
                            .loading(self.saving)
                            .disabled(!has_package || self.saving)
                            .on_click(cx.listener(|this, _, window, cx| this.export(window, cx))),
                    ),
            )
    }
}

use gpui::prelude::FluentBuilder as _;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_export_text_reads_the_synced_row() {
        let row: AppRow = serde_json::from_value(json!({
            "id": "a", "slug": "todos", "name": "Todos", "package": {"templates": {"main": {}}}, "head_version": 1
        }))
        .unwrap();
        let records = vec![
            ExportRecord { id: "r2".into(), kind: "todo".into(), data: json!({"t": 2}), created_at: "2026-10-05 18:00:00+00".into() },
            ExportRecord { id: "r1".into(), kind: "todo".into(), data: json!({"t": 1}), created_at: "2026-10-05T17:00:00Z".into() },
        ];
        let text = export_text(&row, &records, true).unwrap();
        let file: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(file["app"], json!({"name": "Todos", "icon": null, "color": null, "description": ""}));
        assert_eq!(file["records"], json!([{"kind": "todo", "data": {"t": 1}}, {"kind": "todo", "data": {"t": 2}}]));
        let without: serde_json::Value = serde_json::from_str(&export_text(&row, &records, false).unwrap()).unwrap();
        assert!(without.get("records").is_none());
        let bare: AppRow = serde_json::from_value(json!({"id": "b", "slug": "b", "name": "B"})).unwrap();
        assert_eq!(export_text(&bare, &records, true), None);
        assert_eq!(app_export_file_name(&row.slug), "todos.app.json");
    }
}
