//! #EXP-1837 THE ASSISTANT FAB (agent-settings.json `assistant.fab`, web
//! `components/team/assistant-fab.tsx`): on the IDE the viewer's Assistant
//! lives in a 52px glass circle bottom-right of the content card (`inset` in),
//! the brand mark as its glyph and THE count badge `row_badge` of the
//! viewer's conversation with it (what its sidebar Agents row carried). A
//! press opens the widget: a `widget.width`×`widget.height` popover anchored
//! to the FAB (side top, align end) holding the REAL chat (the Chat tab's
//! body: the root's transcript + THE composer, no tabs), under a header of
//! ⇱ maximize (the Assistant's chat page), the mark + its name, and —
//! minimize. Esc and an outside click close it; a navigation does too.
//! Hidden on the Assistant's own page and while there is no Assistant
//! (`assistant_fab_visible`). While it shows, the toast stack rises by
//! `toastLift` (the Shell sets it).
//!
//! #EXP-1860 NOTHING HIDES UNDER IT (`assistant.fab.clearance`, web
//! `.fab-clearance` / `.fab-lane` in @exp/ui styles.css): the FAB records
//! its circle per window ([`FabCorners`]); a page's scroll content ends
//! with [`page_end`] (a spacer the corner tall while the FAB shows) and THE
//! Dock band keeps its right edge [`lane`] in where its column runs into the
//! circle, so the composer's send button sits beside the FAB.
//! #EXP-1872: every other control row pinned to a pane's foot (the plan
//! rail's Back / Next / decision, a side pane's footer) is THE [`foot`]: it
//! measures itself and takes the same lane wherever it runs into the circle.

use std::collections::HashMap;

use gpui::{
    canvas, div, px, App, AppContext as _, Bounds, Entity, Global, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, WindowId,
};
use gpui_component::{h_flex, popover::Popover, v_flex, ActiveTheme as _, Icon, Selectable, Sizable as _};
use sync::Store;

use domain::agent_settings::{assistant_fab_visible, ASSISTANT_FAB};
use domain::nav_badges::CountBadgeValue;

use crate::icons::{registry, ExpIcon};
use crate::navigation::{navigate, resolved_screen, Navigation, Screen};

/// THE FAB circle (web `FAB_CIRCLE_CLASS` `size-[52px]`) and its glyph (`size-5`).
const FAB_PX: f32 = 52.;
const FAB_GLYPH_PX: f32 = 20.;
/// The widget's gap above the FAB (web `sideOffset={8}`).
const WIDGET_OFFSET: f32 = 8.;
/// The widget header's height (web `h-11`) and its mark (`size-4`).
const HEADER_H: f32 = 44.;
const HEADER_MARK_PX: f32 = 16.;

pub(crate) const TEST_IDS: (&str, &str, &str) = ("assistant-fab", "assistant-widget-maximize", "assistant-widget-minimize");
/// The open widget's body (the live tests read whether it paints).
pub(crate) const WIDGET_TEST_ID: &str = "assistant-widget";

/// #EXP-1860: each window's FAB circle in window coordinates while it
/// shows (`None` = no FAB on that window's screen).
#[derive(Default)]
struct FabCorners {
    by_window: HashMap<WindowId, Bounds<Pixels>>,
    /// #EXP-1872: each [`foot`]'s box as last painted, by its key.
    feet: HashMap<(WindowId, SharedString), Bounds<Pixels>>,
}

impl Global for FabCorners {}

fn corner(window: &Window, cx: &App) -> Option<Bounds<Pixels>> {
    cx.try_global::<FabCorners>()
        .and_then(|corners| corners.by_window.get(&window.window_handle().window_id()).copied())
}

/// Record (or clear) the window's circle; a change repaints the next frame
/// so the cached dock area reads it.
fn set_corner(window: &mut Window, circle: Option<Bounds<Pixels>>, cx: &mut App) {
    let id = window.window_handle().window_id();
    let corners = cx.default_global::<FabCorners>();
    let changed = match circle {
        Some(circle) => corners.by_window.insert(id, circle) != Some(circle),
        None => corners.by_window.remove(&id).is_some(),
    };
    if changed {
        window.on_next_frame(|window, _| window.refresh());
    }
}

