//! N-12: THE Dock ×desktop (`fixtures/dock.json` ×4; web
//! `packages/ui/src/dock.tsx`, model [`domain::dock`]): the floating band
//! over the transcript's bottom edge in the composer column: the DockCards
//! (a waiting plan or question, THE Queue, the queued messages, a status
//! banner) piled like the toasts ([`domain::stack::stack_layout`] with the
//! Dock's `stack`, anchored at the bottom: the front card last, the older
//! ones peeking out above it, `visible` at most; hovering fans them `gap`
//! apart), then THE composer, which never leaves. A click on a card opens
//! its sheet ([`sheet`]: a scrim over the window, the sheet in the composer
//! column at most `sheet.maxHeightFraction` of the window, a pinned header,
//! ONE scrolling body, a pinned footer; Escape or the scrim closes it). The
//! band measures itself ([`DockState::band_height`]): the host pads the
//! transcript by it and lifts the jump button over it. gpui has no
//! reduce-motion signal (`theme::motion`), so the motion always runs.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;

use domain::dock::{dock, dock_auto_open, DockMode, DockRow, DockTint, DOCK};
use gpui::{
    anchored, canvas, deferred, div, linear_color_stop, linear_gradient, point, prelude::FluentBuilder as _, px,
    AnyElement, App, Bounds, Div, ElementId, Entity, FocusHandle, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _,
    Window,
};
use gpui_base::{transition, Transition};
use gpui_component::{button::{Button, ButtonVariants as _}, h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};
use theme::tokens as t;

use crate::controls::WebControl as _;
use crate::icons::registry;

/// Tailwind `amber-400`: the warning banner's tint (web `text-amber-400`).
const AMBER: u32 = 0xfbbf24;

/// The transcript's last row clears the band by this much (web `DOCK_GAP`).
pub(crate) const DOCK_GAP: f32 = 16.;

/// The band's own padding under the composer (web `md:pb-4`).
const BAND_BOTTOM: f32 = 16.;

/// How far the fade reaches above the band (web `-top-10`).
const FADE_ABOVE: f32 = 40.;

/// How long after the host first reads as loaded the cards still count as
/// already waiting (a replayed history lands over a few frames).
const SETTLE: std::time::Duration = std::time::Duration::from_millis(1500);

/// A card's height before its first measurement.
const ESTIMATED_CARD: f32 = 62.;

/// The Dock's per-host state: the fan, the open sheet (and the one leaving),
/// the measured heights, the ids auto-open has seen.
pub(crate) struct DockState {
    expanded: bool,
    open: Option<String>,
    opened_at: Option<Instant>,
    closing: Option<(Instant, String)>,
    heights: Rc<RefCell<HashMap<String, f32>>>,
    band_height: Rc<Cell<f32>>,
    column: Rc<Cell<Option<Bounds<Pixels>>>>,
    pile_width: Rc<Cell<f32>>,
    sheet_height: Rc<Cell<f32>>,
    seen: HashSet<String>,
    /// When the host's state first read as loaded (auto-open's memory
    /// settles over [`SETTLE`] from then on).
    primed: Option<Instant>,
    focus_pending: bool,
    focus: Option<FocusHandle>,
}

impl Default for DockState {
    fn default() -> Self {
        Self {
            expanded: false,
            open: None,
            opened_at: None,
            closing: None,
            heights: Rc::default(),
            band_height: Rc::new(Cell::new(0.)),
            column: Rc::new(Cell::new(None)),
            pile_width: Rc::new(Cell::new(0.)),
            sheet_height: Rc::new(Cell::new(0.)),
            seen: HashSet::new(),
            primed: None,
            focus_pending: false,
            focus: None,
        }
    }
}

impl DockState {
    /// A state fanned on mount (the styleguide's fanned demo).
    pub(crate) fn fanned() -> Self {
        Self { expanded: true, ..Self::default() }
    }

    /// The open sheet's card id.
    pub(crate) fn open_id(&self) -> Option<&str> {
        self.open.as_deref()
    }

