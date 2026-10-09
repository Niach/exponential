//! Web-parity control metrics (EXP-525) — the shadcn sizing layer.
//!
//! gpui-component sizes its controls in rem off this app's root font size
//! ([`theme::FONT_SIZE_PX`], 14px since EXP-723), which still lands every
//! button/input under the web's shadcn boxes (web Button
//! default h-9/px-4, sm h-8/px-3, xs h-6/px-2, all on the theme radius since
//! EXP-1176 — a text button is a rounded rectangle, the capsule is the pill
//! recipe's; inputs h-9). These helpers are pure `Styled` refinements — gpui-component
//! applies caller refinements after its own base styles (`refine_style` runs
//! last and is replayed inside the selected/disabled state closures), so they
//! win without forking the component. `cursor_pointer` rides the same
//! refinement: gpui-component buttons default to `cursor_default`, the web
//! (and every native toolkit convention we mirror) points on hover.
//!
//! VAPP-90: the BODIES of the generic controls (the text/search fields, the
//! disclosure header, the text button, the sizing traits and `CTL_*` rungs,
//! the empty state, the glass/ghost icon buttons, the switch, the segmented
//! capsule, the textarea, the skeleton, the checkbox, the alert) live in the
//! Exponential UI SDK's `exponential_ui_gpui::controls`; this module
//! re-exports them under their old names, so no call site changed, and keeps
//! the IDE-only controls (jump-to-bottom, the back glyph, the right-click
//! claim, the danger menu item, the typeahead menu). The SDK reads its token
//! values from a host-installed `Chrome`: [`ide_chrome`] builds the IDE's
//! from `theme::tokens` and `ui::init` installs it.

use std::sync::LazyLock;

use exponential_ui_gpui::chrome::{Chrome, ChromeIcons};
use gpui::{
    anchored, deferred, div, point, prelude::FluentBuilder as _, px, Anchor, Animation,
    AnimationExt as _, AnyElement, App, Div, ElementId, InteractiveElement, IntoElement,
    ParentElement as _, Pixels, SharedString, Stateful, StatefulInteractiveElement as _, Styled,
    Window,
};
use gpui_component::{menu::PopupMenuItem, v_flex, ActiveTheme as _, Icon, IconNamed as _};
use theme::tokens as t;

#[allow(unused_imports)] // the shared API; not every control has an IDE caller today
pub(crate) use exponential_ui_gpui::controls::{
    alert, alert_title, checkbox, disclosure_header, empty_state, ghost_icon_button,
    glass_icon_button, glass_input, search_field, segmented, segmented_item, skeleton,
    text_button, web_switch, web_textarea, AlertVariant, CheckState, ChevronSide,
    SearchFieldSize, Skeleton, TextButtonVariant, WebControl, WebText, CHECKBOX_PX, CTL_LG_H,
    CTL_MD_H, CTL_SM_H, SEARCH_FIELD_SM_H, SKELETON_PULSE,
};

/// The IDE's [`Chrome`]: every value straight from the generated design
/// tokens (`theme::tokens::glass` / `radius` / `motion::ease`) and the icon
/// registry's concept glyphs, so the SDK controls paint byte-identically to
/// the bodies they replaced (`ide_chrome_is_the_sdk_default` pins the paint
/// half against `Chrome::default()`). Built once; `ui::init` installs a copy
/// for the `cx`-taking builders and the pure `Div` recipes in
/// `crate::surface` pass this one.
pub(crate) fn ide_chrome() -> &'static Chrome {
    static CHROME: LazyLock<Chrome> = LazyLock::new(|| {
        use crate::icons::registry;
        Chrome {
            fill_section: t::glass::FILL_SECTION.to_hsla(),
            fill_row: t::glass::FILL_ROW.to_hsla(),
            fill_card: t::glass::FILL_CARD.to_hsla(),
            fill_panel: t::glass::FILL_PANEL.to_hsla(),
            fill_active: t::glass::FILL_ACTIVE.to_hsla(),
            stroke_row: t::glass::STROKE_ROW.to_hsla(),
            stroke_section: t::glass::STROKE_SECTION.to_hsla(),
            stroke_card: t::glass::STROKE_CARD.to_hsla(),
            stroke_strong: t::glass::STROKE_STRONG.to_hsla(),
            stroke_active: t::glass::STROKE_ACTIVE.to_hsla(),
            radius_sm: t::radius::SM,
            radius_md: t::radius::MD,
            radius_lg: t::radius::LG,
            radius_xl: t::radius::XL,
            radius_xl2: t::radius::XL2,
            radius_xl3: t::radius::XL3,
            icons: ChromeIcons {
                search: registry::NAV_SEARCH.path(),
                clear: registry::UI_CLEAR.path(),
                chevron_right: registry::UI_CHEVRON_RIGHT.path(),
                chevron_down: registry::UI_CHEVRON_DOWN.path(),
                check: registry::UI_CHECK.path(),
                minus: registry::UI_MINUS.path(),
            },
            ease_standard: t::motion::ease::STANDARD,
        }
    });
    &CHROME
}

