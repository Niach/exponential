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
    alert, alert_title, checkbox, choice_tiles, disclosure_header, empty_state, empty_state_actions,
    empty_state_list, ghost_icon_button,
    glass_icon_button, glass_input, kbd, kbd_group, search_field, segmented, segmented_item, skeleton,
    text_button, web_switch, web_textarea, AlertVariant, CheckState, ChevronSide, ChoiceTile,
    ChoiceTilesGeometry, OnTilePick, TilePicture,
    SearchFieldSize, Skeleton, TextButtonVariant, WebControl, WebText, CHECKBOX_PX, CTL_LG_H,
    CTL_MD_H, CTL_SM_H, SEARCH_FIELD_SM_H, SKELETON_PULSE,
};

/// S-3: a push-to-talk binding as THE keycaps (the web `ShortcutKeys`): one
/// [`kbd`] per `shortcutLabel` part in a [`kbd_group`]; empty =
/// `copy.shortcutNone` as muted words.
pub(crate) fn shortcut_keys(value: &str, cx: &App) -> Div {
    let caps = domain::voice::shortcut_keycaps(value);
    if caps.is_empty() {
        return div().text_sm().text_color(cx.theme().muted_foreground).child(domain::voice::shortcut_label(value));
    }
    kbd_group(caps.into_iter().map(|cap| kbd(cap, cx)))
}

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
            fill_card_opaque: exponential_ui_gpui::chrome::composite(
                t::POPOVER.to_hsla(),
                t::glass::FILL_CARD.to_hsla(),
            ),
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
                refresh: registry::UI_REFRESH.path(),
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
        // EXP-1243: an enter — `ENTER` on decelerate.
        .with_animation(
            "jump-to-bottom-fade",
            Animation::new(theme::motion::ENTER).with_easing(theme::motion::decelerate()),
            |button, delta| button.opacity(delta),
        )
        .into_any_element()
}

/// EXP-1316 (1n) — how close to the end a scroller must sit to count as ON
/// its tail (gpui's list re-arms within a pixel too).
const TAIL_SLOP: f32 = 1.;

/// Whether a scroller at `offset_y` (≤ 0, more negative further down) with
/// `max_y` of scroll range sits on its tail. Nothing to scroll = on it.
pub(crate) fn tail_reached(offset_y: f32, max_y: f32) -> bool {
    max_y <= TAIL_SLOP || -offset_y >= max_y - TAIL_SLOP
}

/// EXP-1316 (1n) — the session feed's tail follow for a plain scroller (the
/// thread conversation, the agent channel): the steer viewer's
/// `FollowMode::Tail` rule over a [`gpui::ScrollHandle`]. It opens on the
/// newest row and stays pinned while rows land AND while they grow (rows
/// arrive async from the shapes, cards and pictures measure later: any
/// growth under a pinned reader re-pins); an UPWARD wheel or a drag away
/// from the end releases it (the jump-to-bottom button shows); scrolling
/// back onto the end, or the button, re-arms it. Shared across frames.
///
/// The snap to the end is ISSUED, never standing: once on open
/// ([`TailFollow::take_snap`]), on every [`TailLayout::Repin`] the probe
/// sees, and from the button. A `scroll_to_bottom` on every render while
/// pinned used to undo a scrollbar drag each frame (the drag's `set_offset`
/// notifies the view, the re-render re-snapped at the next prepaint, the
/// probe then read the end again), so [`TailLayout::Released`] was only ever
/// reachable from the wheel.
#[derive(Clone)]
pub(crate) struct TailFollow(std::rc::Rc<std::cell::Cell<TailState>>);

#[derive(Clone, Copy, Debug, PartialEq)]
struct TailState {
    following: bool,
    /// The scroll range at the last layout (growth vs a reader's drag).
    last_max: f32,
    /// The next render still owes the end a snap (opening, a re-pin).
    snap_owed: bool,
}

/// What a layout means for the follow ([`TailFollow::on_layout`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TailLayout {
    /// Nothing changed.
    Keep,
    /// Pinned and the content grew past the view: snap to the end again.
    Repin,
    /// Pinned, but the reader moved off the end without growth (a
    /// scrollbar drag): released.
    Released,
    /// Released, and the reader is back on the end: pinned again.
    Rearmed,
}

