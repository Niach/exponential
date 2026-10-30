//! EXP-1459: `sealed-locked` (Special components): THE locked state for
//! sealed session content this install cannot open (yet)
//! (`fixtures/sealed-content.json` `placeholder`), the twin of web
//! `sealed-locked.tsx`: the transcript's divider pill with the lock concept
//! and the reason's copy (`noKey`, `unreadable`), no time. The desktop UI
//! slice of EXP-1459 wires it into the thread screen and the chats list.

use gpui::{div, px, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use crate::icons::registry;
use crate::surface::{glass_pill, PillMode, PillSize};

pub(crate) const ID: &str = "sealed-locked";
pub(crate) const OWNER: &str = "EXP-1459";

const SEALED_JSON: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/sealed-content.json");

/// The placeholder's icon concept and its two labels (noKey, unreadable).
fn placeholder() -> (String, [String; 2]) {
    let fixture: serde_json::Value = serde_json::from_str(SEALED_JSON).expect("sealed-content.json parses");
    let placeholder = &fixture["placeholder"];
    let text = |key: &str| placeholder["copy"][key].as_str().expect("placeholder copy").to_string();
    (
        placeholder["icon"].as_str().expect("placeholder icon").to_string(),
        [text("noKey"), text("unreadable")],
    )
}

fn locked_pill(id: &str, icon: &str, label: &str, cx: &App) -> gpui::AnyElement {
    let muted = cx.theme().muted_foreground;
    let glyph = registry::concept_by_name(icon).unwrap_or(registry::UI_INFO);
    h_flex()
        .w_full()
        .justify_center()
        .child(
            glass_pill(SharedString::from(format!("sealed-locked-{id}")), PillSize::Sm, PillMode::Readonly, cx)
                .child(Icon::new(glyph).with_size(px(PillSize::Sm.glyph())).text_color(muted))
                .child(div().text_xs().text_color(muted).child(SharedString::from(label.to_string()))),
        )
        .into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let (icon, [no_key, unreadable]) = placeholder();
    v_flex()
        .w(px(520.))
        .gap_3()
        .child(locked_pill("no-key", &icon, &no_key, cx))
        .child(locked_pill("unreadable", &icon, &unreadable, cx))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_fixture_names_the_icon_and_both_labels() {
        let (icon, labels) = super::placeholder();
        assert!(super::registry::concept_by_name(&icon).is_some(), "{icon} is a concept");
        assert!(labels.iter().all(|label| !label.is_empty()));
    }
}