/// EXP-1191 — the jump-to-bottom button's box and glyph (px), the spec all
/// four clients share: a 32px circle, a 16px `ui-arrow-down`.
pub(crate) const JUMP_TO_BOTTOM_SIZE: f32 = 32.;
const JUMP_TO_BOTTOM_GLYPH: f32 = 16.;
/// Its gap to the top edge of whatever sits under the scroller (the composer
/// card on the run view).
pub(crate) const JUMP_TO_BOTTOM_GAP: f32 = 12.;
pub(crate) const JUMP_TO_BOTTOM_LABEL: &str = "Jump to bottom";

/// EXP-1191 — the ONE jump-to-bottom button (web `JumpToBottomButton`, iOS
/// `JumpToBottomButton`, Android twin): a 32px round icon-only button on the
/// ELEVATED surface (`theme.popover`, opaque — it floats over the text it
/// covers), a hairline stroke, a barely-there shadow, the `ui-arrow-down`
/// glyph at 70% foreground. Tooltip "Jump to bottom". It fades in with the
/// FAST motion token each time it mounts (gpui drops a keyed animation's
/// state when the element leaves the tree, so a re-show replays the fade).
///
/// Positioning is the CALLER's (absolute, centred over its scroller,
/// [`JUMP_TO_BOTTOM_GAP`] above the bottom chrome), as is `on_click`: scroll
/// to the newest row and re-arm tail follow. A plain element rather than a
/// `Button`: a custom button variant paints its fill at a fifth of the
/// handed-in alpha, and this surface must be opaque.
pub(crate) fn jump_to_bottom_button(
    id: impl Into<ElementId>,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme();
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(JUMP_TO_BOTTOM_SIZE))
        .rounded_full()
        .bg(colors.popover)
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .hover(|style| style.border_color(t::glass::STROKE_STRONG.to_hsla()))
        .shadow(vec![gpui::BoxShadow {
            color: gpui::black().opacity(0.12),
            offset: point(px(0.), px(1.)),
            blur_radius: px(4.),
            spread_radius: px(0.),
            inset: false,
        }])
        .cursor_pointer()
        .text_color(colors.foreground.opacity(0.7))
        .child(
            Icon::from(crate::icons::registry::UI_ARROW_DOWN).size(px(JUMP_TO_BOTTOM_GLYPH)),
        )
        .tooltip(|window, cx| {
            gpui_component::tooltip::Tooltip::new(JUMP_TO_BOTTOM_LABEL).build(window, cx)
        })
        .on_click(on_click)
        .with_animation(
            "jump-to-bottom-fade",
            Animation::new(theme::motion::FAST).with_easing(theme::motion::standard()),
            |button, delta| button.opacity(delta),
        )
        .into_any_element()
}

/// The size of a back glyph on every client (EXP-862): 16px, the `icon-sm`
/// rung's glyph, bare in a back ROW (EXP-863 retired the session header's
/// Back button; the sidebar carries navigation).
const BACK_GLYPH: f32 = 16.;

/// The BARE back glyph (EXP-862), for the back ROWS where the whole row is
/// the target (the settings nav's and the list nav's "‹ Boards"): a nested
/// button inside a clickable row is a second hit target for the same action,
/// so those rows take the glyph alone and keep their own click.
pub(crate) fn back_glyph() -> Icon {
    // EXP-870: the `ui-back` CONCEPT (an arrow), the web back rows' glyph.
    Icon::from(crate::icons::registry::UI_BACK)
        .size(px(BACK_GLYPH))
        .flex_shrink_0()
}

/// EXP-1228: an element that opens its OWN right-click menu claims the
/// press, so a surrounding surface's menu (the run transcript's Copy) does
/// not open on top of it. `.context_menu(…)` registers its handler after
/// this one and so still fires first; an ancestor's div listener fires last
/// and never sees the press.
pub(crate) fn claim_right_click<E: InteractiveElement>(element: E) -> E {
    element.on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
}