    /// The sheet to paint: the open one, else the one sliding out.
    pub(crate) fn sheet_id(&self) -> Option<&str> {
        self.open.as_deref().or(self.closing.as_ref().map(|(_, id)| id.as_str()))
    }

    pub(crate) fn open(&mut self, id: impl Into<String>) {
        self.open = Some(id.into());
        self.opened_at = Some(Instant::now());
        self.closing = None;
        self.focus_pending = true;
    }

    pub(crate) fn close(&mut self) {
        if let Some(id) = self.open.take() {
            self.closing = Some((Instant::now(), id));
        }
    }

    /// Release train 2026-10-23 (N-12 F2): the open card vanished (`dock`
    /// reads no open id while one is held): forget it, or a later row with
    /// the same fixed id (`tasks`, `queued`, `banner:*`) reopens the sheet
    /// unasked. Returns whether it forgot one.
    pub(crate) fn forget_vanished(&mut self, view_open_id: Option<&str>) -> bool {
        if self.open.is_none() || view_open_id.is_some() {
            return false;
        }
        self.open = None;
        self.opened_at = None;
        true
    }

    /// The band's measured height (px): the transcript's bottom padding.
    pub(crate) fn band_height(&self) -> f32 {
        self.band_height.get()
    }

    /// AUTO-OPEN (dock.json): the plan or question that just arrived opens
    /// its sheet when the view is in front (`in_front`). What already waits
    /// when the view opens stays collapsed: the memory seeds itself from the
    /// first frame the host's state is `loaded` (a relay feed replays its
    /// history after the first frame) and keeps seeding over [`SETTLE`].
    /// Returns whether a sheet opened.
    pub(crate) fn auto_open(&mut self, rows: &[DockRow], in_front: bool, loaded: bool) -> bool {
        let settling = match self.primed {
            None if loaded => {
                self.primed = Some(Instant::now());
                true
            }
            None => true,
            Some(since) => since.elapsed() < SETTLE,
        };
        if settling {
            self.seen.extend(rows.iter().map(|row| row.id.clone()));
            return false;
        }
        let previous: Vec<String> = self.seen.iter().cloned().collect();
        let next = dock_auto_open(&previous, rows.iter().map(|row| (row.id.as_str(), row.kind)));
        self.seen.extend(rows.iter().map(|row| row.id.clone()));
        match next {
            Some(id) if in_front && self.open.is_none() => {
                self.open(id);
                true
            }
            _ => false,
        }
    }
}

fn tint(tint: DockTint, cx: &App) -> Hsla {
    match tint {
        DockTint::Plan => cx.theme().primary,
        DockTint::Question => t::YELLOW.to_hsla(),
        DockTint::Muted => cx.theme().muted_foreground,
        DockTint::Warning => gpui::rgb(AMBER).into(),
    }
}

fn concept(name: &str) -> crate::icons::ExpIcon {
    registry::concept_by_name(name).unwrap_or(registry::UI_INFO)
}

/// THE card fill (N-12: ONE gray ×4, web `bg-glass-card-opaque`): the card
/// fill over the popover surface — every card, the composer, the sheet.
pub(crate) fn card_fill() -> Hsla {
    crate::controls::ide_chrome().fill_card_opaque
}

