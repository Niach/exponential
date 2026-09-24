//! EXP-1029 contract — THE picker primitive on the IDE (EXP-1021 implements
//! it). One primitive per platform, typed pickers on top, the same names
//! everywhere: web `packages/ui/src/picker`, iOS `ExpUI/Sources/Picker`
//! (`GlassPicker`), Android `ui/components/picker`.
//!
//! Presentation belongs to the primitive, never to the caller: on the IDE
//! (a pointer device) every picker is a context menu / popover anchored at
//! the trigger — a `PopupMenu` for a short single pick, the searchable
//! popover (`pickers::PICKER_SEARCH_WIDTH`, filter input + rows) when
//! `search` is on or the mode is multi. Multi-select marks rows by the
//! highlight fill, never a circle. The trigger is whatever chip or button
//! the caller hands in (`pickers::chip_button` for the issue chip row); the
//! primitive owns the surface.
//!
//! This module is the CONTRACT: the types and a [`Picker::render`] stub
//! that renders the trigger only. Every typed picker (one file each beside
//! this one) delegates to it; the ignored test at the bottom is the
//! acceptance case EXP-1021 un-ignores. The EXP-288 pickers in
//! `pickers.rs` (status / priority / assignee / labels / due date) and the
//! account picker in `coding_selects.rs` move onto this primitive there.
#![allow(dead_code)]

use std::rc::Rc;

use gpui::{AnyElement, App, Hsla, SharedString, Window};
use gpui_component::Icon;

pub(crate) mod account_picker;
pub(crate) mod action_picker;
pub(crate) mod assignee_picker;
pub(crate) mod board_picker;
pub(crate) mod device_picker;
pub(crate) mod icon_picker;
pub(crate) mod issue_picker;
pub(crate) mod label_picker;
pub(crate) mod priority_picker;
pub(crate) mod status_picker;

/// One row of a picker.
#[derive(Clone)]
pub(crate) struct PickerItem<T: Clone> {
    /// The stable identity of the row; also what search matches on.
    pub value: T,
    /// What the row reads as; also the default search keyword.
    pub label: SharedString,
    /// A leading glyph — always a registry icon, never a raw glyph name.
    pub icon: Option<Icon>,
    /// A colour for the glyph (a board's hex, a label's dot, a status tone).
    pub color: Option<Hsla>,
    /// A muted second line or trailing note (an email, a branch age).
    pub description: Option<SharedString>,
    /// Rendered, never pickable.
    pub disabled: bool,
    /// Extra search terms (an identifier, an email).
    pub keywords: Vec<SharedString>,
}

impl<T: Clone> PickerItem<T> {
    pub(crate) fn new(value: T, label: impl Into<SharedString>) -> Self {
        Self {
            value,
            label: label.into(),
            icon: None,
            color: None,
            description: None,
            disabled: false,
            keywords: Vec::new(),
        }
    }

    pub(crate) fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub(crate) fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub(crate) fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn keywords(mut self, keywords: Vec<SharedString>) -> Self {
        self.keywords = keywords;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickerMode {
    /// Closes on a pick; `on_change` gets the one value.
    Single,
    /// Toggles without closing; `on_change` gets the whole new set.
    Multi,
}

/// The change callback: the whole new selection (one value in single mode).
pub(crate) type OnPickerChange<T> = Rc<dyn Fn(Vec<T>, &mut Window, &mut App)>;

/// THE picker. Built with the trigger the caller owns; `render` mounts the
/// surface behind it.
pub(crate) struct Picker<T: Clone> {
    pub items: Vec<PickerItem<T>>,
    pub mode: PickerMode,
    /// The current selection (at most one value in single mode).
    pub value: Vec<T>,
    pub on_change: OnPickerChange<T>,
    /// A filter field at the top of the surface.
    pub search: bool,
    /// The chip or button that opens the picker.
    pub trigger: AnyElement,
    /// What an empty list (or an empty search) reads as.
    pub empty_text: Option<SharedString>,
    pub disabled: bool,
}

impl<T: Clone + PartialEq + 'static> Picker<T> {
    pub(crate) fn single(
        items: Vec<PickerItem<T>>,
        value: Option<T>,
        trigger: AnyElement,
        on_change: OnPickerChange<T>,
    ) -> Self {
        Self {
            items,
            mode: PickerMode::Single,
            value: value.into_iter().collect(),
            on_change,
            search: false,
            trigger,
            empty_text: None,
            disabled: false,
        }
    }

    pub(crate) fn multi(
        items: Vec<PickerItem<T>>,
        value: Vec<T>,
        trigger: AnyElement,
        on_change: OnPickerChange<T>,
    ) -> Self {
        Self {
            items,
            mode: PickerMode::Multi,
            value,
            on_change,
            search: false,
            trigger,
            empty_text: None,
            disabled: false,
        }
    }

    pub(crate) fn search(mut self, search: bool) -> Self {
        self.search = search;
        self
    }

    pub(crate) fn empty_text(mut self, text: impl Into<SharedString>) -> Self {
        self.empty_text = Some(text.into());
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Contract stub: the trigger alone. EXP-1021 mounts the surface.
    pub(crate) fn render(self, _window: &mut Window, _cx: &mut App) -> AnyElement {
        self.trigger
    }
}

/// The keywords a row matches on: the explicit ones, else its label.
pub(crate) fn item_keywords<T: Clone>(item: &PickerItem<T>) -> Vec<SharedString> {
    if item.keywords.is_empty() {
        vec![item.label.clone()]
    } else {
        item.keywords.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_matches_on_its_keywords_else_on_its_label() {
        let plain = PickerItem::new("a", "Alpha");
        assert_eq!(item_keywords(&plain), vec![SharedString::from("Alpha")]);
        let keyed = PickerItem::new("a", "Alpha").keywords(vec!["APP-1".into()]);
        assert_eq!(item_keywords(&keyed), vec![SharedString::from("APP-1")]);
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1021 moves every typed picker onto the primitive"]
    fn every_typed_picker_renders_through_the_primitive() {
        // board, issue, action, account, device, assignee, icon(set),
        // status, priority, label: each typed constructor returns a
        // `Picker<_>` (asserted by type), and its rendered element carries
        // the primitive's surface — checked with a gpui test window there.
        unimplemented!("EXP-1021")
    }
}
