//! The IDE's skills multi picker (`picker::skill_picker` behind a
//! `glass_picker_row`, searchable, `ui-docs` + name + description, the
//! first two of three picked) vs genUI Select `multiple` + `searchable`
//! `triggerVariant: row`: a run of adjacent picked rows is ONE block.
//! Shoot `settings_select_multi_ide*`, then `settings_select_multi_eui*`
//! with `EXP_SETTINGS_BEFORE` at the first: 0 px.
use super::*;
use serde_json::json;
use std::rc::Rc;

fn skills() -> Vec<crate::launch_options::SkillOption> {
    let skill = |id: &str, name: &str, description: &str| crate::launch_options::SkillOption { id: id.into(), name: name.into(), description: description.into(), enabled_by_default: false };
    vec![skill("s1", "deploy", "Ship a build to staging"), skill("s2", "review", "Review the open pull request"), skill("s3", "triage", "Sort the inbox by priority")]
}

const PICKED: [&str; 2] = ["s1", "s2"];
const SHOWN: &str = "deploy, review";

pub(super) struct Ide(bool);

impl Render for Ide {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let skills = skills();
        let trigger = crate::trigger_editor::picker_trigger("bench-skills".into(), SHOWN, cx).into_any_element();
        let open = self.0;
        let control = crate::picker::deferred(move |window, cx| {
            let picker = crate::picker::skill_picker::skill_picker(&skills, PICKED.iter().map(|s| s.to_string()).collect(), trigger, Rc::new(|_: Vec<String>, _: &mut Window, _: &mut gpui::App| {}))
                .id(gpui::SharedString::from("bench-skill-picker"));
            let picker = if open { picker.open(Rc::new(|_: &mut Window, _: &mut gpui::App| {})) } else { picker };
            picker.render(window, cx)
        })
        .into_any_element();
        crate::surface::glass_group_rows(vec![crate::surface::glass_picker_row("Skills", None, control, cx)])
    }
}

pub(super) struct Eui {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
    open: Option<u8>,
}

impl Eui {
    fn new(open: bool, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let ink = crate::settings::doc_ink(cx);
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({}), |s: &serde_json::Value, _| s.clone()));
        let options: Vec<serde_json::Value> = skills().iter().map(|s| json!({"label": s.name, "value": s.id, "icon": "ui-docs", "description": s.description})).collect();
        let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "borderRadius": ink.0, "backgroundColor": ink.1, "overflow": "hidden"}, "children": [
            {"id": "skills", "component": "Select", "props": {"label": "Skills", "name": "skills", "triggerVariant": "row", "multiple": true, "searchable": true, "value": PICKED.join(","), "emptyText": crate::picker::skill_picker::NO_SKILLS_FOUND, "options": options}},
        ]});
        let view = crate::settings::doc_mount_lifted("bench-select-multi", tree, &binding, window, cx);
        Self { view, open: open.then_some(2) }
    }
}

impl Render for Eui {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        match self.open {
            Some(0) => {
                self.open = None;
                self.view.update(cx, |view, cx| {
                    if let Some(trigger) = view.index_of("skills.trigger") {
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
            None if self.view.read(cx).layers().iter().any(|layer| layer.owner == "skills") => {
                self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
            }
            None => {}
        }
        div().w_full().child(self.view.clone())
    }
}

#[test]
#[ignore]
fn settings_select_multi_ide() {
    shoot_pane("select-multi", |_, _| Ide(false), |_, _| {});
}

#[test]
#[ignore]
fn settings_select_multi_ide_open() {
    shoot_pane_settled("select-multi-open", |_, _| Ide(true), |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}

#[test]
#[ignore]
fn settings_select_multi_eui() {
    shoot_pane("select-multi", |window, cx| Eui::new(false, window, cx), |_, _| {});
}

#[test]
#[ignore]
fn settings_select_multi_eui_open() {
    shoot_pane_settled("select-multi-open", |window, cx| Eui::new(true, window, cx), |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}