/// A DESTRUCTIVE popup-menu item (EXP-697): label AND glyph in the theme's
/// danger red, matching the iOS/Android glass menus where delete/remove always
/// reads red. `PopupMenuItem` has no danger variant upstream, so the label
/// rides `PopupMenuItem::element` — the only escape hatch that lets a menu row
/// paint its own text color.
pub(crate) fn danger_menu_item(
    label: impl Into<SharedString>,
    icon: Icon,
    cx: &App,
) -> PopupMenuItem {
    let danger = cx.theme().danger;
    let label = label.into();
    PopupMenuItem::element(move |_, _| {
        pointer_row_fill(div().text_color(danger).child(label.clone()), false)
    })
    .icon(icon.text_color(danger))
}

// ---------------------------------------------------------------------------
// THE IDE menu (EXP-1249): a gpui-component `PopupMenu` whose rows POINT
// ---------------------------------------------------------------------------
//
// The pinned gpui-component (rev da4f936, a git dependency, not vendored)
// sets no cursor on any `PopupMenu` row, so every IDE menu showed the arrow
// over its items — the web's MENU_ROW_BASE had the same bug (`cursor-default`
// beating the base rule). Two halves fix it without forking the crate:
//
// 1. Rows built here are `PopupMenuItem::element` rows whose body fills the
//    item, padding included ([`pointer_row_fill`]), under `cursor_pointer`
//    (a disabled row keeps the arrow). Element rows are the only escape
//    hatch; a plain `PopupMenuItem::new` row cannot carry a cursor.
// 2. A `Submenu` row has no element form, so [`PointerMenu`] — the
//    `dropdown_menu` twin every IDE-owned menu hangs off — lays a pointer
//    layer OVER its menu. The layer's hitbox is a plain one (it occludes
//    nothing: hover, click, scroll and the submenu's hover-open still reach
//    the rows), and it only sets the cursor. A submenu paints deferred, above
//    the layer, so its own rows answer for themselves (half 1).
//
// Call sites still on `gpui_component::menu::DropdownMenu` / `.context_menu`
// move here one by one; the row helpers work inside either.

/// The pointer menu's row height: the IDE control rung (32px) — the 26px
/// crate row with the tokens' 8px side padding (`menu::pointer`).
pub(crate) const MENU_ROW_H: f32 = t::size::CONTROL_MD;
/// The side padding `PopupMenu` puts around an element row's body (its
/// `INNER_PADDING`): the fill reaches over it so the WHOLE row points.
const MENU_ROW_INSET: f32 = 8.;

/// Wrap an element row's body so it fills its item (over the crate's 8px
/// side padding) and points — the arrow stays on a `disabled` row.
pub(crate) fn pointer_row_fill(body: impl IntoElement, disabled: bool) -> Div {
    pointer_fill(body, disabled).min_h(px(MENU_ROW_H))
}

/// [`pointer_row_fill`] at the crate's own row height.
pub(crate) fn pointer_fill(body: impl IntoElement, disabled: bool) -> Div {
    use gpui::prelude::FluentBuilder as _;
    gpui_component::h_flex()
        .flex_1()
        .min_w_0()
        .mx(px(-MENU_ROW_INSET))
        .px(px(MENU_ROW_INSET))
        .items_center()
        .when(!disabled, |row| row.cursor_pointer())
        .when(disabled, |row| row.cursor_default())
        .child(body)
}

/// The drop-in for `PopupMenuItem::new(label)` that POINTS (half 1 of the
/// section note) in a menu that mixes in `submenu` rows or crate-slot icons:
/// the crate keeps its icon slot, its check and its own row height, so the
/// rows of one menu stay one height. Chain `.icon` / `.checked` /
/// `.on_click` exactly as on a plain row.
pub(crate) fn pointer_label_item(label: impl Into<SharedString>, disabled: bool) -> PopupMenuItem {
    let label = label.into();
    PopupMenuItem::element(move |_, cx| {
        let color = if disabled {
            cx.theme().muted_foreground
        } else {
            cx.theme().popover_foreground
        };
        pointer_fill(div().text_color(color).child(label.clone()), disabled)
    })
    .disabled(disabled)
}