/// The Shell's half: no FAB on this window (signed out, onboarding).
pub(crate) fn clear_corner(window: &mut Window, cx: &mut App) {
    set_corner(window, None, cx);
}

/// #EXP-1860: the end of a page's scroll content: a spacer the FAB's corner
/// tall (`clearance`) while the FAB shows, so the last row scrolls clear of
/// the circle; nothing otherwise. Append it as the scroll content's last
/// child.
pub(crate) fn page_end(window: &Window, cx: &App) -> gpui::Div {
    div().flex_shrink_0().w_full().h(page_end_height(window, cx))
}

/// [`page_end`]'s height, for a virtual list's trailing spacer row.
pub(crate) fn page_end_height(window: &Window, cx: &App) -> Pixels {
    px(if corner(window, cx).is_some() { ASSISTANT_FAB.clearance } else { 0. })
}

/// #EXP-1860: THE Dock band's right padding (and #EXP-1872 every [`foot`]'s):
/// where the band's column runs into the circle (a page's chat, a pane
/// pinned to the card's corner), its content ends `inset` px left of the
/// circle (= `clearance` px in from the card's edge); else `gutter` (the
/// widget's own chat above the FAB, a pane beside it).
pub(crate) fn lane(column: Option<Bounds<Pixels>>, gutter: Pixels, window: &Window, cx: &App) -> Pixels {
    let inset = px(ASSISTANT_FAB.inset);
    match (corner(window, cx), column) {
        (Some(circle), Some(column))
            if column.right() > circle.left() - inset && column.bottom() > circle.top() =>
        {
            gutter.max(column.right() - circle.left() + inset)
        }
        _ => gutter,
    }
}