impl Default for TailFollow {
    fn default() -> Self {
        Self(std::rc::Rc::new(std::cell::Cell::new(TailState {
            following: true,
            last_max: 0.,
            snap_owed: true,
        })))
    }
}

impl TailFollow {
    pub(crate) fn following(&self) -> bool {
        self.0.get().following
    }

    fn set_following(&self, following: bool) {
        let mut state = self.0.get();
        state.following = following;
        self.0.set(state);
    }

    /// A wheel moved the scroller by `dy` (> 0 = toward the top): an upward
    /// one releases the tail. Returns whether the state changed.
    pub(crate) fn on_wheel(&self, dy: f32) -> bool {
        if dy > 0. && self.following() {
            self.set_following(false);
            return true;
        }
        false
    }

    /// After layout (offset ≤ 0, `max_y` = the scroll range).
    pub(crate) fn on_layout(&self, offset_y: f32, max_y: f32) -> TailLayout {
        let mut state = self.0.get();
        let grew = max_y > state.last_max + TAIL_SLOP;
        state.last_max = max_y;
        let at_end = tail_reached(offset_y, max_y);
        let outcome = match (state.following, at_end) {
            (true, true) | (false, false) => TailLayout::Keep,
            (true, false) if grew => TailLayout::Repin,
            (true, false) => {
                state.following = false;
                TailLayout::Released
            }
            (false, true) if max_y > TAIL_SLOP => {
                state.following = true;
                TailLayout::Rearmed
            }
            (false, true) => TailLayout::Keep,
        };
        self.0.set(state);
        outcome
    }

    /// The jump-to-bottom button: re-pin (the next render snaps).
    pub(crate) fn pin(&self) {
        let mut state = self.0.get();
        state.following = true;
        state.snap_owed = true;
        self.0.set(state);
    }

    /// Whether THIS render snaps the scroller to the end — true once after
    /// construction and after [`TailFollow::pin`], never while a reader
    /// drags (see the type docs). A [`TailLayout::Repin`] snaps from the
    /// probe itself, so it owes nothing here.
    pub(crate) fn take_snap(&self) -> bool {
        let mut state = self.0.get();
        let owed = state.following && state.snap_owed;
        state.snap_owed = false;
        self.0.set(state);
        owed
    }

    /// The scroller around `content` (`id`, tracked by `handle`; a
    /// `track_scroll`ed `overflow_y_scroll` area + the overlay scrollbar, the
    /// [`crate::scroll_pane::v_scroll_pane`] recipe — gpui-component's
    /// `overflow_y_scrollbar` scrolls its OWN handle, never `handle`) with the
    /// follow applied and the jump-to-bottom button ([`jump_to_bottom_button`],
    /// [`JUMP_TO_BOTTOM_GAP`] above the bottom edge) while released.
    /// `entity_id` = the view to repaint when the state flips.
    pub(crate) fn scroller(
        &self,
        id: &'static str,
        handle: &gpui::ScrollHandle,
        content: impl IntoElement,
        entity_id: gpui::EntityId,
        cx: &App,
    ) -> AnyElement {
        let following = self.following();
        // Issued once (open, the button), not on every pinned render: a
        // standing snap undid a scrollbar drag each frame (type docs).
        if self.take_snap() {
            handle.scroll_to_bottom();
        }
        let wheel = self.clone();
        let probe_state = self.clone();
        let probe_handle = handle.clone();
        // Paint-time: the layout settled the offset and the range this frame.
        let probe = gpui::canvas(
            move |_, _, cx| {
                let offset = f32::from(probe_handle.offset().y);
                let max = f32::from(probe_handle.max_offset().y);
                match probe_state.on_layout(offset, max) {
                    TailLayout::Keep => {}
                    TailLayout::Repin => {
                        probe_handle.scroll_to_bottom();
                        cx.defer(move |cx| cx.notify(entity_id));
                    }
                    TailLayout::Released | TailLayout::Rearmed => {
                        cx.defer(move |cx| cx.notify(entity_id));
                    }
                }
            },
            |_, _: (), _, _| {},
        )
        .absolute()
        .size_0();
        let jump = (!following).then(|| {
            let target = handle.clone();
            let state = self.clone();
            gpui_component::h_flex()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(JUMP_TO_BOTTOM_GAP))
                .justify_center()
                .child(jump_to_bottom_button(
                    SharedString::from(format!("{id}-jump-to-bottom")),
                    move |_, _, cx| {
                        state.pin();
                        target.scroll_to_bottom();
                        cx.notify(entity_id);
                    },
                    cx,
                ))
        });
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                div()
                    .id(id)
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(handle)
                    .on_scroll_wheel(move |event, window, cx| {
                        let dy = f32::from(event.delta.pixel_delta(window.line_height()).y);
                        if wheel.on_wheel(dy) {
                            cx.notify(entity_id);
                        }
                    })
                    .child(content),
            )
            .child(
                div().absolute().top_0().left_0().right_0().bottom_0().child(
                    gpui_component::scroll::Scrollbar::new(handle)
                        .axis(gpui_component::scroll::ScrollbarAxis::Vertical),
                ),
            )
            .child(probe)
            .children(jump)
            .into_any_element()
    }
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