/// One pointer-menu row: an optional leading glyph, the label, and an
/// optional trailing part (a value, a switch, a chevron) — the web `Menu`'s
/// item anatomy. The glyph rides INSIDE the body (never `.icon()`), so no
/// empty icon slot shifts the rows of a menu built only from these.
pub(crate) fn menu_row(
    icon: Option<crate::icons::ExpIcon>,
    label: impl Into<SharedString>,
    trailing: Option<AnyElement>,
    disabled: bool,
    cx: &App,
) -> Div {
    use gpui::prelude::FluentBuilder as _;
    let muted = cx.theme().muted_foreground;
    pointer_row_fill(
        gpui_component::h_flex()
            .flex_1()
            .min_w_0()
            .gap(px(t::menu::pointer::ITEM_GAP))
            .items_center()
            .when_some(icon, |row, icon| {
                row.child(
                    Icon::new(icon)
                        .size(px(t::menu::pointer::ICON_SIZE))
                        .flex_shrink_0()
                        .text_color(muted),
                )
            })
            .child(div().flex_1().min_w_0().truncate().child(label.into()))
            .children(trailing),
        disabled,
    )
    .when(disabled, |row| row.text_color(muted))
}

/// The muted trailing value of a row ("High", "2"), with the `›` of a row
/// that opens more choices.
pub(crate) fn menu_row_value(value: Option<SharedString>, chevron: bool, cx: &App) -> AnyElement {
    use gpui::prelude::FluentBuilder as _;
    let muted = cx.theme().muted_foreground;
    gpui_component::h_flex()
        .flex_shrink_0()
        .gap_1()
        .items_center()
        .text_xs()
        .text_color(muted)
        .when_some(value, |row, value| row.child(value))
        .when(chevron, |row| {
            row.child(
                Icon::new(crate::icons::registry::UI_CHEVRON_RIGHT)
                    .size(px(14.))
                    .text_color(muted),
            )
        })
        .into_any_element()
}

/// A pointer row that does something: `on_select` runs, then the menu closes
/// (the crate dismisses after every element row's click).
pub(crate) fn pointer_menu_item(
    icon: Option<crate::icons::ExpIcon>,
    label: impl Into<SharedString>,
    trailing: impl Fn(&App) -> Option<AnyElement> + 'static,
    on_select: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    let label = label.into();
    PopupMenuItem::element(move |_, cx| menu_row(icon.clone(), label.clone(), trailing(cx), false, cx))
        .on_click(move |_, window, cx| on_select(window, cx))
}

/// A pointer row with a switch on the right (Ultracode, Computer use): a
/// click anywhere on the row flips it.
pub(crate) fn pointer_toggle_item(
    id: impl Into<SharedString>,
    icon: Option<crate::icons::ExpIcon>,
    label: impl Into<SharedString>,
    on: bool,
    on_toggle: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    let id: SharedString = id.into();
    pointer_menu_item(
        icon,
        label,
        move |_| Some(web_switch(id.clone()).checked(on).into_any_element()),
        move |window, cx| on_toggle(!on, window, cx),
    )
}

/// A pointer row of a single-pick submenu: the label, and the check on the
/// picked one (right side, the picker rule).
pub(crate) fn pointer_check_item(
    label: impl Into<SharedString>,
    checked: bool,
    on_select: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    pointer_menu_item(
        None,
        label,
        move |cx| {
            checked.then(|| {
                Icon::new(crate::icons::registry::UI_CHECK)
                    .size(px(14.))
                    .text_color(cx.theme().foreground)
                    .into_any_element()
            })
        },
        on_select,
    )
}

/// THE IDE dropdown menu (EXP-1249): `trigger` opens a `PopupMenu` built by
/// `builder` (rebuilt on every open, like upstream's `dropdown_menu`), with
/// the pointer layer over it (see the section note). Same open/dismiss
/// behaviour as `gpui_component::menu::DropdownMenu`: the menu takes focus,
/// a pick or Escape or a click outside closes it.
#[derive(IntoElement)]
pub(crate) struct PointerMenu<T: gpui_component::Selectable + IntoElement + 'static> {
    id: ElementId,
    trigger: T,
    anchor: Anchor,
    builder: std::rc::Rc<
        dyn Fn(
            gpui_component::menu::PopupMenu,
            &mut Window,
            &mut gpui::Context<gpui_component::menu::PopupMenu>,
        ) -> gpui_component::menu::PopupMenu,
    >,
}

impl<T: gpui_component::Selectable + IntoElement + 'static> PointerMenu<T> {
    pub(crate) fn new(
        id: impl Into<ElementId>,
        trigger: T,
        builder: impl Fn(
                gpui_component::menu::PopupMenu,
                &mut Window,
                &mut gpui::Context<gpui_component::menu::PopupMenu>,
            ) -> gpui_component::menu::PopupMenu
            + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            trigger,
            anchor: Anchor::TopLeft,
            builder: std::rc::Rc::new(builder),
        }
    }

    /// Which corner of the trigger the menu hangs off (upstream's anchor:
    /// `BottomLeft` opens the menu ABOVE the trigger).
    pub(crate) fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }
}

