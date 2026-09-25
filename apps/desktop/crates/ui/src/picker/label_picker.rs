//! EXP-1029 contract — the label picker: ALWAYS multi, searchable, each row
//! its colour dot + name; the surface stays open across toggles.
//! `pickers::label_picker_popover` (EXP-288) moves onto this.

use gpui::AnyElement;

use domain::rows::Label;

use crate::settings::parse_hex_color;

use super::{OnPickerChange, Picker, PickerItem};

/// A label row carries a COLOUR and no glyph, which is exactly what makes
/// the primitive draw it as the coloured dot — the caller never picks a
/// shape.
pub(crate) fn label_items(labels: &[Label]) -> Vec<PickerItem<String>> {
    labels
        .iter()
        .map(|label| {
            PickerItem::new(label.id.clone(), label.name.clone()).color(
                label
                    .color
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or_else(|| gpui::opaque_grey(0.5, 1.0)),
            )
        })
        .collect()
}

pub(crate) fn label_picker(
    labels: &[Label],
    value: Vec<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::multi(label_items(labels), value, trigger, on_change)
        .search(true)
        .empty_text("No labels")
}
