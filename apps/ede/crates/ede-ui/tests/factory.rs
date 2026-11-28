//! The factory documents (apps/ede/ui, Express since #EDE-216) against the
//! look they get at run time.

use eui_core::theme::{resolve_token, Mode};
use serde_json::Value;

fn factory_documents() -> Vec<(String, Value)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui");
    let mut docs: Vec<(String, Value)> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "express"))
        .map(|p| {
            let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
            let source = std::fs::read_to_string(&p).unwrap();
            let doc = ede_ui::compile_document(&stem, &source).unwrap_or_else(|issues| panic!("{stem}: {issues:?}"));
            (p.file_name().unwrap().to_string_lossy().into_owned(), doc)
        })
        .collect();
    docs.sort_by(|a, b| a.0.cmp(&b.0));
    docs
}

/// #EDE-60: a layer surface floats over windows, so its root must be opaque
/// in the dark theme (`$color.card` is a white glaze at 6% there: windows
/// showed through Settings). Popups sit over their own anchor; the sealed
/// documents are checked by their crate.
#[test]
fn layer_documents_have_an_opaque_root() {
    let theme = eui_core::themes::builtin_theme("exponential").expect("the exponential theme");
    let mut checked = 0;
    for (name, doc) in factory_documents() {
        if doc["surface"].get("layer").is_none() {
            continue;
        }
        let Some(bg) = doc["root"]["style"].get("backgroundColor") else {
            // No root background: the children paint their own (toasts),
            // or nothing of it shows (#EDE-253: the window menu's one-pixel
            // anchor; the menu is its popup).
            assert!(matches!(name.as_str(), "notifications.express" | "window-menu.express"), "{name}: a layer document's root paints a background");
            continue;
        };
        let resolved = resolve_token(&theme, bg, Mode::Dark).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| bg.to_string());
        let hex = resolved.trim_start_matches('#');
        let opaque = hex.len() == 6 || (hex.len() == 8 && hex.ends_with("ff"));
        assert!(opaque, "{name}: the root background {bg} resolves to {resolved} in the dark theme, which is not opaque");
        checked += 1;
    }
    assert!(checked >= 3, "the panel, Settings, the OSD and the switcher were checked ({checked})");
}

/// Every factory document applies as shipped: the header, Tier C (every
/// `@call` in `surface.functions`), the body. The A2UI 1.0 move (#VAPP-289)
/// renamed the calls to `ede_*` and left the headers at `ede.*`, so eDE
/// 0.2.5 refused its own start menu and taskbar; this keeps the two in step.
#[test]
fn factory_documents_validate() {
    let mut checked = 0;
    for (name, doc) in factory_documents() {
        let stem = name.trim_end_matches(".express");
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui").join(&name)).unwrap();
        assert!(ede_ui::validate_document(stem, &doc).is_ok());
        if let Err(issues) = ede_ui::validate_express(stem, &source) {
            panic!("{name}: {}", issues.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("; "));
        }
        checked += 1;
    }
    assert!(checked >= 20, "every factory document was checked ({checked})");
}

/// #EDE-216: documents are Express only; no JSON document ships.
#[test]
fn no_json_documents_ship() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui");
    let json: Vec<String> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".json") && n != "theme.json").collect();
    assert!(json.is_empty(), "{json:?}");
}

/// #EDE-270: the Voice tray's Mute is a Toggle again: a press target with
/// only `on.change` (eui-core's `pressable`, what eui-gpui's click keys
/// on), pressed while /voice/muted, and a press calls `ede_voice {action:
/// mute}` (the holder flips the call's mute and feeds /voice/muted back).
#[test]
fn the_voice_tray_mute_toggle_presses_and_calls_the_holder() {
    let (_, doc) = factory_documents().into_iter().find(|(n, _)| n == "voice-tray.express").expect("voice-tray.express");
    let document = ede_ui::validate_document("voice-tray", &doc).expect("valid");
    let mut s = document.surface(eui_core::themes::builtin_theme("exponential").expect("the exponential theme"));
    s.set_data("/voice/call", Some(serde_json::json!(true))).unwrap();
    let mut m = eui_core::measure::FixedMeasure { sizes: Default::default(), wrap: true };
    s.layout(&mut m);
    let i = s.index_of("mute").expect("the Mute toggle");
    let node = s.layout_node(i).expect("laid out");
    assert_eq!((node.component.as_str(), node.pressable), ("Toggle", true));
    let calls: Vec<(String, Value)> = s.event(i, "press", None).into_iter().filter_map(|e| match e {
        eui_core::surface::OutEvent::FunctionCall { name, args, .. } => Some((name, args)),
        _ => None,
    }).collect();
    assert_eq!(calls, vec![("ede_voice".to_string(), serde_json::json!({"action": "mute"}))]);
}