impl PointerMenu<gpui_component::button::Button> {
    /// The menu hung off a `Button` trigger, keyed by the button's own id —
    /// the drop-in for upstream's `button.dropdown_menu(builder)`.
    pub(crate) fn for_button(
        mut trigger: gpui_component::button::Button,
        builder: impl Fn(
                gpui_component::menu::PopupMenu,
                &mut Window,
                &mut gpui::Context<gpui_component::menu::PopupMenu>,
            ) -> gpui_component::menu::PopupMenu
            + 'static,
    ) -> Self {
        let id = trigger
            .interactivity()
            .element_id
            .clone()
            .unwrap_or_else(|| ElementId::Name("pointer-menu".into()));
        Self::new(id, trigger, builder)
    }
}

/// The open menu entity, kept across frames and dropped on dismiss so the
/// next open rebuilds it from current state.
#[derive(Default)]
struct PointerMenuState {
    menu: Option<gpui::Entity<gpui_component::menu::PopupMenu>>,
}

impl<T: gpui_component::Selectable + IntoElement + 'static> gpui::RenderOnce for PointerMenu<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui::Focusable as _;
        let builder = self.builder.clone();
        let state_id = ElementId::Name(SharedString::from(format!("pointer-menu:{:?}", self.id)));
        let menu_state =
            window.use_keyed_state(state_id.clone(), cx, |_, _| PointerMenuState::default());
        gpui_component::popover::Popover::new(ElementId::Name(SharedString::from(format!(
            "pointer-menu-popover:{:?}",
            self.id
        ))))
        .appearance(false)
        .overlay_closable(false)
        .trigger(self.trigger)
        .anchor(self.anchor)
        .content(move |_, window, cx| {
            let menu = match menu_state.read(cx).menu.clone() {
                Some(menu) => menu,
                None => {
                    let builder = builder.clone();
                    let menu = gpui_component::menu::PopupMenu::build(window, cx, move |menu, window, cx| {
                        builder(menu, window, cx)
                    });
                    menu_state.update(cx, |state, _| state.menu = Some(menu.clone()));
                    menu.focus_handle(cx).focus(window, cx);
                    let popover_state = cx.entity();
                    let dismiss_state = menu_state.clone();
                    window
                        .subscribe(&menu, cx, move |_, _: &gpui::DismissEvent, window, cx| {
                            popover_state.update(cx, |state, cx| state.dismiss(window, cx));
                            dismiss_state.update(cx, |state, _| state.menu = None);
                        })
                        .detach();
                    menu
                }
            };
            with_pointer_layer(menu)
        })
    }
}

/// A menu surface under the pointer layer (half 2 of the section note): the
/// layer covers the menu, sets the cursor and nothing else. Shared by
/// [`PointerMenu`] and the styleguide's open specimens.
pub(crate) fn with_pointer_layer(menu: impl IntoElement) -> Div {
    div()
        .relative()
        .child(menu)
        .child(div().absolute().top_0().left_0().size_full().cursor_pointer())
}

// ---------------------------------------------------------------------------
// The typeahead menu (EXP-970)
// ---------------------------------------------------------------------------

/// The menu's width band (web `TypeaheadMenu`: `w-72`, grown to the widest
/// row the desktop lists).
pub(crate) const TYPEAHEAD_MIN_W: f32 = 260.;
pub(crate) const TYPEAHEAD_MAX_W: f32 = 380.;
/// The tallest the menu gets on either side of the caret (web `MAX_HEIGHT`).
pub(crate) const TYPEAHEAD_MAX_H: f32 = 320.;
/// The shortest it is allowed to be capped to (web `MIN_HEIGHT`) — under
/// this the menu keeps its floor and the window snap slides it instead.
const TYPEAHEAD_MIN_H: f32 = 48.;
/// Below this much room under the caret the menu flips above, when the room
/// above is larger (web `FLIP_BELOW`).
const TYPEAHEAD_FLIP_BELOW: f32 = 200.;
/// The gap between the caret line and the menu edge (web `ANCHOR_GAP`).
const TYPEAHEAD_GAP: f32 = 4.;
/// The window margin the menu keeps clear (web `VIEWPORT_PAD`).
const TYPEAHEAD_PAD: f32 = 8.;

/// Which side of the caret a [`typeahead_menu`] opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeaheadPlacement {
    Below,
    Above,
}