/// N-6: a row handler bound to the window its menu was BUILT for. A menu
/// that moved to a menu layer (`menu_window`, dialogs) runs its rows in the
/// layer window; the handler expects the host (it focuses the composer's
/// input, opens the file picker over the dialog). `run_in_host` runs it
/// there; outside a layer this is the handler itself.
pub(crate) fn host_bound(
    f: impl Fn(&mut Window, &mut App) + 'static,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let f = std::rc::Rc::new(f);
    move |window: &mut Window, cx: &mut App| {
        let f = f.clone();
        crate::menu_window::run_in_host(window, cx, move |window, cx| f(window, cx));
    }
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
    let on_select = host_bound(on_select);
    PopupMenuItem::element(move |_, cx| menu_row(icon.clone(), label.clone(), trailing(cx), false, cx))
        .on_click(move |_, window, cx| on_select(window, cx))
}

/// [`pointer_menu_item`]'s DESTRUCTIVE twin (thread 58cb32df: a chat's
/// Stop and delete / Delete): the same row geometry (icon size, gap,
/// inset) as its neighbours, glyph and label in the danger tint.
pub(crate) fn danger_pointer_menu_item(
    icon: crate::icons::ExpIcon,
    label: impl Into<SharedString>,
    on_select: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    let label = label.into();
    let on_select = host_bound(on_select);
    PopupMenuItem::element(move |_, cx| {
        let danger = cx.theme().danger;
        pointer_row_fill(
            gpui_component::h_flex()
                .flex_1()
                .min_w_0()
                .gap(px(t::menu::pointer::ITEM_GAP))
                .items_center()
                .text_color(danger)
                .child(Icon::new(icon.clone()).size(px(t::menu::pointer::ICON_SIZE)).flex_shrink_0().text_color(danger))
                .child(div().flex_1().min_w_0().truncate().child(label.clone())),
            false,
        )
    })
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

/// [`pointer_check_item`] led by its own `icon`, drawn as given (an agent's
/// glyph in its colour, NEW-3's Message an agent › rows): the Picker row's
/// glyph, the label, the trailing check on the picked one.
pub(crate) fn pointer_check_item_with_icon(
    icon: impl Fn() -> Icon + 'static,
    label: impl Into<SharedString>,
    checked: bool,
    on_select: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    let label = label.into();
    let on_select = host_bound(on_select);
    PopupMenuItem::element(move |_, cx| {
        pointer_row_fill(
            gpui_component::h_flex()
                .flex_1()
                .min_w_0()
                .gap(px(t::menu::pointer::ITEM_GAP))
                .items_center()
                .child(icon().size(px(t::menu::pointer::ICON_SIZE)).flex_shrink_0())
                .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                .children(checked.then(|| {
                    Icon::new(crate::icons::registry::UI_CHECK)
                        .size(px(14.))
                        .text_color(cx.theme().foreground)
                })),
            false,
        )
    })
    .on_click(move |_, window, cx| on_select(window, cx))
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
    /// EXP-1259 (DEV-ONLY): open on this render, with the first row
    /// selected (a submenu row = its submenu open), for a capture.
    dev_open: bool,
    /// N-6: the height the menu wants — the menu layer picks its side by it.
    wanted: Pixels,
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
            dev_open: false,
            wanted: px(crate::menu_window::DEFAULT_WANTED_HEIGHT),
        }
    }

    /// N-6: the height the menu wants, room permitting (the composer's "+"
    /// menu says its `PLUS_MENU_WANTED_HEIGHT`). Only the menu layer reads
    /// it: in-window, `anchor` already encodes the side the caller measured.
    pub(crate) fn wanted_height(mut self, wanted: f32) -> Self {
        self.wanted = px(wanted);
        self
    }

    /// EXP-1259 (DEV-ONLY, `EXP_DEV_COMPOSER_MENU`): open the menu on THIS
    /// render, as a click would, and select its first row once it is built,
    /// as a hover would — so a first-row submenu shows open beside it. Pass
    /// `true` for one render only; the popover keeps itself open after.
    pub(crate) fn dev_open(mut self, open: bool) -> Self {
        self.dev_open = open;
        self
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

/// N-14: `button.pointer_menu(builder)` — the drop-in for upstream's
/// `button.dropdown_menu(builder)` that goes through THE IDE menu
/// ([`PointerMenu`]): rows that point, and inside a dialog window the menu
/// window, so the menu overhangs the dialog instead of clipping.
pub(crate) trait PointerMenuButtonExt {
    fn pointer_menu(
        self,
        builder: impl Fn(
                gpui_component::menu::PopupMenu,
                &mut Window,
                &mut gpui::Context<gpui_component::menu::PopupMenu>,
            ) -> gpui_component::menu::PopupMenu
            + 'static,
    ) -> PointerMenu<gpui_component::button::Button>;
}

impl PointerMenuButtonExt for gpui_component::button::Button {
    fn pointer_menu(
        self,
        builder: impl Fn(
                gpui_component::menu::PopupMenu,
                &mut Window,
                &mut gpui::Context<gpui_component::menu::PopupMenu>,
            ) -> gpui_component::menu::PopupMenu
            + 'static,
    ) -> PointerMenu<gpui_component::button::Button> {
        PointerMenu::for_button(self, builder)
    }
}

/// The open menu entity, kept across frames and dropped on dismiss so the
/// next open rebuilds it from current state.
#[derive(Default)]
struct PointerMenuState {
    menu: Option<gpui::Entity<gpui_component::menu::PopupMenu>>,
    /// EXP-1259: a dev open is waiting for the menu to select its first row.
    dev_select: bool,
    /// N-6: where the trigger painted last, in window coordinates — the
    /// menu layer hangs the menu off it.
    trigger_bounds: gpui::Bounds<Pixels>,
    /// N-6: the OS refused a menu layer for this trigger: the in-window
    /// popover opens on the next render and this trigger stays in-window.
    layer_refused: bool,
    force_open: bool,
    /// N-6: a dev open (EXP-1259) is waiting for a measured trigger to open
    /// its menu layer with the first row selected.
    dev_layer: bool,
}

impl<T: gpui_component::Selectable + IntoElement + 'static> PointerMenu<T> {
    /// N-6: the trigger alone, opening the menu in a menu layer over this
    /// window (`menu_window`) on a press — the same press the in-window
    /// popover opens on. A second press, or any press while the layer is
    /// up, closes it (the layer covers the trigger, so in practice the
    /// layer's own outside-click dismiss gets there first).
    fn render_layer_trigger(
        self,
        menu_state: gpui::Entity<PointerMenuState>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        use gpui_component::ElementExt as _;
        let id = SharedString::from(format!("{:?}", self.id));
        let open = crate::menu_window::is_open(&id, cx);
        let builder = self.builder.clone();
        let wanted = self.wanted;
        // EXP-1259's dev open, the layer way: once the trigger has painted,
        // open the layer as a press would, its first row selected.
        if self.dev_open {
            menu_state.update(cx, |state, _| state.dev_layer = true);
        }
        let measured = menu_state.read(cx).trigger_bounds;
        if menu_state.read(cx).dev_layer && !open && measured.size.width > px(0.) {
            menu_state.update(cx, |state, _| state.dev_layer = false);
            let refused_state = menu_state.clone();
            crate::menu_window::open(
                window,
                cx,
                crate::menu_window::MenuAnchor { rect: measured, wanted },
                builder.clone(),
                id.clone(),
                true,
                move |window, cx| {
                    refused_state.update(cx, |state, _| {
                        state.layer_refused = true;
                        state.force_open = true;
                        state.dev_select = true;
                    });
                    window.refresh();
                },
            );
        }
        let slot = menu_state.clone();
        div()
            .id(ElementId::Name(SharedString::from(format!("pointer-menu-layer:{id}"))))
            .on_prepaint(move |bounds, _, cx| {
                slot.update(cx, |state, _| state.trigger_bounds = bounds);
            })
            .child(self.trigger.selected(open))
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                if open {
                    crate::menu_window::close(cx);
                    return;
                }
                let rect = menu_state.read(cx).trigger_bounds;
                let refused_state = menu_state.clone();
                crate::menu_window::open(
                    window,
                    cx,
                    crate::menu_window::MenuAnchor { rect, wanted },
                    builder.clone(),
                    id.clone(),
                    false,
                    move |window, cx| {
                        refused_state.update(cx, |state, _| {
                            state.layer_refused = true;
                            state.force_open = true;
                        });
                        window.refresh();
                    },
                );
            })
            .into_any_element()
    }
}