/// A DockCard collapsed (web `DockCard`): the AskCard chrome (the kind
/// icon, the tinted label, the muted caption, the trailing meta) over the
/// one title line and the open chevron (an expandable card) or the
/// banner's `action`. `hidden` = a card behind the front one while piled
/// (its chrome peeks, its words do not). The caller wires the click.
pub(crate) fn dock_card(row: &DockRow, hidden: f32, action: Option<AnyElement>, cx: &App) -> Stateful<Div> {
    let muted = cx.theme().muted_foreground;
    let color = tint(row.tint, cx);
    let header = h_flex()
        .w_full()
        .min_w_0()
        .mb_1()
        .gap_2()
        .items_center()
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(color)
                .child(SharedString::from(row.label.clone())),
        )
        .when_some(row.caption.clone(), |header, caption| {
            header.child(div().min_w_0().truncate().text_xs().text_color(muted).child(SharedString::from(caption)))
        })
        .when_some(row.meta.clone(), |header, meta| {
            header.child(div().ml_auto().flex_shrink_0().text_xs().text_color(muted).child(SharedString::from(meta)))
        });
    let trailing = if row.expandable {
        Some(Icon::new(concept(&DOCK.icons.open)).with_size(px(14.)).flex_shrink_0().text_color(muted).into_any_element())
    } else {
        action.map(|action| div().flex_shrink_0().child(action).into_any_element())
    };
    let title = h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(cx.theme().foreground.opacity(0.9))
                .child(SharedString::from(row.title.clone())),
        )
        .children(trailing);
    crate::surface::glass_card()
        .id(SharedString::from(format!("{}-{}", DOCK.test_ids.card, row.id)))
        .w_full()
        .min_w_0()
        .p_3()
        .when(row.expandable, |card| {
            card.cursor_pointer()
                .hover(|style| style.bg(crate::picker::over(card_fill(), t::glass::FILL_ACTIVE.to_hsla())))
        })
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .items_start()
                .opacity(1. - hidden)
                .child(
                    div()
                        .flex_shrink_0()
                        .mt(px(1.))
                        .child(Icon::new(concept(&row.icon)).with_size(px(14.)).text_color(color)),
                )
                .child(v_flex().flex_1().min_w_0().child(header).child(title)),
        )
}

fn slow() -> Transition {
    Transition::new(theme::motion::SLOW).ease(theme::motion::standard())
}

/// THE band's facts.
pub(crate) struct Band {
    /// Element-id prefix (one per host).
    pub id: SharedString,
    /// `dock_rows(sources)`, top to bottom.
    pub rows: Vec<DockRow>,
    /// The banners' action buttons, by row id.
    pub actions: Vec<(String, AnyElement)>,
    /// THE composer card, under the cards.
    pub composer: Option<AnyElement>,
}