/// The measured half of an anchored typeahead: the side and the height cap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TypeaheadPlan {
    pub(crate) placement: TypeaheadPlacement,
    pub(crate) max_h: f32,
}

/// The pure placement rule, the web `placeTypeaheadMenu` twin: below the
/// caret unless the room there is short AND the room above is larger, capped
/// to the room on the chosen side (never taller than [`TYPEAHEAD_MAX_H`],
/// never shorter than the floor). `caret_top`/`caret_bottom` are the caret
/// LINE's edges in window coordinates.
pub(crate) fn plan_typeahead(caret_top: f32, caret_bottom: f32, viewport_h: f32) -> TypeaheadPlan {
    let space_below = viewport_h - caret_bottom - TYPEAHEAD_PAD;
    let space_above = caret_top - TYPEAHEAD_PAD;
    let cap = |space: f32| (space - TYPEAHEAD_GAP).min(TYPEAHEAD_MAX_H).max(TYPEAHEAD_MIN_H);
    if space_below < TYPEAHEAD_FLIP_BELOW && space_above > space_below {
        TypeaheadPlan {
            placement: TypeaheadPlacement::Above,
            max_h: cap(space_above),
        }
    } else {
        TypeaheadPlan {
            placement: TypeaheadPlacement::Below,
            max_h: cap(space_below),
        }
    }
}

/// How a [`typeahead_menu`] sits in its host.
pub(crate) enum TypeaheadArm {
    /// Hangs off the caret: a deferred, window-anchored popup that flips
    /// above the caret when the room below runs out (the `@`/`#`/`:` menus
    /// of the editors and the mention composer).
    Anchored {
        /// The caret line's left edge, top and bottom, in window coordinates.
        caret_left: Pixels,
        caret_top: Pixels,
        caret_bottom: Pixels,
    },
    /// Sits in the flow of its host, full width (the steer composer's `/`
    /// menu, which lives inside the composer card above the textarea).
    Inline,
}

/// EXP-970 — the ONE typeahead menu, the web `TypeaheadMenu` twin: the menu
/// that follows what someone is TYPING (`@` mentions, `#` issue refs, `:`
/// emoji, `/` commands), as opposed to the combobox, which owns its own
/// field. The surface is the floating-menu recipe (`MENU_SURFACE_CLASS`):
/// the opaque popover fill under the card hairline on the row rung
/// (`radius::MD`), a 4px inset, rows 2px apart, capped and scrolling. The
/// rows are [`typeahead_row`]s the host builds (it owns their content, the
/// click that accepts and the hover that moves the selection).
///
/// Four hosts drew this box by hand before, each with its own width, radius,
/// corner and cap; the slash menu was a bare column. Anchoring is the
/// [`TypeaheadArm`]: the anchored arm measures the room itself
/// ([`plan_typeahead`]) and hands gpui the corner to hang from, so a long
/// list near the bottom of the window opens upward instead of being slid
/// over the caret.
///
/// `scroll` is the HOST's handle on the capped list (release review R5): the
/// host calls `scroll_to_item(selected)` from its ↑/↓ handlers so a keyboard
/// move past the cap brings the selected row into view; a hover move never
/// scrolls.
pub(crate) fn typeahead_menu(
    id: impl Into<ElementId>,
    arm: TypeaheadArm,
    rows: Vec<AnyElement>,
    scroll: &gpui::ScrollHandle,
    window: &Window,
    cx: &App,
) -> AnyElement {
    use gpui::StatefulInteractiveElement as _;
    let theme = cx.theme();
    let surface = v_flex()
        .id(id)
        .occlude()
        .track_scroll(scroll)
        .p_1()
        .gap_0p5()
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .rounded(px(t::radius::MD))
        .shadow_md()
        .overflow_y_scroll();
    match arm {
        TypeaheadArm::Inline => surface
            .w_full()
            .min_w_0()
            .max_h(px(TYPEAHEAD_MAX_H))
            .children(rows)
            .into_any_element(),
        TypeaheadArm::Anchored {
            caret_left,
            caret_top,
            caret_bottom,
        } => {
            let plan = plan_typeahead(
                f32::from(caret_top),
                f32::from(caret_bottom),
                f32::from(window.viewport_size().height),
            );
            let menu = surface
                .min_w(px(TYPEAHEAD_MIN_W))
                .max_w(px(TYPEAHEAD_MAX_W))
                .max_h(px(plan.max_h))
                .children(rows);
            let (corner, position) = match plan.placement {
                TypeaheadPlacement::Below => (
                    Anchor::TopLeft,
                    point(caret_left, caret_bottom + px(TYPEAHEAD_GAP)),
                ),
                TypeaheadPlacement::Above => (
                    Anchor::BottomLeft,
                    point(caret_left, caret_top - px(TYPEAHEAD_GAP)),
                ),
            };
            deferred(
                anchored()
                    .anchor(corner)
                    .position(position)
                    .snap_to_window_with_margin(px(TYPEAHEAD_PAD))
                    .child(menu),
            )
            .with_priority(200)
            .into_any_element()
        }
    }
}

