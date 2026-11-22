//! #EDE-155: a family the app adds is drawn with the app's faces only, on
//! gpui's Linux text system (the vendored `gpui-pre-wgpu` patch). eDE's
//! shell embeds Inter; Debian's `fonts-inter` installs another build of it
//! and drew the shell's text instead. Here the machine's DejaVu Sans (every
//! runner image has Regular and Bold) stands in: the app adds the Regular
//! face only, so a bold request resolves, and shapes, at the added 400.

#![cfg(any(target_os = "linux", target_os = "freebsd"))]

use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;

use gpui::{px, Font, FontStyle, FontWeight, PlatformTextSystem, SharedString};

const DIR: &str = "/usr/share/fonts/truetype/dejavu";

fn bold() -> Font {
    Font { family: SharedString::from("DejaVu Sans"), features: Default::default(), fallbacks: None, weight: FontWeight(700.0), style: FontStyle::Normal }
}

fn weight(ts: &gpui_wgpu::CosmicTextSystem, id: gpui::FontId) -> f32 {
    ts.font_weight_and_style(id).expect("the face is known").0 .0
}

#[test]
fn an_added_family_shadows_the_systems_faces() {
    if !Path::new(DIR).join("DejaVuSans-Bold.ttf").is_file() {
        eprintln!("skipped: no system DejaVu Sans Bold in {DIR}");
        return;
    }
    let machine = Arc::new(gpui_wgpu::CosmicTextSystem::new("DejaVu Sans"));
    let id = machine.font_id(&bold()).expect("DejaVu Sans resolves");
    assert_eq!(weight(&machine, id), 700.0, "the machine's own Bold answers a bold request");

    let app = Arc::new(gpui_wgpu::CosmicTextSystem::new("DejaVu Sans"));
    let regular = std::fs::read(Path::new(DIR).join("DejaVuSans.ttf")).expect("DejaVu Sans Regular");
    app.add_fonts(vec![Cow::Owned(regular)]).expect("the font loads");
    let id = app.font_id(&bold()).expect("DejaVu Sans resolves");
    assert_eq!(weight(&app, id), 400.0, "only the added Regular is left for its family");
    let text = "Local desktops";
    let line = app.layout_line(text, px(14.0), &[gpui::FontRun { len: text.len(), font_id: id }]);
    assert!(!line.runs.is_empty());
    for run in &line.runs {
        assert_eq!(weight(&app, run.font_id), 400.0, "the shaper takes the added face too, never the system's Bold");
    }
}