impl<T: gpui_component::Selectable + IntoElement + 'static> gpui::RenderOnce for PointerMenu<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui::Focusable as _;
        let builder = self.builder.clone();
        let state_id = ElementId::Name(SharedString::from(format!("pointer-menu:{:?}", self.id)));
        let menu_state =
            window.use_keyed_state(state_id.clone(), cx, |_, _| PointerMenuState::default());
        if self.dev_open {
            menu_state.update(cx, |state, _| state.dev_select = true);
        }
        // N-6: in a dialog window the menu opens in a menu layer over the
        // window, so it can overhang the dialog's edges. The in-window
        // popover stays for every other window and once the OS refused a
        // layer.
        let layer_mode =
            !menu_state.read(cx).layer_refused && crate::menu_window::wanted(window, cx);
        if layer_mode {
            return self.render_layer_trigger(menu_state, window, cx);
        }
        let force_open = menu_state.update(cx, |state, _| std::mem::take(&mut state.force_open));
        let popover = gpui_component::popover::Popover::new(ElementId::Name(SharedString::from(
            format!("pointer-menu-popover:{:?}", self.id),
        )));
        // EXP-1259: controlled for this one render; uncontrolled after, so
        // a pick, Escape or a click outside still closes it.
        let popover = if self.dev_open || force_open { popover.open(true) } else { popover };
        popover
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
                    let dev_select = menu_state.update(cx, |state, _| {
                        state.menu = Some(menu.clone());
                        std::mem::take(&mut state.dev_select)
                    });
                    menu.focus_handle(cx).focus(window, cx);
                    if dev_select {
                        // The first frame that PAINTED the menu owns its key
                        // context: Down from nothing selects row 0.
                        let handle = menu.focus_handle(cx);
                        window.on_next_frame(move |window, cx| {
                            handle.dispatch_action(&gpui_base::actions::SelectDown, window, cx);
                        });
                    }
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
            overlay_enter(
                ElementId::Name(SharedString::from(format!("pointer-menu-enter:{:?}", self.id))),
                with_pointer_layer(menu),
            )
        })
        .into_any_element()
    }
}