/// One row of a [`typeahead_menu`] (web `TYPEAHEAD_ROW_CLASS`): full width,
/// `px_2 py_1`, the small rung's corner, and the list's active fill on the
/// SELECTED row only — hovering MOVES the selection (EXP-892) rather than
/// painting a second highlight, so the host chains `.on_hover` for that and
/// `.on_mouse_down` to accept; the content (`completion_row_content`, the
/// slash row's name/hint/description) is the host's child.
pub(crate) fn typeahead_row(id: impl Into<ElementId>, selected: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .px_2()
        .py_1()
        .rounded(px(t::radius::SM))
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.list_active))
}

#[cfg(test)]
mod tests {
    /// EXP-862 — the structural half of [`super::web_switch`]: a `Switch`
    /// built anywhere else is a switch that does not point on hover, and no
    /// compiler can say so. A grep over the crate's own sources is the
    /// cheapest honest check there is (the `text_selection_guard` rule uses
    /// the same scan).
    ///
    /// VAPP-90: the one allowed constructor now lives in the SDK
    /// (`exponential_ui_gpui::controls::web_switch`, outside this crate's
    /// sources) and this module re-exports it, so the rule reads the same —
    /// construct through `controls::…`. This module is still the one file the
    /// scan skips: its docs spell the needles.
    #[test]
    fn only_controls_constructs_switches() {
        assert_only_controls_constructs("Switch::new(", "EXP-862", "controls::web_switch(id)");
    }

    /// EXP-970: the same rule for gpui-component's `Checkbox` — its box, its
    /// radius and its tick are the crate's, not the ladder's.
    #[test]
    fn only_controls_constructs_checkboxes() {
        assert_only_controls_constructs(
            "Checkbox::new(",
            "EXP-970",
            "controls::checkbox(id, state, disabled, cx)",
        );
    }

    /// EXP-970: and for its `Skeleton` — the crate's pulse and radius.
    #[test]
    fn only_controls_constructs_skeletons() {
        assert_only_controls_constructs(
            "Skeleton::new(",
            "EXP-970",
            "controls::skeleton()",
        );
    }

    /// The scan behind the three constructor rules: `needle` may appear in
    /// no source file of this crate but this module (the re-export of the
    /// SDK's allowed constructor, and the one file that spells the needle in
    /// its docs).
    fn assert_only_controls_constructs(needle: &str, issue: &str, replacement: &str) {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders: Vec<String> = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("the crate's own sources are readable") {
                let path = entry.expect("a readable dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                    continue;
                }
                if path.file_name().and_then(|name| name.to_str()) == Some("controls.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("a readable source file");
                let hits = text.matches(needle).count();
                if hits > 0 {
                    let name = path
                        .strip_prefix(&src)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    offenders.push(format!("{name} ({hits})"));
                }
            }
        }
        offenders.sort();
        assert!(
            offenders.is_empty(),
            "{issue}: construct through `{replacement}` — `{needle}` still appears in: {}",
            offenders.join(", ")
        );
    }

    /// EXP-970 — the placement rule is the web `placeTypeaheadMenu`, number
    /// for number (`typeahead.test.tsx`).
    #[test]
    fn the_typeahead_opens_below_and_flips_above_only_when_below_is_short() {
        use super::{plan_typeahead, TypeaheadPlacement, TYPEAHEAD_MAX_H};
        // Plenty of room below: below, capped at the maximum.
        let plan = plan_typeahead(100., 120., 800.);
        assert_eq!(plan.placement, TypeaheadPlacement::Below);
        assert_eq!(plan.max_h, TYPEAHEAD_MAX_H);
        // Room below shrinks under the flip line while above is larger: above,
        // capped to the room above minus the pad and the gap.
        let plan = plan_typeahead(500., 520., 700.);
        assert_eq!(plan.placement, TypeaheadPlacement::Above);
        assert_eq!(plan.max_h, TYPEAHEAD_MAX_H);
        let plan = plan_typeahead(300., 320., 400.);
        assert_eq!(plan.placement, TypeaheadPlacement::Above);
        assert_eq!(plan.max_h, 300. - 8. - 4.);
        // Short below AND short above: stays below, capped to what is there.
        let plan = plan_typeahead(60., 80., 200.);
        assert_eq!(plan.placement, TypeaheadPlacement::Below);
        assert_eq!(plan.max_h, 200. - 80. - 8. - 4.);
        // The floor: a menu never plans shorter than its minimum height.
        let plan = plan_typeahead(20., 40., 60.);
        assert_eq!(plan.placement, TypeaheadPlacement::Below);
        assert_eq!(plan.max_h, 48.);
    }

