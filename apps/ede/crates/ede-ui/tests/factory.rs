//! The factory documents (apps/ede/ui) against the look they get at run time.

use eui_core::theme::{resolve_token, Mode};
use serde_json::Value;

fn factory_documents() -> Vec<(String, Value)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui");
    let mut docs: Vec<(String, Value)> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json") && p.file_name().is_some_and(|n| n != "theme.json"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()))
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
            // No root background: the children paint their own (toasts).
            assert_eq!(name, "notifications.json", "{name}: a layer document's root paints a background");
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
        let stem = name.trim_end_matches(".json");
        if let Err(issues) = ede_ui::validate_document(stem, &doc) {
            panic!("{name}: {}", issues.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("; "));
        }
        checked += 1;
    }
    assert!(checked >= 20, "every factory document was checked ({checked})");
}