/// EXP-1243 — an overlay's ENTER (a menu, a popover the pointer opened): a
/// fade over [`theme::motion::ENTER`] on `decelerate`. gpui has no element
/// scale, so the web's `scale 0.96 → 1` is the fade alone; the exit is
/// instant (the popover unmounts). Never for a keyboard-opened surface.
pub(crate) fn overlay_enter(id: impl Into<ElementId>, overlay: Div) -> impl IntoElement {
    overlay.with_animation(
        id,
        Animation::new(theme::motion::ENTER).with_easing(theme::motion::decelerate()),
        |overlay, delta| overlay.opacity(delta),
    )
}

/// A menu surface under the pointer layer (half 2 of the section note): the
/// layer covers the menu, sets the cursor and nothing else. Shared by
/// [`PointerMenu`] and the styleguide's open specimens. The menu lays out in
/// a [`MenuRem`] scope, so the crate's own rows read the tokens' type too.
pub(crate) fn with_pointer_layer(menu: impl IntoElement) -> Div {
    div()
        .relative()
        .child(MenuRem(menu.into_any_element()))
        .child(div().absolute().top_0().left_0().size_full().cursor_pointer())
}

/// A menu laid out at the web's 16px root (`type.baseSize`) instead of the
/// IDE's 14px rem: gpui-component sizes EVERY row's text `text_sm`
/// (0.875rem, crate-fixed, `Submenu` and plain rows included), which is
/// 12.25px at the IDE's rem and the tokens' 14px (web `text-sm`) here. A
/// submenu is a deferred child of its menu and keeps the scope (gpui's
/// deferred draw records the rem). Zed's `ui::WithRemSize`, minus the div.
struct MenuRem(AnyElement);

