//! #EXP-1629: the IDE's device picker (`picker::device_picker` behind a
//! `glass_picker_row`) vs genUI Select `triggerVariant: row` with option
//! `icon` + `description`, closed and open. Shoot `settings_select_ide*`,
//! then `settings_select_eui*` with `EXP_SETTINGS_BEFORE` at the first: 0 px.
use super::*;
use serde_json::json;
use std::rc::Rc;

fn rows(long: bool) -> Vec<crate::picker::device_picker::DevicePickerDevice> {
    let device = |id: &str, name: &str, icon: &str, server: bool, offline: bool| crate::picker::device_picker::DevicePickerDevice {
        id: id.into(),
        name: name.into(),
        icon: Some(icon.into()),
        server,
        description: offline.then(|| "Offline".to_string()),
        disabled: false,
    };
    let server = if long { "build-server-eu-central-hetzner-2026" } else { "build-server" };
    vec![device("d1", "mint", "monitor", false, false), device("d2", server, "server", true, false), device("d3", "macbook", "laptop", false, true)]
}

pub(super) struct Ide(bool, bool);

impl Render for Ide {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let rows = rows(self.1);
        let trigger = crate::trigger_editor::picker_trigger("bench-device".into(), "mint", cx).into_any_element();
        let open = self.0;
        let control = crate::picker::deferred(move |window, cx| {
            let picker = crate::picker::device_picker::device_picker(&rows, Some("d1".to_string()), trigger, Rc::new(|_: Vec<String>, _: &mut Window, _: &mut gpui::App| {}))
                .id(gpui::SharedString::from("bench-device-picker"));
            let picker = if open { picker.open(Rc::new(|_: &mut Window, _: &mut gpui::App| {})) } else { picker };
            picker.render(window, cx)
        })
        .into_any_element();
        crate::surface::glass_group_rows(vec![crate::surface::glass_picker_row("Device", None, control, cx)])
    }
}

pub(super) struct Eui {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
    open: Option<u8>,
}

impl Eui {
    fn new(open: bool, long: bool, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let ink = crate::settings::doc_ink(cx);
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({}), |s: &serde_json::Value, _| s.clone()));
        let options: Vec<serde_json::Value> = rows(long).iter().map(|d| {
            let mut o = json!({"label": d.name, "value": d.id, "icon": crate::icons::device_icon_name(d.icon.as_deref(), d.server)});
            if let Some(description) = &d.description {
                o["description"] = json!(description);
            }
            o
        }).collect();
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "borderRadius": ink.0, "backgroundColor": ink.1, "overflow": "hidden"}, "children": [
            {"id": "device", "component": "Select", "props": {"label": "Device", "name": "device", "triggerVariant": "row", "value": "d1", "popupMinWidth": 170, "options": options}},
        ]});
        let view = crate::settings::doc_mount_lifted("bench-select", tree, &binding, window, cx);
        Self { view, open: open.then_some(2) }
    }
}

impl Render for Eui {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        match self.open {
            Some(0) => {
                self.open = None;
                self.view.update(cx, |view, cx| {
                    if let Some(trigger) = view.index_of("device.trigger") {
                        view.fire(trigger, "press", None, cx);
                    }
                });
            }
            Some(n) => {
                self.open = Some(n - 1);
                cx.notify();
            }
            // The headless clock stands still: the popup's enter fade
            // runs out on the frames after the press.
            None if self.view.read(cx).layers().iter().any(|layer| layer.owner == "device") => {
                self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
            }
            None => {}
        }
        div().w_full().child(self.view.clone())
    }
}

#[test]
#[ignore]
fn settings_select_ide() {
    shoot_pane("select", |_, _| Ide(false, false), |_, _| {});
}

#[test]
#[ignore]
fn settings_select_ide_open() {
    shoot_pane_settled("select-open", |_, _| Ide(true, false), |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}

#[test]
#[ignore]
fn settings_select_eui() {
    shoot_pane("select", |window, cx| Eui::new(false, false, window, cx), |_, _| {});
}

#[test]
#[ignore]
fn settings_select_eui_open() {
    shoot_pane_settled("select-open", |window, cx| Eui::new(true, false, window, cx), |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}

#[test]
#[ignore]
fn settings_select_ide_open_long() {
    shoot_pane_settled("select-open-long", |_, _| Ide(true, true), |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}

#[test]
#[ignore]
fn settings_select_eui_open_long() {
    shoot_pane_settled("select-open-long", |window, cx| Eui::new(true, true, window, cx), |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}
