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
//! This module is the CONTRACT *and* (EXP-1045) its implementation: the
//! types, the two surfaces, and the typed pickers beside it that every IDE
//! call site now goes through. The EXP-288 pickers in `pickers.rs` (status /
//! priority / assignee / labels) moved onto it here; the due date is not one
//! of the ten and keeps its calendar popover. `pickers::searchable_picker`,
//! the HOST-state predecessor, is GONE with its last caller — what is left
//! in `pickers.rs` is the shared vocabulary the primitive itself rides
//! (`picker_row`, `selection_glyph`, the cursor arithmetic, the chip
//! trigger, `StatusPick` + `status_menu` for the row CONTEXT submenus a
//! trigger-based picker cannot express).
//! EXP-1030 finished the sweep: the composer's `#` and ▶ tools, the launch
//! pins, device settings, the automation editor's runner and the workflow
//! runner row all mount their surface here now.
#![allow(dead_code)]

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, Div, ElementId, Entity, Focusable as _,
    Hsla, InteractiveElement, Interactivity, IntoElement, ParentElement as _, RenderOnce,
    SharedString, Stateful, StyleRefinement, Styled as _, Window,
};
use gpui_component::{
    h_flex,
    input::InputState,
    menu::{DropdownMenu as _, PopupMenuItem},
    popover::Popover,
    v_flex, ActiveTheme as _, Icon, Selectable, Side,
};
use theme::tokens as t;

use crate::pickers::{
    clamp_picker_selection, picker_row, selection_glyph, step_picker_selection, SelectionState,
    PICKER_MENU_MIN_WIDTH, PICKER_SEARCH_WIDTH,
};

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
    /// Multi mode only: what THIS row reads as when membership in `value` is
    /// not the whole story — a bulk edit over rows that DISAGREE marks a
    /// label on all of them [`PickerChecked::All`], on some of them
    /// [`PickerChecked::Some`]. Set, it wins over membership; absent, the
    /// state is derived from membership. (web `checked?: boolean |
    /// "indeterminate"`, Android `PickerChecked`.)
    pub checked: Option<PickerChecked>,
}

/// A multi row's mark when membership alone cannot say it (see
/// [`PickerItem::checked`]). All three paint with the HIGHLIGHT alone —
/// wash + inset stroke = all, bare wash = some, nothing = none — never with
/// a trailing check/minus column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickerChecked {
    None,
    Some,
    All,
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
            checked: None,
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

    /// Say this row's mark outright (a bulk edit; see [`Self::checked`]).
    pub(crate) fn checked(mut self, checked: PickerChecked) -> Self {
        self.checked = Some(checked);
        self
    }

    /// The mark this row wears: its own when it has one, else membership.
    pub(crate) fn state<V: PartialEq>(&self, value: &[V]) -> PickerChecked
    where
        T: PartialEq<V>,
    {
        self.checked.unwrap_or_else(|| {
            if value.iter().any(|picked| self.value == *picked) {
                PickerChecked::All
            } else {
                PickerChecked::None
            }
        })
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

/// Replaces the row BODY (the web primitive's `renderItem`). The selection
/// language stays the PRIMITIVE's, so a custom row can never invent a second
/// "this is picked" idiom — the member rows' avatars are the one caller, an
/// avatar being a photo and a picker glyph always an icon.
pub(crate) type PickerRenderItem<T> = Rc<dyn Fn(&PickerItem<T>, &mut App) -> AnyElement>;

/// Close the surface from inside a [`PickerPanel`] — a grid picks without
/// a row, so it needs the dismiss the rows get for free.
pub(crate) type PickerDismiss = Rc<dyn Fn(&mut Window, &mut App)>;

/// REPLACES the filter field and the rows with an inline body (the icon
/// picker's swatch grid). The surface and the trigger stay the primitive's,
/// so a grid still dismisses, anchors and keys like every other picker.
pub(crate) type PickerPanel = Rc<dyn Fn(PickerDismiss, &mut Window, &mut App) -> AnyElement>;

/// REPLACES [`filter_items`] — the rows a query leaves, by index, in the
/// order they are to be drawn. The primitive's own filter is a substring
/// match; a caller with a real RANKING (the composer's issue picker runs
/// `domain::issue_search`, the ONE engine ×4) hands its order in here rather
/// than sorting a list the primitive would then re-filter. `&mut App`,
/// because a ranking that deep is memoised on the host (EXP-868).
pub(crate) type PickerRank<T> = Rc<dyn Fn(&[PickerItem<T>], &str, &mut App) -> Vec<usize>>;

/// Muted NOTE rows under the list, for what the rows themselves cannot say:
/// how many matches the cap hid, that a query matched nothing. Returning
/// `Some` for an EMPTY list also replaces [`Picker::empty_text`] — the caller
/// that has something more precise to say gets to say it.
pub(crate) type PickerFooter = Rc<dyn Fn(&str, &mut App) -> Option<AnyElement>>;

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
    /// The element identity the surface and its across-frame state hang off.
    /// `None` = the `render` call site's source location, which is one
    /// picker per line — a picker built in a LOOP has to name itself
    /// ([`Picker::id`]) or every row would share one query field.
    pub id: Option<ElementId>,
    /// A custom row body ([`PickerRenderItem`]).
    pub render_item: Option<PickerRenderItem<T>>,
    /// An inline body instead of the rows ([`PickerPanel`]).
    pub panel: Option<PickerPanel>,
    /// A wider (or narrower) searchable surface than the default. The
    /// composer's pickers are the callers: their rows carry a whole issue
    /// line, where a property chip's carry a word.
    pub width: Option<gpui::Pixels>,
    /// EXP-946 — the side this picker opens on and how tall it may be,
    /// measured by the caller from where its trigger actually painted
    /// ([`popover_fit`]). `None` = the surface's own placement, which only
    /// ever opens DOWN: a picker low in the window (the composer's) has to
    /// flip, everything anchored high never does.
    pub fit: Option<(gpui::Anchor, gpui::Pixels)>,
    /// A ranking that replaces the substring filter ([`PickerRank`]).
    pub rank: Option<PickerRank<T>>,
    /// Note rows under the list ([`PickerFooter`]).
    pub footer: Option<PickerFooter>,
}