impl MenuRem {
    const REM: f32 = t::typography::BASE_SIZE;
}

impl IntoElement for MenuRem {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::Element for MenuRem {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, ()) {
        let layout = window.with_rem_size(Some(px(Self::REM)), |window| self.0.request_layout(window, cx));
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(px(Self::REM)), |window| self.0.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(px(Self::REM)), |window| self.0.paint(window, cx));
    }
}

// ---------------------------------------------------------------------------
// The pointer CONTEXT menu (wave D): `PointerMenu`'s right-click twin
// ---------------------------------------------------------------------------

/// The right-click twin of [`PointerMenu`]: gpui-component's `ContextMenu`
/// (rev da4f936) with the SAME pointer layer over the open menu, so a
/// `Submenu` row (Status ›, Labels › …, which has no element form) points
/// like every element row does. Behaviour otherwise mirrors upstream: a
/// right press inside the element opens the menu at the pointer (built on
/// the next frame from current state), a pick / Escape / click outside
/// dismisses it, and the dismiss hands focus back to whatever held it before
/// the press unless the pick moved it elsewhere. Upstream's private
/// `set_previous_focus` is out of reach, so that restore lives in the
/// dismiss subscription here.
pub(crate) trait PointerContextMenuExt: InteractiveElement + gpui::ParentElement + Styled {
    #[track_caller]
    fn pointer_context_menu(
        mut self,
        builder: impl Fn(
                gpui_component::menu::PopupMenu,
                &mut Window,
                &mut gpui::Context<gpui_component::menu::PopupMenu>,
            ) -> gpui_component::menu::PopupMenu
            + 'static,
    ) -> PointerContextMenu<Self>
    where
        Self: Sized,
    {
        let caller = std::panic::Location::caller();
        let id = self
            .interactivity()
            .element_id
            .clone()
            .map(|id| ElementId::Name(format!("pointer-context-menu-{id:?}").into()))
            .unwrap_or_else(|| ElementId::CodeLocation(*caller));
        PointerContextMenu {
            id,
            element: Some(self),
            builder: std::rc::Rc::new(builder),
            ignore_style: gpui::StyleRefinement::default(),
        }
    }
}

impl<E: InteractiveElement + gpui::ParentElement + Styled> PointerContextMenuExt for E {}

type ContextMenuBuilder = std::rc::Rc<
    dyn Fn(
        gpui_component::menu::PopupMenu,
        &mut Window,
        &mut gpui::Context<gpui_component::menu::PopupMenu>,
    ) -> gpui_component::menu::PopupMenu,
>;

/// See [`PointerContextMenuExt`].
pub(crate) struct PointerContextMenu<E: gpui::ParentElement + Styled + Sized> {
    id: ElementId,
    element: Option<E>,
    builder: ContextMenuBuilder,
    // Never read: the style refinement forwarded once the element is taken.
    ignore_style: gpui::StyleRefinement,
}

impl<E: gpui::ParentElement + Styled> gpui::ParentElement for PointerContextMenu<E> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        if let Some(element) = &mut self.element {
            element.extend(elements);
        }
    }
}

impl<E: gpui::ParentElement + Styled> Styled for PointerContextMenu<E> {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        match &mut self.element {
            Some(element) => element.style(),
            None => &mut self.ignore_style,
        }
    }
}

impl<E: gpui::ParentElement + Styled + IntoElement + 'static> IntoElement for PointerContextMenu<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[derive(Default)]
struct PointerContextShared {
    menu: Option<gpui::Entity<gpui_component::menu::PopupMenu>>,
    open: bool,
    position: gpui::Point<Pixels>,
    previous_focus: Option<gpui::FocusHandle>,
    subscription: Option<gpui::Subscription>,
}

#[derive(Default)]
pub(crate) struct PointerContextState {
    element: Option<AnyElement>,
    shared: std::rc::Rc<std::cell::RefCell<PointerContextShared>>,
}