/// #EXP-1872 THE pinned foot: the row of controls a pane pins under its
/// scroll (Back / Next, a decision, a footer's buttons, an answer field).
/// Put the foot's content in it: it records its own box (`key` = unique
/// per window) and keeps its content beside the Assistant FAB wherever it
/// runs into the circle ([`lane`]); anywhere else it is a plain full-width
/// box. A pane never pins a control at its foot without it.
pub(crate) fn foot(key: impl Into<SharedString>, window: &Window, cx: &App) -> gpui::Div {
    let key = (window.window_handle().window_id(), key.into());
    let measured = cx.try_global::<FabCorners>().and_then(|corners| corners.feet.get(&key).copied());
    let probe = canvas(
        move |bounds, window, cx| {
            let corners = cx.default_global::<FabCorners>();
            if corners.feet.insert(key.clone(), bounds) != Some(bounds) {
                window.on_next_frame(|window, _| window.refresh());
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full();
    div().relative().flex_shrink_0().w_full().min_w_0().pr(lane(measured, px(0.), window, cx)).child(probe)
}

/// What the FAB derives from the store: the viewer's Assistant in the
/// active team and its conversation's badge.
#[derive(Clone, Debug, PartialEq)]
struct Derived {
    assistant_id: String,
    name: String,
    badge: CountBadgeValue,
}

pub(crate) struct AssistantFab {
    nav: Entity<Navigation>,
    derived: Option<Derived>,
    open: bool,
    /// The widget's chat, built on first open, keyed by the agent id.
    chat: Option<(String, Entity<crate::agent_screen::AgentScreenView>)>,
    /// The screen the widget opened over: any navigation closes it.
    opened_on: Option<Screen>,
    /// DEV-ONLY: `EXP_DEV_ASSISTANT_OPEN=1` opens the widget once the
    /// Assistant resolved (the desktop capture of the open widget).
    dev_open: bool,
    refresh_queued: bool,
    _subscriptions: Vec<Subscription>,
}

impl AssistantFab {
    pub(crate) fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = vec![cx.observe(&nav, |this, _, cx| this.queue_refresh(cx))];
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe(&collections.agents, |this, _, cx| this.queue_refresh(cx)));
            subscriptions.push(cx.observe(&collections.sessions, |this, _, cx| this.queue_refresh(cx)));
            subscriptions.push(cx.observe(&collections.messages, |this, _, cx| this.queue_refresh(cx)));
            subscriptions.push(cx.observe(&collections.session_reads, |this, _, cx| this.queue_refresh(cx)));
            subscriptions.push(cx.observe(&collections.teams, |this, _, cx| this.queue_refresh(cx)));
        }
        let mut this = Self {
            nav,
            derived: None,
            open: false,
            chat: None,
            opened_on: None,
            dev_open: std::env::var("EXP_DEV_ASSISTANT_OPEN").is_ok(),
            refresh_queued: false,
            _subscriptions: subscriptions,
        };
        this.refresh(cx);
        this
    }

    fn queue_refresh(&mut self, cx: &mut gpui::Context<Self>) {
        if self.refresh_queued {
            return;
        }
        self.refresh_queued = true;
        let this = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = this.update(cx, |this, cx| {
                this.refresh_queued = false;
                this.refresh(cx);
            });
        });
    }

    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let derived = crate::navigation::active_team_id(&self.nav, cx)
            .and_then(|team| crate::session_data::my_assistant(&team, cx))
            .map(|assistant| {
                let badge = crate::session_data::my_conversation(&assistant.id, cx)
                    .map(|conversation| crate::session_data::conversation_badge(&conversation, cx))
                    .unwrap_or(domain::nav_badges::row_badge(0, false));
                Derived { name: crate::agent_screen::agent_def_name(&assistant), assistant_id: assistant.id, badge }
            });
        // A navigation closes the widget (maximize, a cross-session link).
        let screen = resolved_screen(&self.nav, cx);
        if self.open && self.opened_on != screen {
            self.open = false;
        }
        if self.dev_open && derived.is_some() && screen.is_some() {
            self.dev_open = false;
            self.open = true;
            self.opened_on = screen;
        }
        if derived != self.derived {
            self.derived = derived;
        }
        cx.notify();
    }

    /// Whether the FAB shows on the window's current screen.
    pub(crate) fn shows(&self, cx: &App) -> bool {
        let active_agent = match resolved_screen(&self.nav, cx) {
            Some(Screen::Agent { agent_id }) => Some(crate::agent_screen::resolve_agent_id(&agent_id, cx)),
            // The dev-only specimen screens are catalogue pages (web draws
            // them outside the team shell): no FAB over a specimen.
            Some(Screen::ExponentialUiSpecimen { .. } | Screen::Specimen { .. }) => return false,
            _ => None,
        };
        assistant_fab_visible(true, self.derived.as_ref().map(|d| d.assistant_id.as_str()), active_agent.as_deref())
    }

    fn set_open(&mut self, open: bool, cx: &mut gpui::Context<Self>) {
        if self.open != open {
            self.open = open;
            self.opened_on = resolved_screen(&self.nav, cx);
            cx.notify();
        }
    }

    fn chat_for(&mut self, agent_id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) -> Entity<crate::agent_screen::AgentScreenView> {
        if let Some((id, view)) = &self.chat {
            if id == agent_id {
                return view.clone();
            }
        }
        let id = agent_id.to_string();
        let view = cx.new(|cx| crate::agent_screen::AgentScreenView::for_widget(id, window, cx));
        self.chat = Some((agent_id.to_string(), view.clone()));
        view
    }
}

/// THE FAB circle: the card hairline, the popover fill at 85%, fully round,
/// the white brand mark, THE count badge on its corner (web `FabButton
/// emphasis="primary"`, genUI Button variant `fab`).
#[derive(IntoElement)]
struct FabCircle {
    selected: bool,
    badge: CountBadgeValue,
}