/// Which surface a picker mounts — the IDE's answer to the web primitive's
/// `data-picker-*` markers, and what the contract test asserts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickerSurface {
    /// A short single pick: the `PopupMenu` every other IDE dropdown wears.
    Menu,
    /// A filter field over scroll-capped rows. `search` asks for it, and a
    /// MULTI pick always needs it: a menu dismisses on its first click, and
    /// a multi surface has to survive every toggle.
    Search,
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
            id: None,
            render_item: None,
            panel: None,
            width: None,
            fit: None,
            rank: None,
            footer: None,
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
            id: None,
            render_item: None,
            panel: None,
            width: None,
            fit: None,
            rank: None,
            footer: None,
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

    /// Draw the row bodies yourself ([`PickerRenderItem`]).
    pub(crate) fn render_item(
        mut self,
        render_item: impl Fn(&PickerItem<T>, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_item = Some(Rc::new(render_item));
        self
    }

    /// Put an inline body in the surface instead of the rows
    /// ([`PickerPanel`]). A panel always takes the popover: a grid cannot
    /// live inside a `PopupMenu`.
    pub(crate) fn panel(
        mut self,
        panel: impl Fn(PickerDismiss, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.panel = Some(Rc::new(panel));
        self
    }

    /// Widen (or narrow) the searchable surface. A caller only reaches for
    /// this when its ROWS are a different shape from a property pick's — the
    /// composer's issue rows are a whole issue line.
    pub(crate) fn width(mut self, width: gpui::Pixels) -> Self {
        self.width = Some(width);
        self
    }

    /// EXP-946 — open on the side [`popover_fit`] measured, capped to the
    /// room that side has. The composer sits at the BOTTOM of the window, so
    /// its pickers open upward; nothing else has to care.
    pub(crate) fn fit(mut self, fit: (gpui::Anchor, gpui::Pixels)) -> Self {
        self.fit = Some(fit);
        self
    }

    /// Rank the rows yourself ([`PickerRank`]) instead of taking the
    /// primitive's substring filter.
    pub(crate) fn rank(
        mut self,
        rank: impl Fn(&[PickerItem<T>], &str, &mut App) -> Vec<usize> + 'static,
    ) -> Self {
        self.rank = Some(Rc::new(rank));
        self
    }

    /// Say what the rows cannot ([`PickerFooter`]).
    pub(crate) fn footer(
        mut self,
        footer: impl Fn(&str, &mut App) -> Option<AnyElement> + 'static,
    ) -> Self {
        self.footer = Some(Rc::new(footer));
        self
    }

    /// Name this picker. Only needed when one source line builds SEVERAL
    /// pickers (a picker per list row) — the default identity is the
    /// `render` call site, and two pickers sharing one identity would share
    /// one query field and one keyboard cursor.
    pub(crate) fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Which surface this picker mounts. Presentation belongs to the
    /// primitive: a caller asks for `search` or for `multi`, never for a
    /// popover.
    pub(crate) fn surface(&self) -> PickerSurface {
        if self.search || self.mode == PickerMode::Multi || self.panel.is_some() {
            PickerSurface::Search
        } else {
            PickerSurface::Menu
        }
    }

    /// Mount the trigger and the surface behind it.
    ///
    /// `#[track_caller]`, because the default element identity is this call
    /// site — the same trick `Window::use_state` plays, and for the same
    /// reason: a picker needs state that outlives the frame (its query, its
    /// keyboard cursor) and nothing else about a `Picker` is stable enough
    /// to key it on.
    #[track_caller]
    pub(crate) fn render(self, window: &mut Window, cx: &mut App) -> AnyElement {
        // A disabled picker is its trigger, inert: no surface is mounted at
        // all, so no stray popover can outlive the reason it was disabled.
        if self.disabled {
            return self.trigger;
        }
        let id = self
            .id
            .clone()
            .unwrap_or_else(|| ElementId::CodeLocation(*std::panic::Location::caller()));
        match self.surface() {
            PickerSurface::Menu => self.render_menu(id),
            PickerSurface::Search => self.render_search(id, window, cx),
        }
    }

    /// The short single pick: upstream's `PopupMenu`, so a picker keyboard-
    /// and dismiss-behaves like every other IDE dropdown. The rows are
    /// `element` items, not plain ones — a picker row is the SAME body on
    /// both surfaces (glyph/dot, label, muted description), and a plain
    /// menu item can only carry a glyph and a string.
    fn render_menu(self, id: ElementId) -> AnyElement {
        let Picker {
            items,
            value,
            on_change,
            trigger,
            empty_text,
            render_item,
            ..
        } = self;
        let current = value.into_iter().next();
        let empty_text = empty_text.unwrap_or_else(|| SharedString::from("No options"));
        PickerTrigger::new(id, trigger)
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu
                    .min_w(px(PICKER_MENU_MIN_WIDTH))
                    .max_h(px(PICKER_MENU_MAX_HEIGHT))
                    .scrollable(true)
                    .check_side(Side::Right);
                if items.is_empty() {
                    return menu.item(PopupMenuItem::label(empty_text.clone()));
                }
                for item in &items {
                    // A menu row is a single pick, so its mark is
                    // membership — unless the row said otherwise.
                    let checked = match item.checked {
                        Some(checked) => checked != PickerChecked::None,
                        None => current.as_ref() == Some(&item.value),
                    };
                    let value = item.value.clone();
                    let on_change = on_change.clone();
                    let body = item.clone();
                    let render_item = render_item.clone();
                    menu = menu.item(
                        PopupMenuItem::element(move |_, cx| draw_item(&render_item, &body, cx))
                            .checked(checked)
                            .disabled(item.disabled)
                            .on_click(move |_, window, cx| {
                                on_change(vec![value.clone()], window, cx);
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// The searchable popover: the filter field, a hairline, the rows. The
    /// query and the keyboard cursor are the PRIMITIVE's, kept across frames
    /// by `Window::use_keyed_state` — where `pickers::searchable_picker`
    /// (EXP-963) made every host hold two entities of its own, a picker here
    /// owns them, which is what lets ten typed pickers share one surface.
    fn render_search(self, id: ElementId, window: &mut Window, cx: &mut App) -> AnyElement {
        let Picker {
            items,
            mode,
            value,
            on_change,
            search,
            trigger,
            empty_text,
            render_item,
            panel,
            width,
            fit,
            rank,
            footer,
            ..
        } = self;
        let multi = mode == PickerMode::Multi;
        let empty_text = empty_text.unwrap_or_else(|| SharedString::from("No matches"));
        let key = |suffix: &str| -> ElementId {
            ElementId::Name(SharedString::from(format!("{id:?}-picker-{suffix}")))
        };

        let query = window.use_keyed_state(key("query"), cx, |window, cx| {
            InputState::new(window, cx).placeholder("Search…")
        });
        let state = window.use_keyed_state(key("state"), cx, |_, _| PickerSurfaceState::default());

        let rows_id = key("rows");
        let query_for_open = query.clone();
        let state_for_open = state.clone();
        let mut popover = Popover::new(key("surface")).p_1();
        popover = match width {
            Some(width) => popover.w(width),
            None if search => popover.w(px(PICKER_SEARCH_WIDTH)),
            None => popover.min_w(px(PICKER_MENU_MIN_WIDTH)),
        };
        // EXP-946: the side the caller measured (the composer's pickers flip
        // above their trigger); everything else keeps the surface's default.
        if let Some((anchor, _)) = fit {
            popover = popover.anchor(anchor);
        }
        popover
            .trigger(PickerTrigger::new(key("trigger"), trigger))
            .on_open_change(move |open, window, cx| {
                // Every open is a fresh pick: an empty filter, the top row
                // under the cursor, the field focused (the web
                // `CommandInput` recipe).
                query_for_open.update(cx, |input, cx| input.set_value("", window, cx));
                state_for_open.update(cx, |state, _| state.reset());
                if *open && search {
                    query_for_open.read(cx).focus_handle(cx).focus(window, cx);
                }
            })
            .content(move |_, window, cx| {
                use gpui::{InteractiveElement as _, StatefulInteractiveElement as _};
                let popover_state = cx.entity();
                // The view this popover paints inside: a keyboard or hover
                // move repaints it, as the base popover repaints itself.
                let parent_view_id = window.current_view();
                let filter = query.read(cx).value().trim().to_lowercase();
                state.update(cx, |state, _| state.sync_query(&filter));

                let mut column = v_flex().w_full();
                // EXP-946: the whole surface shrinks with the room its side
                // has, so a picker can never run off the window.
                if let Some((_, max_height)) = fit {
                    column = column.max_h(max_height);
                }
                // A panel REPLACES the field and the rows: the icon grid is
                // its own body, the surface around it still the primitive's.
                if let Some(panel) = &panel {
                    let dismiss_state = popover_state.clone();
                    let dismiss: PickerDismiss = Rc::new(move |window, cx: &mut App| {
                        dismiss_state.update(cx, |state, cx| state.dismiss(window, cx));
                    });
                    return column.child(panel(dismiss, window, cx));
                }
                if search {
                    column = column
                        .child(
                            crate::controls::search_field(
                                &query,
                                crate::controls::SearchFieldSize::Sm,
                                window,
                                cx,
                            )
                            .appearance(false),
                        )
                        .child(
                            div()
                                .h(px(1.))
                                .w_full()
                                .my_1()
                                .bg(cx.theme().border.opacity(0.5)),
                        );
                }

                let visible = match &rank {
                    Some(rank) => rank(&items, &filter, cx),
                    None => filter_items(&items, &filter),
                };
                if visible.is_empty() {
                    // ONE copy for "nothing here" and "nothing matched" —
                    // the web primitive's `emptyText` is the same string on
                    // both, so the IDE never invents a second one. A caller
                    // whose footer has something MORE precise to say about
                    // an empty list says it instead.
                    let note = footer.as_ref().and_then(|footer| footer(&filter, cx));
                    return match note {
                        Some(note) => column.child(note),
                        None => column.child(picker_empty_row(empty_text.clone(), cx)),
                    };
                }

                // The keyboard model: which rows can be picked, where the
                // cursor sits (clamped to what is on screen), what Enter
                // picks.
                let enabled: Vec<bool> = visible.iter().map(|ix| !items[*ix].disabled).collect();
                let cursor_at = clamp_picker_selection(state.read(cx).cursor(), &enabled);
                let target = cursor_at.map(|position| items[visible[position]].value.clone());
                let list_active = cx.theme().list_active;

                column = column
                    .capture_action(cursor_move_listener::<gpui_component::input::MoveUp>(
                        &state,
                        -1,
                        enabled.clone(),
                        parent_view_id,
                    ))
                    .capture_action(cursor_move_listener::<gpui_component::input::MoveDown>(
                        &state,
                        1,
                        enabled,
                        parent_view_id,
                    ))
                    .capture_action({
                        let on_change = on_change.clone();
                        let popover_state = popover_state.clone();
                        let value = value.clone();
                        move |_: &gpui_component::input::Enter, window, cx: &mut App| {
                            let Some(target) = target.clone() else {
                                return;
                            };
                            pick(&on_change, multi, &value, target, window, cx);
                            if !multi {
                                popover_state.update(cx, |state, cx| state.dismiss(window, cx));
                            }
                            cx.notify(parent_view_id);
                        }
                    })
                    .capture_action({
                        let popover_state = popover_state.clone();
                        move |_: &gpui_component::input::Escape, window, cx: &mut App| {
                            popover_state.update(cx, |state, cx| state.dismiss(window, cx));
                            cx.notify(parent_view_id);
                            cx.stop_propagation();
                        }
                    });

                let mut rows = v_flex()
                    .id(rows_id.clone())
                    .w_full()
                    // A measured fit caps the whole COLUMN, so the rows take
                    // whatever is left of it rather than a second ceiling.
                    .when(fit.is_some(), |rows| rows.flex_1().min_h_0())
                    .when(fit.is_none(), |rows| rows.max_h(px(PICKER_ROWS_MAX_HEIGHT)))
                    .overflow_y_scroll();
                for (position, ix) in visible.into_iter().enumerate() {
                    let item = &items[ix];
                    let mark = item.state(&value);
                    let selected = mark != PickerChecked::None;
                    let disabled = item.disabled;
                    let item_value = item.value.clone();
                    let on_change = on_change.clone();
                    let popover_state = popover_state.clone();
                    let state = state.clone();
                    let value = value.clone();
                    let mut row = picker_row(
                        ElementId::Name(SharedString::from(format!("{rows_id:?}-{position}"))),
                        cx,
                    )
                    // A border on EVERY row, transparent unless the row is
                    // picked: the mark must not move the rows it marks.
                    .border_1()
                    .border_color(gpui::transparent_black())
                    // THE selection language (EXP-1021): a multi pick reads
                    // as the row's own highlight — fill plus the active
                    // stroke, never a circle. The stroke is also the third
                    // state: on ALL of a bulk edit's rows it is there, on
                    // only SOME the wash stands alone. The cursor rides the
                    // SAME fill, so both live in `row_fill` (below) — one
                    // `bg` for the two of them, never one overpainting the
                    // other.
                    .when_some(
                        row_fill(multi, mark, cursor_at == Some(position), list_active),
                        |row, fill| row.bg(fill),
                    )
                    .when(multi && mark == PickerChecked::All, |row| {
                        row.border_color(t::glass::STROKE_ACTIVE.to_hsla())
                    })
                    .when(disabled, |row| row.text_color(cx.theme().muted_foreground))
                    .on_hover(move |hovered, _window, cx| {
                        // ONE highlight: hovering MOVES the cursor onto the
                        // row under the pointer rather than painting a
                        // second tint (EXP-892).
                        if !*hovered || disabled {
                            return;
                        }
                        let moved = state.update(cx, |state, _| state.move_to(position));
                        if moved {
                            cx.notify(parent_view_id);
                        }
                    })
                    .child(draw_item(&render_item, item, cx));
                    if !multi {
                        row = row.children(selection_glyph(
                            false,
                            if selected {
                                SelectionState::Selected
                            } else {
                                SelectionState::Unselected
                            },
                            cx,
                        ));
                    }
                    row = if disabled {
                        row.cursor_default()
                    } else {
                        row.on_click(move |_, window, cx| {
                            pick(&on_change, multi, &value, item_value.clone(), window, cx);
                            // Single closes on the pick; multi stays open,
                            // because a set is several picks.
                            if !multi {
                                popover_state.update(cx, |state, cx| state.dismiss(window, cx));
                            }
                        })
                    };
                    rows = rows.child(row);
                }
                if let Some(note) = footer.as_ref().and_then(|footer| footer(&filter, cx)) {
                    rows = rows.child(note);
                }
                column.child(rows)
            })
            .into_any_element()
    }
}

/// A row's body: the caller's override ([`PickerRenderItem`]) when it has
/// one, else the primitive's own.
fn draw_item<T: Clone>(
    render_item: &Option<PickerRenderItem<T>>,
    item: &PickerItem<T>,
    cx: &mut App,
) -> AnyElement {
    match render_item {
        Some(render) => render(item, cx),
        None => picker_item_body(item, cx),
    }
}

/// The one background a search-surface row paints: the keyboard cursor, the
/// multi SELECTION wash, or BOTH stacked.
///
/// EXP-1045 review: the two used to be two `bg` calls, the cursor's last. It
/// won — and `list_active` IS `glass::FILL_ACTIVE`, the very token the wash
/// paints, so a [`PickerChecked::Some`] row (whose whole mark is that bare
/// wash) read exactly like any hovered row under the pointer, i.e. unpicked.
/// `All` rows only survived because of their stroke. Stacking the two says
/// both at once: a picked row under the cursor sits visibly deeper than a
/// merely hovered one, and it can never lose its mark.
fn row_fill(multi: bool, mark: PickerChecked, at_cursor: bool, cursor: Hsla) -> Option<Hsla> {
    let wash = (multi && mark != PickerChecked::None).then(|| t::glass::FILL_ACTIVE.to_hsla());
    match (at_cursor.then_some(cursor), wash) {
        (Some(cursor), Some(wash)) => Some(over(cursor, wash)),
        (Some(fill), None) | (None, Some(fill)) => Some(fill),
        (None, None) => None,
    }
}

/// `top` composited over `bottom`, source-over, ALPHAS included — gpui's own
/// `Hsla::blend` keeps the base alpha, which is exactly what two translucent
/// glass washes must not do (they would stay one wash deep).
fn over(bottom: Hsla, top: Hsla) -> Hsla {
    let alpha = top.a + bottom.a * (1. - top.a);
    if alpha <= 0. {
        return top;
    }
    let (bottom_rgb, top_rgb) = (gpui::Rgba::from(bottom), gpui::Rgba::from(top));
    let mix = |below: f32, above: f32| (above * top.a + below * bottom.a * (1. - top.a)) / alpha;
    Hsla::from(gpui::Rgba {
        r: mix(bottom_rgb.r, top_rgb.r),
        g: mix(bottom_rgb.g, top_rgb.g),
        b: mix(bottom_rgb.b, top_rgb.b),
        a: alpha,
    })
}

/// How tall a picker menu / row list grows before it scrolls.
const PICKER_MENU_MAX_HEIGHT: f32 = 320.;
const PICKER_ROWS_MAX_HEIGHT: f32 = 240.;

/// Report a pick: single hands over the one value, multi the WHOLE new set.
fn pick<T: Clone + PartialEq + 'static>(
    on_change: &OnPickerChange<T>,
    multi: bool,
    current: &[T],
    value: T,
    window: &mut Window,
    cx: &mut App,
) {
    let next = if multi {
        toggled_selection(current, &value)
    } else {
        vec![value]
    };
    on_change(next, window, cx);
}

/// The whole new set a multi toggle reports — the web primitive's `onChange`
/// gets the set, never the delta, so every consumer writes the same way.
pub(crate) fn toggled_selection<T: Clone + PartialEq>(current: &[T], value: &T) -> Vec<T> {
    if current.iter().any(|picked| picked == value) {
        current.iter().filter(|picked| *picked != value).cloned().collect()
    } else {
        let mut next = current.to_vec();
        next.push(value.clone());
        next
    }
}

/// The rows a query leaves, by index and in the caller's order — the pure
/// half of the searchable surface.
pub(crate) fn filter_items<T: Clone>(items: &[PickerItem<T>], query: &str) -> Vec<usize> {
    let needle = query.trim().to_lowercase();
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            needle.is_empty()
                || item_keywords(item)
                    .iter()
                    .any(|keyword| keyword.to_lowercase().contains(&needle))
        })
        .map(|(ix, _)| ix)
        .collect()
}

/// THE row body, shared by both surfaces (the web primitive's
/// `PickerItemBody`): the registry glyph in its colour — or, with a colour
/// and NO glyph, the row's coloured dot, which is what makes a label row a
/// dot and a board row a tinted glyph without either caller choosing a
/// shape — then the label, then the muted second line. A picker row is never
/// a card, on any surface.
pub(crate) fn picker_item_body<T: Clone>(item: &PickerItem<T>, cx: &App) -> AnyElement {
    let mut row = h_flex().flex_1().min_w_0().items_center().gap_2();
    if let Some(icon) = item.icon.clone() {
        let mut icon = icon.size(px(14.)).flex_shrink_0();
        if let Some(color) = item.color {
            icon = icon.text_color(color);
        }
        row = row.child(icon);
    } else if let Some(color) = item.color {
        row = row.child(div().size_2p5().rounded_full().flex_shrink_0().bg(color));
    }
    let mut text = v_flex().flex_1().min_w_0();
    text = text.child(
        div()
            .w_full()
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(item.label.clone()),
    );
    if let Some(description) = &item.description {
        text = text.child(
            div()
                .w_full()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(description.clone()),
        );
    }
    row.child(text).into_any_element()
}

/// The `CommandEmpty` fallback: `pickers::empty_picker_row` with an OWNED
/// string (a picker's `empty_text` is data, not a literal).
fn picker_empty_row(message: SharedString, cx: &App) -> impl IntoElement {
    div()
        .py_4()
        .w_full()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .text_center()
        .child(message)
}

/// The searchable surface's across-frame state: the keyboard cursor and the
/// query it belongs to. A POSITION in the filtered list — the top row on
/// every open and every query change, ↑/↓ move it, hovering moves it too.
#[derive(Default)]
struct PickerSurfaceState {
    cursor: usize,
    query: String,
}

impl PickerSurfaceState {
    fn cursor(&self) -> usize {
        self.cursor
    }

    /// `true` when the cursor actually moved (only then is a repaint owed).
    fn move_to(&mut self, position: usize) -> bool {
        if self.cursor == position {
            return false;
        }
        self.cursor = position;
        true
    }

    fn reset(&mut self) {
        self.cursor = 0;
        self.query.clear();
    }

    /// A changed query puts the top row back under the cursor.
    fn sync_query(&mut self, query: &str) {
        if self.query != query {
            self.query = query.to_string();
            self.cursor = 0;
        }
    }
}

/// One ↑/↓ handler on the query field's own action: move the cursor and
/// repaint the view the popover paints inside.
fn cursor_move_listener<A: gpui::Action>(
    state: &Entity<PickerSurfaceState>,
    delta: isize,
    enabled: Vec<bool>,
    parent_view_id: gpui::EntityId,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    let state = state.clone();
    move |_, _window, cx| {
        let current = state.read(cx).cursor();
        if let Some(next) = step_picker_selection(current, delta, &enabled) {
            if state.update(cx, |state, _| state.move_to(next)) {
                cx.notify(parent_view_id);
            }
        }
    }
}

/// A picker mounted from a helper that has no `&mut Window` — most of the
/// IDE's property controls are `fn(&self, &Issue, &mut Context<Self>)`, and
/// growing a `window` parameter on each of them (and on every caller above
/// them) buys nothing: a `RenderOnce` element is handed one at paint time,
/// so the picker still goes through [`Picker::render`] and every signature
/// stays. The closure runs ONCE, on the frame this element paints.
#[derive(IntoElement)]
pub(crate) struct DeferredPicker {
    build: Option<Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>>,
}

impl RenderOnce for DeferredPicker {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        match self.build.take() {
            Some(build) => build(window, cx),
            None => gpui::Empty.into_any_element(),
        }
    }
}

/// Mount a picker from a window-less helper (see [`DeferredPicker`]).
pub(crate) fn deferred(
    build: impl FnOnce(&mut Window, &mut App) -> AnyElement + 'static,
) -> DeferredPicker {
    DeferredPicker {
        build: Some(Box::new(build)),
    }
}

/// The caller's trigger, wrapped so the surfaces can take it: upstream's
/// `DropdownMenu` wants a `Styled + Selectable + InteractiveElement +
/// IntoElement` and `Popover::trigger` a `Selectable + IntoElement`, and a
/// bare `AnyElement` is none of those. The wrapper adds NO paint of its own
/// — the caller owns how its chip looks, open or shut.
#[derive(IntoElement)]
struct PickerTrigger {
    base: Stateful<Div>,
    element: Option<AnyElement>,
    selected: bool,
}

impl PickerTrigger {
    fn new(id: ElementId, element: AnyElement) -> Self {
        Self {
            base: div().id(id),
            element: Some(element),
            selected: false,
        }
    }
}

impl Selectable for PickerTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl gpui::Styled for PickerTrigger {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for PickerTrigger {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl gpui_component::menu::DropdownMenu for PickerTrigger {}

impl RenderOnce for PickerTrigger {
    fn render(mut self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let element = self.element.take();
        self.base.children(element)
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

    /// The web presentation table's `search filters rows by label and
    /// keywords`: case-insensitive, on the explicit keywords when there are
    /// any, and an empty query keeps every row in the caller's order.
    #[test]
    fn search_filters_rows_by_label_and_keywords() {
        let items = vec![
            PickerItem::new("a", "Alpha").keywords(vec!["APP-1".into(), "Alpha".into()]),
            PickerItem::new("b", "Beta").keywords(vec!["APP-2".into(), "Beta".into()]),
            PickerItem::new("c", "Gamma"),
        ];
        assert_eq!(filter_items(&items, ""), vec![0, 1, 2]);
        assert_eq!(filter_items(&items, "Bet"), vec![1]);
        assert_eq!(filter_items(&items, "app-1"), vec![0]);
        assert_eq!(filter_items(&items, "gam"), vec![2], "falls back to the label");
        assert!(filter_items(&items, "zzz").is_empty());
    }

    /// The web presentation table's `multi mode … reports the whole set`:
    /// a toggle hands over the NEW set, never the delta, and a picked row
    /// toggles back off.
    #[test]
    fn a_multi_toggle_reports_the_whole_new_set() {
        let picked = vec!["a".to_string()];
        assert_eq!(
            toggled_selection(&picked, &"b".to_string()),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(toggled_selection(&picked, &"a".to_string()).is_empty());
        assert_eq!(
            toggled_selection::<String>(&[], &"a".to_string()),
            vec!["a".to_string()]
        );
    }

    /// The third state (EXP-1021 review round): an explicit `checked` WINS
    /// over membership in `value`; absent, the mark is derived from it.
    /// A bulk edit over rows that disagree is what `Some` is for.
    #[test]
    fn an_explicit_mark_wins_over_membership() {
        let picked = vec!["a".to_string()];
        let derived = PickerItem::new("a".to_string(), "Alpha");
        assert_eq!(derived.state(&picked), PickerChecked::All);
        let absent = PickerItem::new("b".to_string(), "Beta");
        assert_eq!(absent.state(&picked), PickerChecked::None);

        // A label on SOME of a bulk edit's issues: not in `value`, and still
        // marked — the bare wash, where `All` adds the inset stroke.
        let partial = PickerItem::new("b".to_string(), "Beta").checked(PickerChecked::Some);
        assert_eq!(partial.state(&picked), PickerChecked::Some);
        // …and the other way: in `value`, said to be off.
        let cleared = PickerItem::new("a".to_string(), "Alpha").checked(PickerChecked::None);
        assert_eq!(cleared.state(&picked), PickerChecked::None);
    }

    /// EXP-1045 review: a partly-picked row must keep its mark under the
    /// pointer. The cursor tint and the selection wash are the SAME token,
    /// so "cursor last" erased the only thing a `Some` row wears.
    #[test]
    fn a_partly_picked_row_keeps_its_mark_under_the_cursor() {
        // What the theme hands the surface (`list_active`), by construction.
        let cursor = t::glass::FILL_ACTIVE.to_hsla();
        let at_rest = row_fill(true, PickerChecked::Some, false, cursor);
        let hovered_unpicked = row_fill(true, PickerChecked::None, true, cursor);
        let hovered_picked = row_fill(true, PickerChecked::Some, true, cursor);
        assert_eq!(at_rest, Some(cursor), "a picked row at rest is the wash");
        assert_eq!(hovered_unpicked, Some(cursor), "the cursor alone is the wash");
        assert_ne!(
            hovered_picked, hovered_unpicked,
            "a partly-picked row under the cursor still says it is picked"
        );
        assert!(
            hovered_picked.expect("a fill").a > hovered_unpicked.expect("a fill").a,
            "the two stack, so the picked row reads deeper"
        );
        // A single pick marks by the trailing glyph, never by the wash —
        // there the fill is the cursor's alone.
        assert_eq!(row_fill(false, PickerChecked::All, true, cursor), Some(cursor));
        assert_eq!(row_fill(false, PickerChecked::All, false, cursor), None);
        assert_eq!(row_fill(true, PickerChecked::None, false, cursor), None);
    }

    /// Presentation belongs to the primitive: a caller asks for `search`,
    /// for `multi` or for a `panel` — never for a popover.
    #[test]
    fn the_surface_follows_the_search_flag_the_mode_and_the_panel() {
        let items = vec![PickerItem::new("a".to_string(), "Alpha")];
        let noop: OnPickerChange<String> = Rc::new(|_, _, _| {});
        let picker = |search: bool, mode: PickerMode| {
            let trigger = gpui::Empty.into_any_element();
            match mode {
                PickerMode::Single => {
                    Picker::single(items.clone(), None, trigger, noop.clone()).search(search)
                }
                PickerMode::Multi => {
                    Picker::multi(items.clone(), vec![], trigger, noop.clone()).search(search)
                }
            }
        };
        assert_eq!(
            picker(false, PickerMode::Single).surface(),
            PickerSurface::Menu,
            "a short single pick is the menu"
        );
        assert_eq!(picker(true, PickerMode::Single).surface(), PickerSurface::Search);
        assert_eq!(
            picker(false, PickerMode::Multi).surface(),
            PickerSurface::Search,
            "a multi pick always needs a surface that survives a click"
        );
        assert_eq!(
            picker(false, PickerMode::Single)
                .panel(|_, _, _| gpui::Empty.into_any_element())
                .surface(),
            PickerSurface::Search,
            "a grid cannot live in a PopupMenu"
        );
    }

    /// EXP-1029's acceptance case, EXP-1021's implementation: each of the
    /// ten typed constructors returns a `Picker<_>` (asserted by type) and
    /// its element carries the PRIMITIVE's surface — checked by drawing all
    /// ten in one themed window and proving every trigger reached the tree
    /// through `Picker::render`, plus the surface each one mounts.
    #[gpui::test]
    async fn every_typed_picker_renders_through_the_primitive(cx: &mut gpui::TestAppContext) {
        use std::cell::Cell;

        use domain::rows::{Board, Issue, Label, User};
        use domain::statuses::default_resolved_statuses;
        use gpui::{IntoElement as _, ParentElement as _, Styled as _};

        /// One picker's trigger: a fixed box that reports, as it paints,
        /// that the primitive mounted it.
        fn probe(seen: &Rc<Cell<u32>>, slot: u32) -> AnyElement {
            let seen = seen.clone();
            div()
                .relative()
                .w(px(80.))
                .h(px(24.))
                .child(
                    gpui::canvas(
                        move |bounds, _, _| {
                            if bounds.size.width > px(0.) {
                                seen.set(seen.get() | (1 << slot));
                            }
                        },
                        |_, _: (), _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element()
        }

        fn issue_row(id: &str, identifier: &str) -> Issue {
            serde_json::from_value(serde_json::json!({
                "id": id,
                "board_id": "board-1",
                "number": 1,
                "identifier": identifier,
                "title": "A title",
                "status": "backlog",
            }))
            .expect("issue row")
        }

        struct Pickers {
            seen: Rc<Cell<u32>>,
            surfaces: Rc<Cell<[Option<PickerSurface>; 10]>>,
        }

        impl gpui::Render for Pickers {
            fn render(
                &mut self,
                window: &mut Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let seen = self.seen.clone();
                let surfaces = self.surfaces.clone();
                let mut mounted: [Option<PickerSurface>; 10] = [None; 10];
                let noop: OnPickerChange<String> = Rc::new(|_, _, _| {});

                let boards = vec![Board::seeded("board-1", "team-1", "Mobile Bugs")];
                let issues = vec![issue_row("issue-1", "EXP-1")];
                let labels: Vec<Label> = vec![serde_json::from_value(serde_json::json!({
                    "id": "label-1",
                    "team_id": "team-1",
                    "name": "bug",
                    "color": "#EF4444",
                }))
                .expect("label row")];
                let members: Vec<User> = vec![serde_json::from_value(serde_json::json!({
                    "id": "user-1",
                    "name": "Ada Lovelace",
                    "email": "ada@example.com",
                }))
                .expect("user row")];
                let statuses = default_resolved_statuses();
                let actions = vec![action_picker::ActionPickerAction {
                    id: "action-1".to_string(),
                    name: "Fix merge conflicts".to_string(),
                    icon: Some("wrench".to_string()),
                    description: None,
                }];
                let devices = vec![device_picker::DevicePickerDevice {
                    id: "device-1".to_string(),
                    name: "studio".to_string(),
                    icon: Some("laptop".to_string()),
                    description: Some("Offline".to_string()),
                    disabled: true,
                }];
                let accounts = vec![coding::AccountOption {
                    id: "system".to_string(),
                    agent: coding::CodingAgent::Claude,
                    email: "ada@example.com".to_string(),
                    is_device_default: true,
                    health: coding::Health::Ok,
                    limits: None,
                }];

                // Each constructor is bound to a `Picker<_>` FIRST — that
                // binding is the type assertion the contract asks for — and
                // only then rendered through the primitive.
                let board: Picker<String> =
                    board_picker::board_picker(&boards, None, probe(&seen, 0), noop.clone());
                mounted[0] = Some(board.surface());
                let issue: Picker<String> =
                    issue_picker::issue_multi_picker(&issues, vec![], probe(&seen, 1), noop.clone());
                mounted[1] = Some(issue.surface());
                let action: Picker<String> =
                    action_picker::action_picker(&actions, None, probe(&seen, 2), noop.clone());
                mounted[2] = Some(action.surface());
                let account: Picker<String> =
                    account_picker::account_picker(&accounts, None, probe(&seen, 3), noop.clone());
                mounted[3] = Some(account.surface());
                let device: Picker<String> =
                    device_picker::device_picker(&devices, None, probe(&seen, 4), noop.clone());
                mounted[4] = Some(device.surface());
                let assignee: Picker<String> = assignee_picker::assignee_picker(
                    &members,
                    PickerMode::Single,
                    vec![],
                    true,
                    probe(&seen, 5),
                    noop.clone(),
                );
                mounted[5] = Some(assignee.surface());
                let icon: Picker<String> = icon_picker::icon_picker(
                    icon_picker::IconSet::Device,
                    None,
                    probe(&seen, 6),
                    noop.clone(),
                );
                mounted[6] = Some(icon.surface());
                let status: Picker<String> = status_picker::status_picker(
                    &statuses,
                    PickerMode::Single,
                    vec![],
                    probe(&seen, 7),
                    noop.clone(),
                );
                mounted[7] = Some(status.surface());
                let priority: Picker<domain::IssuePriority> = priority_picker::priority_picker(
                    PickerMode::Single,
                    vec![],
                    probe(&seen, 8),
                    Rc::new(|_, _, _| {}),
                );
                mounted[8] = Some(priority.surface());
                let label: Picker<String> =
                    label_picker::label_picker(&labels, vec![], probe(&seen, 9), noop.clone());
                mounted[9] = Some(label.surface());

                surfaces.set(mounted);
                v_flex()
                    .w(px(400.))
                    .child(board.render(window, cx))
                    .child(issue.render(window, cx))
                    .child(action.render(window, cx))
                    .child(account.render(window, cx))
                    .child(device.render(window, cx))
                    .child(assignee.render(window, cx))
                    .child(icon.render(window, cx))
                    .child(status.render(window, cx))
                    .child(priority.render(window, cx))
                    .child(label.render(window, cx))
            }
        }

        let seen = Rc::new(Cell::new(0u32));
        let surfaces = Rc::new(Cell::new([None; 10]));
        let (seen_out, surfaces_out) = (seen.clone(), surfaces.clone());
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            // The typed constructors resolve their glyph tints WITHOUT a
            // theme handle (their signatures are the cross-platform
            // contract). That is only sound while the theme-free answer IS
            // the themed one.
            for token in [
                domain::options::ColorToken::MutedForeground,
                domain::options::ColorToken::Foreground,
                domain::options::ColorToken::Yellow,
                domain::options::ColorToken::Green,
                domain::options::ColorToken::Red,
                domain::options::ColorToken::Orange,
                domain::options::ColorToken::Blue,
            ] {
                assert_eq!(
                    crate::icons::static_token_color(token),
                    crate::icons::token_color(token, cx),
                    "{token:?}: the theme-free tint must be the themed one"
                );
            }
        });
        let (_view, cx) = cx.add_window_view(|_, _| Pickers { seen, surfaces });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        assert_eq!(
            seen_out.get(),
            (1 << 10) - 1,
            "all ten typed pickers mounted their trigger through Picker::render"
        );
        let mounted = surfaces_out.get();
        let expected = [
            PickerSurface::Search, // board   — searchable
            PickerSurface::Search, // issue   — searchable, multi
            PickerSurface::Search, // action  — searchable
            PickerSurface::Menu,   // account — a short single pick
            PickerSurface::Menu,   // device  — a short single pick
            PickerSurface::Search, // assignee— searchable
            PickerSurface::Search, // icon    — the swatch grid panel
            PickerSurface::Menu,   // status  — a short single pick
            PickerSurface::Menu,   // priority— a short single pick
            PickerSurface::Search, // label   — searchable, multi
        ];
        for (ix, want) in expected.into_iter().enumerate() {
            assert_eq!(mounted[ix], Some(want), "picker {ix} mounts the {want:?} surface");
        }
    }
}