impl<E: gpui::ParentElement + Styled> PointerContextMenu<E> {
    fn with_state<R>(
        &mut self,
        id: &gpui::GlobalElementId,
        window: &mut Window,
        cx: &mut App,
        f: impl FnOnce(&mut Self, &mut PointerContextState, &mut Window, &mut App) -> R,
    ) -> R {
        window.with_optional_element_state::<PointerContextState, _>(Some(id), |state, window| {
            let mut state = state.unwrap().unwrap_or_default();
            let result = f(self, &mut state, window, cx);
            (result, Some(state))
        })
    }
}

impl<E: gpui::ParentElement + Styled + IntoElement + 'static> gpui::Element for PointerContextMenu<E> {
    type RequestLayoutState = PointerContextState;
    type PrepaintState = gpui::Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        use gpui::Focusable as _;
        self.with_state(id.unwrap(), window, cx, |this, state, window, cx| {
            let (position, open, menu) = {
                let shared = state.shared.borrow();
                (shared.position, shared.open, shared.menu.clone())
            };
            let menu_element = match menu {
                Some(menu) if open && !menu.read(cx).is_empty() => {
                    if !menu.focus_handle(cx).contains_focused(window, cx) {
                        menu.focus_handle(cx).focus(window, cx);
                    }
                    Some(
                        deferred(
                            anchored().child(
                                div()
                                    .w(window.bounds().size.width)
                                    .h(window.bounds().size.height)
                                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                                    .child(
                                        anchored()
                                            .position(position)
                                            .snap_to_window_with_margin(px(8.))
                                            .anchor(Anchor::TopLeft)
                                            .child(overlay_enter(
                                                "context-menu-enter",
                                                with_pointer_layer(menu),
                                            )),
                                    ),
                            ),
                        )
                        .with_priority(gpui_base::POPUP_PRIORITY)
                        .into_any(),
                    )
                }
                _ => None,
            };
            let mut element = this
                .element
                .take()
                .expect("the context menu's element is laid out once")
                .children(menu_element)
                .into_any_element();
            let layout_id = element.request_layout(window, cx);
            (
                layout_id,
                PointerContextState {
                    element: Some(element),
                    ..Default::default()
                },
            )
        })
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(element) = &mut state.element {
            element.prepaint(window, cx);
        }
        window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        id: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(element) = &mut state.element {
            element.paint(window, cx);
        }
        let builder = self.builder.clone();
        let layer_id = self.id.clone();
        self.with_state(id.unwrap(), window, cx, |_, state, window, _| {
            let shared = state.shared.clone();
            let hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
                if !phase.bubble()
                    || event.button != gpui::MouseButton::Right
                    || !hitbox.is_hovered(window)
                {
                    return;
                }
                // N-6: in a dialog window the menu opens in a menu layer
                // over the window (`menu_window`), hung at the pointer; the
                // in-window menu below is what a refused layer falls back to.
                if crate::menu_window::wanted(window, cx) {
                    let shared = shared.clone();
                    let builder = builder.clone();
                    let position = event.position;
                    crate::menu_window::open(
                        window,
                        cx,
                        crate::menu_window::MenuAnchor {
                            rect: gpui::Bounds {
                                origin: position,
                                size: gpui::size(px(0.), px(0.)),
                            },
                            wanted: px(crate::menu_window::DEFAULT_WANTED_HEIGHT),
                        },
                        builder.clone(),
                        SharedString::from(format!("{:?}", layer_id)),
                        false,
                        move |window, cx| open_in_window(&shared, &builder, position, window, cx),
                    );
                    return;
                }
                open_in_window(&shared, &builder, event.position, window, cx);
            });
        });
    }
}