/// THE floating band, absolutely placed over the bottom of its host's
/// transcript region (the host's box must be `relative()`): the edge fade
/// behind it, the pile, then the composer, in the composer column.
pub(crate) fn band(state: &Entity<DockState>, band: Band, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let Band { id, rows, mut actions, composer } = band;
    let (expanded, open) = {
        let state = state.read(cx);
        (state.expanded, state.open.clone())
    };
    let view = dock(&rows, expanded, open.as_deref());
    if open.is_some() && view.open_id.is_none() {
        state.update(cx, |state, _| state.forget_vanished(view.open_id.as_deref()));
    }
    if view.mode == DockMode::None && composer.is_none() {
        state.read(cx).band_height.set(0.);
        return None;
    }
    let pile = (view.mode != DockMode::None).then(|| pile(state, &id, &view.rows, view.mode, &mut actions, window, cx));
    let (band_height, column) = {
        let state = state.read(cx);
        (state.band_height.clone(), state.column.clone())
    };
    let probe = canvas(
        move |bounds, window, _| {
            let height = f32::from(bounds.size.height);
            if (band_height.get() - height).abs() > 0.5 {
                band_height.set(height);
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full();
    let column_probe = canvas(move |bounds, _, _| column.set(Some(bounds)), |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full();
    Some(
        div()
            .id(SharedString::from(format!("{id}-{}", DOCK.test_ids.dock)))
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .child(crate::surface::dock_edge_fade(FADE_ABOVE))
            .child(
                div().w_full().flex().flex_col().items_center().pb(px(BAND_BOTTOM)).child(
                    v_flex()
                        .relative()
                        .w_full()
                        .min_w_0()
                        .max_w(px(crate::work_header::WORK_COLUMN_W))
                        .px(px(crate::work_header::WORK_GUTTER))
                        .gap(px(DOCK.stack.gap))
                        .children(pile)
                        .children(composer)
                        .child(column_probe),
                ),
            )
            .child(probe)
            .into_any_element(),
    )
}

/// The pile: every card absolutely placed by the stack geometry (measured
/// heights), the front one last; collapsed = the older cards peek out above
/// it, their words hidden; hovering fans it.
pub(crate) fn pile(
    state: &Entity<DockState>,
    id: &SharedString,
    rows: &[DockRow],
    mode: DockMode,
    actions: &mut Vec<(String, AnyElement)>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let fanned = mode != DockMode::Pile;
    let measured = state.read(cx).heights.clone();
    let heights: Vec<f32> = {
        let measured = measured.borrow();
        rows.iter().map(|row| measured.get(&row.id).copied().unwrap_or(ESTIMATED_CARD)).collect()
    };
    let (height, items) = domain::stack::stack_layout(&heights, fanned, true, DOCK.stack);
    let pile_width = state.read(cx).pile_width.clone();
    let width = pile_width.get();
    let front = rows.last().map(|row| row.id.clone());
    let box_height = transition(ElementId::Name(format!("{id}-pile-height").into()), px(height), slow(), window, cx);
    let mut cards: Vec<AnyElement> = Vec::with_capacity(rows.len());
    for (row, item) in rows.iter().zip(&items) {
        let key = format!("{id}-pile-{}", row.id);
        let is_front = Some(&row.id) == front.as_ref();
        let top = transition(ElementId::Name(format!("{key}-top").into()), px(item.offset), slow(), window, cx);
        let inset = transition(
            ElementId::Name(format!("{key}-inset").into()),
            px(width.max(0.) * (1. - item.scale) / 2.),
            slow(),
            window,
            cx,
        );
        let shown = transition(
            ElementId::Name(format!("{key}-shown").into()),
            if item.visible { 1f32 } else { 0. },
            slow(),
            window,
            cx,
        );
        let hidden = transition(
            ElementId::Name(format!("{key}-hidden").into()),
            if fanned || is_front { 0f32 } else { 1. },
            slow(),
            window,
            cx,
        );
        let action = actions.iter().position(|(row_id, _)| *row_id == row.id).map(|ix| actions.remove(ix).1);
        let (row_id, expandable) = (row.id.clone(), row.expandable);
        let opener = state.clone();
        let heights = measured.clone();
        let measure_id = row.id.clone();
        cards.push(
            div()
                .absolute()
                .left(inset)
                .right(inset)
                .top(top)
                .opacity(shown)
                .on_children_prepainted(move |bounds, window, _| {
                    let Some(bounds) = bounds.first() else { return };
                    let height = f32::from(bounds.size.height);
                    let mut heights = heights.borrow_mut();
                    if heights.get(&measure_id).is_none_or(|h| (h - height).abs() > 0.5) {
                        heights.insert(measure_id.clone(), height);
                        window.refresh();
                    }
                })
                .child(dock_card(row, hidden, action, cx).when(expandable && item.visible, |card| {
                    card.on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        opener.update(cx, |state, _| state.open(row_id.clone()));
                        cx.refresh_windows();
                    })
                }))
                .into_any_element(),
        );
    }
    let hover = state.clone();
    let width_probe = canvas(
        move |bounds, window, _| {
            let width = f32::from(bounds.size.width);
            if (pile_width.get() - width).abs() > 0.5 {
                pile_width.set(width);
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full();
    div()
        .id(SharedString::from(format!("{id}-{}", DOCK.test_ids.pile)))
        .relative()
        .w_full()
        .h(box_height)
        .child(width_probe)
        .when(mode != DockMode::Single, |pile| {
            pile.on_hover(move |hovered, _, cx| {
                let changed = hover.update(cx, |state, _| std::mem::replace(&mut state.expanded, *hovered) != *hovered);
                if changed {
                    cx.refresh_windows();
                }
            })
        })
        .children(cards)
        .into_any_element()
}

thread_local! {
    /// How many Dock sheets are building their content right now (see
    /// [`sheet_host`]).
    static SHEET_HOST: Cell<usize> = const { Cell::new(0) };
}

/// N-6 + N-14 (`picker-presentation.json`): the Dock sheet is a SHEET host.
/// Build a sheet's content inside this scope and every picker in it takes
/// the INLINE presentation (the picker reads [`in_sheet_host`] like a
/// dialog window), its rows in the sheet's flow. Menus stay IN-WINDOW
/// popovers: the sheet is a layer of the window, not a dialog window, so
/// they never route through the menu window and paint over the sheet.
pub(crate) fn sheet_host<R>(build: impl FnOnce() -> R) -> R {
    SHEET_HOST.with(|depth| depth.set(depth.get() + 1));
    let out = build();
    SHEET_HOST.with(|depth| depth.set(depth.get() - 1));
    out
}

/// Whether a Dock sheet's content is being built ([`sheet_host`]).
pub(crate) fn in_sheet_host() -> bool {
    SHEET_HOST.with(|depth| depth.get() > 0)
}

/// [`sheet_host`] held over a subtree's LAYOUT too: a picker mounted from a
/// window-less helper ([`crate::picker::deferred`]) builds while its parent
/// lays out, after the content's own build scope has closed.
struct SheetHostScope(AnyElement);

impl IntoElement for SheetHostScope {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for SheetHostScope {
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
        (sheet_host(|| self.0.request_layout(window, cx)), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        sheet_host(|| self.0.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.paint(window, cx);
    }
}

/// A sheet's content: the header's trailing meta (the plan's "Question 2
/// of 3"), the ONE scrolling body, the pinned footer.
pub(crate) struct SheetContent {
    pub meta: Option<SharedString>,
    /// VAPP-129: a trailing header button before the close glyph (web: the
    /// meta slot takes any node; the app editor's "Move to the side").
    pub action: Option<AnyElement>,
    pub body: AnyElement,
    pub footer: Option<AnyElement>,
}

/// The sheet's ONE scrolling region (web `DockSheetBody`: `px-4 pb-3`,
/// `text-sm`).
pub(crate) fn sheet_body(id: impl Into<ElementId>, body: impl IntoElement) -> AnyElement {
    div()
        .id(id)
        .flex_shrink(1.)
        .min_h_0()
        .overflow_y_scroll()
        .px_4()
        .pb_3()
        .text_sm()
        .child(body)
        .into_any_element()
}

/// The sheet's pinned footer (web `DockSheetFooter`): a hairline over the
/// primary actions.
pub(crate) fn sheet_footer(body: impl IntoElement) -> AnyElement {
    v_flex()
        .id(SharedString::from(DOCK.test_ids.sheet_footer.clone()))
        .flex_shrink_0()
        .gap_2()
        .border_t_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .px_4()
        .py_3()
        .child(body)
        .into_any_element()
}

/// The sheet's face (web `DockSheetChrome`): the pinned header (the card's
/// icon, label, caption, the content's meta, the close glyph) over the body
/// and the footer. The live sheet and the styleguide's frame share it.
pub(crate) fn sheet_chrome(
    row: &DockRow,
    content: SheetContent,
    on_close: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    cx: &App,
) -> Vec<AnyElement> {
    let muted = cx.theme().muted_foreground;
    let color = tint(row.tint, cx);
    let close = Button::new(SharedString::from(DOCK.test_ids.close.clone()))
        .ghost()
        .web_icon_xs()
        .icon(concept(&DOCK.icons.close))
        .tooltip(DOCK.copy.close.clone())
        .when_some(on_close, |button, on_close| {
            button.on_click(move |_, window, cx| {
                cx.stop_propagation();
                on_close(window, cx);
            })
        });
    let header = h_flex()
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .px_4()
        .pt_3()
        .pb_2()
        .child(Icon::new(concept(&row.icon)).with_size(px(14.)).flex_shrink_0().text_color(color))
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(color)
                .child(SharedString::from(row.label.clone())),
        )
        .when_some(row.caption.clone(), |header, caption| {
            header.child(div().min_w_0().truncate().text_xs().text_color(muted).child(SharedString::from(caption)))
        })
        .child(
            h_flex()
                .ml_auto()
                .flex_shrink_0()
                .gap_2()
                .items_center()
                .text_xs()
                .text_color(muted)
                .when_some(content.meta, |meta_row, meta| meta_row.child(meta))
                .children(content.action)
                .child(close),
        );
    let mut out = vec![header.into_any_element(), content.body];
    out.extend(content.footer);
    out
}

/// The sheet's surface: THE card fill (N-12: ONE gray, not a darker
/// ground), the card hairline on the top and the sides, `sheet.radius` on
/// its top corners.
fn sheet_surface() -> Div {
    v_flex()
        .bg(card_fill())
        .border_t_1()
        .border_l_1()
        .border_r_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .rounded_tl(px(DOCK.sheet.radius))
        .rounded_tr(px(DOCK.sheet.radius))
        .overflow_hidden()
}

/// The styleguide's sheet at rest (no overlay): the same chrome in a box
/// wearing the sheet's face, `height` tall.
pub(crate) fn sheet_frame(row: &DockRow, content: SheetContent, height: f32, cx: &App) -> AnyElement {
    SheetHostScope(sheet_surface().w_full().h(px(height)).children(sheet_chrome(row, content, None, cx)).into_any_element())
        .into_any_element()
}

/// THE sheet a DockCard opens into, as a layer over the window: the scrim,
/// then the sheet in the composer column at the window's bottom edge, at
/// most `sheet.maxHeightFraction` of the window, fitting its content below
/// that. It slides up over motion `slow` (decelerate) and out over
/// `standard` (accelerate); Escape, the scrim or the close glyph closes it.
pub(crate) fn sheet(state: &Entity<DockState>, row: &DockRow, content: SheetContent, window: &mut Window, cx: &mut App) -> AnyElement {
    let viewport = window.viewport_size();
    let (opened_at, closing, column, measured) = {
        let state = state.read(cx);
        (state.opened_at, state.closing.clone(), state.column.get(), state.sheet_height.clone())
    };
    // The sheet's travel: 0 = in place, 1 = below the window edge.
    let progress = match &closing {
        Some((since, _)) => {
            let raw = (since.elapsed().as_secs_f32() / theme::motion::STANDARD.as_secs_f32()).min(1.);
            if raw >= 1. {
                state.update(cx, |state, _| state.closing = None);
                return div().into_any_element();
            }
            1. - theme::motion::accelerate()(raw)
        }
        None => {
            let raw = opened_at
                .map(|since| (since.elapsed().as_secs_f32() / theme::motion::SLOW.as_secs_f32()).min(1.))
                .unwrap_or(1.);
            theme::motion::decelerate()(raw)
        }
    };
    let animating = closing.is_some() || progress < 1.;
    if animating {
        window.request_animation_frame();
    }
    let focus = state.update(cx, |state, cx| state.focus.get_or_insert_with(|| cx.focus_handle()).clone());
    if state.read(cx).focus_pending {
        state.update(cx, |state, _| state.focus_pending = false);
        window.focus(&focus, cx);
    }
    let (left, width) = match column {
        Some(bounds) => (bounds.origin.x, bounds.size.width),
        None => (px(0.), viewport.width),
    };
    let close: Rc<dyn Fn(&mut Window, &mut App)> = {
        let state = state.clone();
        Rc::new(move |_, cx| {
            state.update(cx, |state, _| state.close());
            cx.refresh_windows();
        })
    };
    let height = measured.get();
    let drop = px(height.max(f32::from(viewport.height) * 0.3) * (1. - progress));
    let scrim_close = close.clone();
    let escape_close = close.clone();
    let probe = canvas(
        move |bounds, _, _| measured.set(f32::from(bounds.size.height)),
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full();
    deferred(
        anchored().position(point(px(0.), px(0.))).child(
            div()
                .id(SharedString::from(format!("{}-layer", DOCK.test_ids.sheet)))
                .relative()
                .w(viewport.width)
                .h(viewport.height)
                .occlude()
                .child(
                    div()
                        .id("dock-sheet-scrim")
                        .absolute()
                        .inset_0()
                        .bg(gpui::black().opacity(0.6 * progress))
                        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| scrim_close(window, cx)),
                )
                .child(SheetHostScope(
                    sheet_surface()
                        .id(SharedString::from(format!("{}-{}", DOCK.test_ids.sheet, row.id)))
                        .track_focus(&focus)
                        .on_key_down(move |event, window, cx| {
                            if event.keystroke.key == "escape" {
                                cx.stop_propagation();
                                escape_close(window, cx);
                            }
                        })
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .absolute()
                        .left(left)
                        .w(width)
                        .bottom(-drop)
                        .max_h(viewport.height * DOCK.sheet.max_height_fraction)
                        .children(sheet_chrome(row, content, Some(close), cx))
                        .child(probe)
                        .into_any_element(),
                )),
        ),
    )
    .into_any_element()
}

/// The edge fade's gradient: the ground at the bottom, nothing at the top
/// (web `linear-gradient(to top, var(--background) 72%, transparent)`).
pub(crate) fn fade_gradient(ground: Hsla) -> gpui::Background {
    linear_gradient(180., linear_color_stop(ground.opacity(0.), 0.), linear_color_stop(ground, 0.28))
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::dock::{dock_rows, DockSource};

    fn plan(id: &str, at: i64) -> DockSource {
        DockSource::Plan { id: id.into(), text: "Plan".into(), session_name: None, questions: 0, created_at: at }
    }

    /// The first frame seeds the memory (what already waits stays
    /// collapsed); an ask that arrives later opens its sheet, once.
    #[test]
    fn auto_open_skips_what_waited_and_opens_what_arrives() {
        let mut state = settled();
        state.seen.insert("p1".into());
        assert!(!state.auto_open(&dock_rows(&[plan("p1", 1)]), true, true));
        assert_eq!(state.open_id(), None);
        assert!(state.auto_open(&dock_rows(&[plan("p1", 1), plan("p2", 2)]), true, true));
        assert_eq!(state.open_id(), Some("p2"));
        state.close();
        assert_eq!(state.open_id(), None);
        assert_eq!(state.sheet_id(), Some("p2"), "the sheet slides out before it goes");
        assert!(!state.auto_open(&dock_rows(&[plan("p1", 1), plan("p2", 2)]), true, true));
    }

    /// A card that arrives while the view is not in front stays collapsed.
    #[test]
    fn auto_open_waits_for_the_view_in_front() {
        let mut state = settled();
        assert!(!state.auto_open(&dock_rows(&[plan("p1", 1)]), false, true));
        assert!(!state.auto_open(&dock_rows(&[plan("p1", 1)]), true, true));
        assert_eq!(state.open_id(), None);
    }

    /// Nothing counts as arriving before the host reads as loaded, nor
    /// during the settle window after it.
    #[test]
    fn auto_open_waits_for_the_host_to_load_and_settle() {
        let mut state = DockState::default();
        assert!(!state.auto_open(&dock_rows(&[plan("p1", 1)]), true, false));
        assert!(!state.auto_open(&dock_rows(&[plan("p1", 1), plan("p2", 2)]), true, true));
        assert_eq!(state.open_id(), None);
    }

    /// Release train 2026-10-23 (N-12 F2): a sheet whose card vanished is
    /// forgotten, so the same fixed id returning stays collapsed.
    #[test]
    fn a_vanished_open_card_is_forgotten() {
        let mut state = settled();
        state.open("p1");
        let rows = dock_rows(&[plan("p1", 1)]);
        let view = dock(&rows, false, state.open_id());
        assert!(!state.forget_vanished(view.open_id.as_deref()));
        assert_eq!(state.open_id(), Some("p1"));
        let view = dock(&[], false, state.open_id());
        assert!(state.forget_vanished(view.open_id.as_deref()));
        assert_eq!(state.open_id(), None);
        let view = dock(&rows, false, state.open_id());
        assert_eq!(view.open_id, None);
    }

    /// A state whose memory has settled on nothing.
    fn settled() -> DockState {
        DockState { primed: Some(Instant::now() - SETTLE), ..DockState::default() }
    }
}