impl Selectable for FabCircle {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for FabCircle {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pressed = theme::tokens::glass::FILL_ACTIVE.to_hsla();
        div()
            .id(TEST_IDS.0)
            .debug_selector(|| TEST_IDS.0.into())
            .relative()
            .size(px(FAB_PX))
            .flex_shrink_0()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().popover.opacity(0.85))
            .border_1()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .cursor_pointer()
            .active(move |style| style.bg(pressed))
            .child(Icon::from(ExpIcon::Logo).with_size(px(FAB_GLYPH_PX)).text_color(cx.theme().foreground))
            .children(crate::sidebar::nav_count(self.badge, cx).map(|badge| {
                div().absolute().top(px(-2.)).right(px(-2.)).child(badge)
            }))
    }
}

impl Render for AssistantFab {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AssistantFab").entered();
        let Some(derived) = self.derived.clone().filter(|_| self.shows(cx)) else {
            self.open = false;
            set_corner(window, None, cx);
            return div().into_any_element();
        };
        let open = self.open;
        let chat = open.then(|| self.chat_for(&derived.assistant_id, window, cx));
        let fab = cx.entity().downgrade();
        let copy = &ASSISTANT_FAB.copy;
        let name: SharedString = derived.name.clone().into();
        let assistant_id = derived.assistant_id.clone();
        let header_fab = fab.clone();
        let content = move |_: &mut gpui_component::popover::PopoverState, _: &mut Window, cx: &mut gpui::Context<gpui_component::popover::PopoverState>| {
            let maximize_fab = header_fab.clone();
            let minimize_fab = header_fab.clone();
            let agent_id = assistant_id.clone();
            let maximize = crate::controls::ghost_icon_button(TEST_IDS.1, Icon::new(registry::UI_MAXIMIZE), cx)
                .debug_selector(|| TEST_IDS.1.into())
                .tooltip(copy.maximize.clone())
                .on_click(move |_, window, cx| {
                    let _ = maximize_fab.update(cx, |fab, cx| fab.set_open(false, cx));
                    navigate(window, cx, Screen::Agent { agent_id: agent_id.clone() });
                });
            let minimize = crate::controls::ghost_icon_button(TEST_IDS.2, Icon::new(registry::UI_MINIMIZE), cx)
                .debug_selector(|| TEST_IDS.2.into())
                .tooltip(copy.minimize.clone())
                .on_click(move |_, _, cx| {
                    let _ = minimize_fab.update(cx, |fab, cx| fab.set_open(false, cx));
                });
            let header = h_flex()
                .flex_shrink_0()
                .h(px(HEADER_H))
                .px_2()
                .gap_2()
                .items_center()
                .border_b_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .child(maximize)
                .child(Icon::from(ExpIcon::Logo).with_size(px(HEADER_MARK_PX)).text_color(cx.theme().foreground))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(name.clone()),
                )
                .child(minimize);
            v_flex()
                .debug_selector(|| WIDGET_TEST_ID.into())
                .size_full()
                .min_h_0()
                .overflow_hidden()
                .child(header)
                .child(div().flex_1().min_h_0().children(chat.clone()))
        };
        let widget = &ASSISTANT_FAB.widget;
        div()
            .absolute()
            .right(px(ASSISTANT_FAB.inset))
            .bottom(px(ASSISTANT_FAB.inset))
            .child(
                Popover::new("assistant-widget")
                    .open(open)
                    .anchor(gpui::Anchor::BottomRight)
                    .offset(px(WIDGET_OFFSET))
                    .w(px(widget.width))
                    .h(px(widget.height))
                    .p_0()
                    .overflow_hidden()
                    .trigger(FabCircle { selected: open, badge: derived.badge })
                    .on_open_change(move |open, _, cx| {
                        let open = *open;
                        let _ = fab.update(cx, |fab, cx| fab.set_open(open, cx));
                    })
                    .content(content),
            )
            // #EXP-1860: the circle's window bounds (`page_end`, `lane`).
            .child(
                canvas(|bounds, window, cx| set_corner(window, Some(bounds), cx), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(FAB_PX)),
            )
            .into_any_element()
    }
}