/// The in-window half of [`PointerContextMenu`]: record the press, build the
/// menu on the next frame from current state and hand focus back on dismiss.
fn open_in_window(
    shared: &std::rc::Rc<std::cell::RefCell<PointerContextShared>>,
    builder: &ContextMenuBuilder,
    position: gpui::Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    {
                {
                    let mut state = shared.borrow_mut();
                    // A press while our own menu holds focus keeps the focus
                    // captured when THAT menu opened.
                    let focused = window.focused(cx);
                    let inside_menu = match (&state.menu, &focused) {
                        (Some(menu), Some(focused)) => {
                            use gpui::Focusable as _;
                            menu.focus_handle(cx) == *focused
                        }
                        _ => false,
                    };
                    if !inside_menu {
                        state.previous_focus = focused;
                    }
                    state.menu = None;
                    state.subscription = None;
                    state.position = position;
                    state.open = true;
                }
                window.defer(cx, {
                    let shared = shared.clone();
                    let builder = builder.clone();
                    move |window, cx| {
                        let menu = gpui_component::menu::PopupMenu::build(
                            window,
                            cx,
                            move |menu, window, cx| builder(menu, window, cx),
                        );
                        let subscription = window.subscribe(&menu, cx, {
                            let shared = shared.clone();
                            move |menu, _: &gpui::DismissEvent, window, cx| {
                                use gpui::Focusable as _;
                                let previous = {
                                    let mut state = shared.borrow_mut();
                                    state.open = false;
                                    state.previous_focus.take()
                                };
                                // Hand focus back unless the pick moved it
                                // (a dialog's input, say).
                                let lost = window.focused(cx).is_none()
                                    || menu.focus_handle(cx).contains_focused(window, cx);
                                if let (true, Some(previous)) = (lost, previous) {
                                    window.focus(&previous, cx);
                                }
                                window.refresh();
                            }
                        });
                        let mut state = shared.borrow_mut();
                        state.menu = Some(menu);
                        state.subscription = Some(subscription);
                        window.refresh();
                    }
                });
    }
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
    use super::{tail_reached, TailFollow, TailLayout};

    /// EXP-1316 (1n): the follow opens pinned, an upward wheel releases
    /// it, growth keeps it, landing back on the end re-arms it, the button
    /// re-pins.
    #[test]
    fn the_tail_follow_releases_up_and_rearms_on_the_end() {
        assert!(tail_reached(0., 0.), "nothing to scroll");
        assert!(tail_reached(-400., 400.));
        assert!(tail_reached(-399.5, 400.));
        assert!(!tail_reached(-200., 400.));
        let follow = TailFollow::default();
        assert!(follow.following());
        // Opening: the first layouts grow from nothing — re-pin each time.
        assert_eq!(follow.on_layout(0., 0.), TailLayout::Keep);
        assert_eq!(follow.on_layout(0., 600.), TailLayout::Repin, "rows landed async");
        assert_eq!(follow.on_layout(-600., 600.), TailLayout::Keep);
        assert_eq!(follow.on_layout(-600., 900.), TailLayout::Repin, "a card measured later");
        assert!(follow.following());
        assert!(!follow.on_wheel(-30.), "down keeps it");
        assert!(follow.on_wheel(30.));
        assert!(!follow.following());
        assert_eq!(follow.on_layout(-200., 900.), TailLayout::Keep, "still up");
        assert_eq!(follow.on_layout(-900., 900.), TailLayout::Rearmed, "back on the end");
        assert!(follow.following());
        // A drag off the end without growth releases it.
        assert_eq!(follow.on_layout(-500., 900.), TailLayout::Released);
        assert!(!follow.following());
        follow.pin();
        assert!(follow.following());
    }

    /// The snap is ISSUED, never standing: the opening render owes one, the
    /// renders a scrollbar drag triggers owe none (so the probe reads the
    /// dragged offset and releases), a re-pin snaps from the probe, the
    /// button owes one again. Released = never.
    #[test]
    fn the_tail_follow_snaps_once_on_open_and_never_under_a_drag() {
        let follow = TailFollow::default();
        assert!(follow.take_snap(), "the opening render snaps");
        assert!(!follow.take_snap(), "the next one does not");
        assert_eq!(follow.on_layout(-600., 600.), TailLayout::Keep);
        // Growth under a pinned reader: the probe snaps itself (`Repin`),
        // the render it notifies owes nothing — a drag in that frame wins.
        assert_eq!(follow.on_layout(-600., 900.), TailLayout::Repin);
        assert!(!follow.take_snap());
        // The drag: `set_offset` re-renders the view (no snap), then the
        // probe reads the dragged offset without growth → released.
        assert!(!follow.take_snap());
        assert_eq!(follow.on_layout(-500., 900.), TailLayout::Released);
        assert!(!follow.following());
        assert!(!follow.take_snap(), "released owes no snap");
        // The button: pinned, and the next render snaps once.
        follow.pin();
        assert!(follow.take_snap());
        assert!(!follow.take_snap());
        // Back on the end by hand re-arms without a snap of its own.
        assert!(follow.on_wheel(30.));
        assert_eq!(follow.on_layout(-900., 900.), TailLayout::Rearmed);
        assert!(!follow.take_snap());
    }


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