    /// VAPP-90: the IDE's chrome paints exactly what the SDK's default
    /// does, so moving the control bodies behind `Chrome` changed no pixel —
    /// and the SDK's hard-coded default cannot drift from the design tokens
    /// without failing here. Only the glyph paths differ (the registry's vs
    /// gpui-component's bundled set).
    #[test]
    fn ide_chrome_is_the_sdk_default() {
        use exponential_ui_gpui::chrome::Chrome;
        let ide = super::ide_chrome();
        let default = Chrome {
            icons: ide.icons.clone(),
            ..Chrome::default()
        };
        assert_eq!(*ide, default);
    }

    /// VAPP-90: the SDK's size ladder is the token ladder.
    #[test]
    fn control_rungs_are_the_token_rungs() {
        use theme::tokens as t;
        assert_eq!(super::CTL_LG_H, t::size::CONTROL_LG);
        assert_eq!(super::CTL_LG_H, t::size::INPUT_HEIGHT);
        assert_eq!(super::CTL_MD_H, t::size::CONTROL_MD);
        assert_eq!(super::CTL_SM_H, t::size::CONTROL_SM);
    }

    #[test]
    fn a_bool_is_a_two_state_check() {
        use super::CheckState;
        assert_eq!(CheckState::from(true), CheckState::Checked);
        assert_eq!(CheckState::from(false), CheckState::Unchecked);
    }

    /// EXP-1228: a right press on a tile with its own menu opens that menu
    /// and never the transcript's around it; unclaimed content falls through
    /// to the transcript's.
    fn right_press_in_feed(claimed: bool) -> (bool, bool) {
        use std::cell::Cell;
        use std::rc::Rc;

        use gpui::{
            div, point, px, Context, InteractiveElement as _, IntoElement, MouseButton,
            ParentElement as _, Render, Styled as _, TestApp, Window,
        };

        struct Feed {
            feed_menu: Rc<Cell<bool>>,
            tile_menu: Rc<Cell<bool>>,
            claimed: bool,
        }

        impl Render for Feed {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let feed_menu = self.feed_menu.clone();
                let tile_menu = self.tile_menu.clone();
                // Stands in for `.context_menu(…)`: a right-press listener
                // registered after the tile's own (it paints first).
                let tile = div().w(px(120.)).h(px(40.)).child(
                    div()
                        .size_full()
                        .on_mouse_down(MouseButton::Right, move |_, _, _| tile_menu.set(true)),
                );
                div()
                    .w(px(400.))
                    .h(px(200.))
                    .on_mouse_down(MouseButton::Right, move |_, _, _| feed_menu.set(true))
                    .child(if self.claimed {
                        super::claim_right_click(tile).into_any_element()
                    } else {
                        tile.into_any_element()
                    })
            }
        }

        let feed_menu = Rc::new(Cell::new(false));
        let tile_menu = Rc::new(Cell::new(false));
        let mut app = TestApp::new();
        let mut window = app.open_window({
            let feed_menu = feed_menu.clone();
            let tile_menu = tile_menu.clone();
            move |_, _| Feed {
                feed_menu,
                tile_menu,
                claimed,
            }
        });
        window.simulate_mouse_down(point(px(20.), px(10.)), MouseButton::Right);
        (feed_menu.get(), tile_menu.get())
    }

    #[test]
    fn a_claimed_tile_keeps_its_menu_and_suppresses_the_feed_menu() {
        let (feed_menu, tile_menu) = right_press_in_feed(true);
        assert!(tile_menu, "the tile's own menu must open");
        assert!(!feed_menu, "the transcript menu must not open over it");
    }

    #[test]
    fn unclaimed_content_falls_through_to_the_feed_menu() {
        let (feed_menu, tile_menu) = right_press_in_feed(false);
        assert!(tile_menu);
        assert!(feed_menu, "the transcript menu opens without the claim");
    }
}
