//! #EXP-1837 THE ASSISTANT FAB (agent-settings.json `assistant.fab`, web
//! `components/team/assistant-fab.tsx`): on the IDE the viewer's Assistant
//! lives in a circle bottom-right of the content card (`inset` in),
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
//!
//! #EXP-1876: the circle is `size` (60) px and IS the brand mark (the disc
//! fills it, no padding, no glass chrome). Its first show on a device plays
//! THE entrance (`entrance`); THE speech bubble (`bubble`,
//! [`assistant_fab_bubble`]) pops above it: the welcome once per device, a
//! waiting plan / question once per card. What this device remembers lives
//! in `settings.json` `assistantFab` per member.

use std::collections::HashMap;
use std::time::Duration;

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    canvas, div, prelude::FluentBuilder as _, px, Animation, AnimationExt as _, App, AppContext as _, BoxShadow, Bounds,
    CursorStyle, Entity, Global, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, Render, RenderOnce, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, Window, WindowId,
};
use gpui_component::{h_flex, popover::Popover, v_flex, ActiveTheme as _, Icon, Selectable, Sizable as _};
use sync::Store;

use domain::agent_settings::{
    assistant_fab_bubble, assistant_fab_visible, assistant_widget_size, AssistantWidgetSize, FabBubble, FabWaitingCard,
    ASSISTANT_FAB,
};
use domain::nav_badges::CountBadgeValue;

use crate::icons::{registry, ExpIcon};
use crate::navigation::{navigate, resolved_screen, Navigation, Screen};

/// #EXP-1876: the `settings.json` key of what this device remembers about
/// the FAB, per member ([`FabMemory`]).
const MEMORY_KEY: &str = "assistantFab";
/// The bubbled card ids kept per member (web: the newest 100).
const BUBBLED_KEPT: usize = 100;
/// The bubble's element id (the live tests read it).
pub(crate) const BUBBLE_TEST_ID: &str = "assistant-fab-bubble";
/// The kicker glyph (web `size-3.5`).
const KICKER_GLYPH_PX: f32 = 14.;
/// The bubble card's horizontal padding + hairline (`message_bubble`: 14 px
/// across, a 1 px border), added to the measured text.
const BUBBLE_CHROME_X: f32 = 14. * 2. + 2.;
/// The card kinds that make a waiting card (web `WAITING_CARD_KINDS`).
const WAITING_CARD_KINDS: [&str; 3] = ["plan", "question", "ask"];

fn fab_px() -> f32 {
    ASSISTANT_FAB.size
}
/// The widget's gap above the FAB (web `sideOffset={8}`).
const WIDGET_OFFSET: f32 = 8.;
/// The widget header's height (web `h-11`) and its mark (`size-4`).
const HEADER_H: f32 = 44.;
const HEADER_MARK_PX: f32 = 16.;

pub(crate) const TEST_IDS: (&str, &str, &str) = ("assistant-fab", "assistant-widget-maximize", "assistant-widget-minimize");
/// The open widget's body (the live tests read whether it paints).
pub(crate) const WIDGET_TEST_ID: &str = "assistant-widget";
/// #EXP-1876: the widget's header (a double-click resets the size) and its
/// three invisible resize bands (web `ASSISTANT_FAB_TEST_IDS`).
pub(crate) const HEADER_TEST_ID: &str = "assistant-widget-header";
pub(crate) const RESIZE_TEST_IDS: (&str, &str, &str) =
    ("assistant-widget-resize-top", "assistant-widget-resize-left", "assistant-widget-resize-corner");

/// #EXP-1876 THE CONTENT CARD's own ground for the widget (web
/// `bg-glass-card-ground`): the page gradient sampled at the widget's own
/// window rows (`top` / `bottom` as fractions of the window height) under the
/// card's panel fill — never the menu surface (`theme.popover`), which is
/// the `in` bubbles' and the composer card's fill and melted into it.
/// Opaque: what scrolls behind the widget never shows.
pub(crate) fn card_ground(top: f32, bottom: f32) -> gpui::Background {
    let panel: gpui::Rgba = theme::tokens::glass::FILL_PANEL.to_hsla().into();
    let at = |t: f32| -> gpui::Hsla {
        let ground: gpui::Rgba = theme::background_gradient_color_at(t).into();
        let k = panel.a;
        gpui::Rgba {
            r: ground.r + (panel.r - ground.r) * k,
            g: ground.g + (panel.g - ground.g) * k,
            b: ground.b + (panel.b - ground.b) * k,
            a: 1.,
        }
        .into()
    };
    gpui::linear_gradient(180., gpui::linear_color_stop(at(top), 0.), gpui::linear_color_stop(at(bottom), 1.))
}

/// #EXP-1876: the widget's float — the card hairline (`strokeCard`) as a
/// crisp 1px ring and the floating surfaces' drop (web
/// `0 12px 32px -12px oklch(0 0 0 / .45)`; gpui's blur is the CSS one
/// halved).
fn widget_shadow() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(0.), theme::tokens::glass::STROKE_CARD.to_hsla()).blur_radius(px(0.)).spread_radius(px(1.)),
        BoxShadow::new(px(0.), px(12.), gpui::hsla(0., 0., 0., 0.45)).blur_radius(px(16.)).spread_radius(px(-12.)),
    ]
}

/// #EXP-1876: a live drag of one of the widget's edges. Anchored bottom-
/// right, so dragging left / up grows it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct WidgetDrag {
    x: f32,
    y: f32,
    start: AssistantWidgetSize,
    width: bool,
    height: bool,
}

/// The ui-prefs key of the member's dragged size.
fn size_key(member: Option<&str>) -> String {
    member.unwrap_or("anon").to_string()
}

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
/// box with `gutter` right padding. A pane never pins a control at its foot
/// without it.
pub(crate) fn foot(key: impl Into<SharedString>, gutter: Pixels, window: &Window, cx: &App) -> gpui::Div {
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
    div().relative().flex_shrink_0().w_full().min_w_0().pr(lane(measured, gutter, window, cx)).child(probe)
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
    /// #EXP-1876: what this device remembers for the member it loaded for.
    memory: Option<(String, FabMemory)>,
    /// #EXP-1876 THE entrance's phase on this window.
    entrance: Entrance,
    /// The window has focus (a needs-you bubble waits for it).
    focused: bool,
    activation: Option<Subscription>,
    /// #EXP-1876 THE speech bubble showing now.
    bubble: Option<SpeechBubble>,
    hovered: bool,
    /// Bumped by every (re)arm or pause of the stay timer: a stale timer
    /// leaves the bubble alone.
    stay_gen: u64,
    _stay: Option<Task<()>>,
    _entrance_done: Option<Task<()>>,
    /// The bubble's painted height (the toast lift grows by it).
    bubble_height: f32,
    /// #EXP-1876: the content card's size as last painted (the widget's
    /// size follows it, re-clamped on every resize).
    card: Rc<Cell<Option<AssistantWidgetSize>>>,
    /// The member's dragged size on this device (ui-prefs.json), read back
    /// through the clamp; `None` = the responsive default.
    stored: Option<AssistantWidgetSize>,
    drag: Option<WidgetDrag>,
    _subscriptions: Vec<Subscription>,
}

/// #EXP-1876: what this device remembers about the FAB for one member
/// (web localStorage `exp.assistantFab.{entered,welcomed,bubbled}.<me>`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct FabMemory {
    #[serde(default)]
    entered: bool,
    #[serde(default)]
    welcomed: bool,
    #[serde(default)]
    bubbled: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Entrance {
    /// Not decided yet (no member loaded).
    Unknown,
    /// The first show on this device: play it on the next paint.
    Pending,
    /// Playing (the circle wears the animation).
    Playing,
    Done,
}

/// The bubble's content (web `FabBubbleView`).
#[derive(Clone, Debug, PartialEq)]
struct SpeechBubble {
    /// `welcome`, else the waiting card's message id (the pop-in replays per key).
    key: SharedString,
    /// The waiting card's message id (`None` = the welcome).
    message_id: Option<String>,
    /// `Plan ready` / `Question` (`None` = the welcome).
    kicker: Option<String>,
    text: String,
    stay: Duration,
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
            memory: None,
            entrance: Entrance::Unknown,
            focused: true,
            activation: None,
            bubble: None,
            hovered: false,
            stay_gen: 0,
            _stay: None,
            _entrance_done: None,
            bubble_height: 0.,
            card: Rc::new(Cell::new(None)),
            stored: crate::ui_prefs::assistant_widget_size(&size_key(crate::session_data::signed_in_user(cx).as_deref()))
                .map(|(width, height)| AssistantWidgetSize { width, height }),
            drag: None,
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
        // #EXP-1987: the member's Assistant whatever team is current (the
        // team's own row before the move, the global one after it).
        let team = crate::navigation::active_team_id(&self.nav, cx);
        let derived = crate::session_data::my_assistant_anywhere(team.as_deref(), cx)
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
        self.load_memory(cx);
        self.decide_bubble(cx);
        cx.notify();
    }

    /// #EXP-1876: load what this device remembers for the signed-in member
    /// (once per member).
    fn load_memory(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(me) = crate::queries::active_account(cx).map(|account| account.user_id) else {
            return;
        };
        if self.memory.as_ref().is_some_and(|(member, _)| *member == me) {
            return;
        }
        let memory: FabMemory = crate::navigation::load_member_setting(cx, MEMORY_KEY, &me)
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        self.entrance = if memory.entered { Entrance::Done } else { Entrance::Pending };
        self.memory = Some((me, memory));
    }

    fn remember(&mut self, cx: &mut gpui::Context<Self>, change: impl FnOnce(&mut FabMemory)) {
        let Some((me, memory)) = self.memory.as_mut() else {
            return;
        };
        let before = memory.clone();
        change(memory);
        if *memory != before {
            let value = serde_json::to_value(&*memory).unwrap_or_default();
            crate::navigation::persist_member_setting(cx, MEMORY_KEY, me.clone(), value);
        }
    }

    /// #EXP-1876 THE speech bubble's decision (web `useFabSpeech`): the
    /// pure [`assistant_fab_bubble`] over the conversation tree's waiting
    /// cards, then the burst rule (the shown card and every older one are
    /// bubbled once); a shown needs-you bubble leaves once its card is
    /// answered or read, every bubble while the widget is open.
    fn decide_bubble(&mut self, cx: &mut gpui::Context<Self>) {
        let Some((_, memory)) = self.memory.as_ref() else {
            return;
        };
        // Nothing is decided before the sessions (the waiting cards ride
        // their digests) and the read marks caught up: an empty snapshot
        // would spend the welcome on a frame a needs-you card wins a moment
        // later.
        if !bubble_inputs_ready(cx) {
            return;
        }
        let shows = self.derived.is_some() && self.shows(cx);
        let messages = match &self.derived {
            Some(derived) => waiting_cards(&derived.assistant_id, cx),
            None => Vec::new(),
        };
        let waiting_ids: Vec<&str> = messages.iter().map(|(message, _)| message.id.as_str()).collect();
        if self.open || !shows {
            self.bubble = None;
        } else if let Some(SpeechBubble { message_id: Some(id), .. }) = &self.bubble {
            if !waiting_ids.contains(&id.as_str()) {
                self.bubble = None;
            }
        }
        let cards: Vec<FabWaitingCard<'_>> = messages
            .iter()
            .map(|(message, _)| FabWaitingCard {
                message_id: &message.id,
                session_id: &message.session_id,
                created_at: message.created_at.as_deref().unwrap_or_default(),
            })
            .collect();
        let read_rows = session_reads(&messages, cx);
        let reads: Vec<(&str, &str)> = read_rows.iter().map(|(session, at)| (session.as_str(), at.as_str())).collect();
        let bubbled: Vec<&str> = memory.bubbled.iter().map(String::as_str).collect();
        let decision = assistant_fab_bubble(memory.welcomed, &cards, &reads, &bubbled, self.open, !shows, self.focused);
        let Some(decision) = decision else {
            return;
        };
        let spec = &ASSISTANT_FAB.bubble;
        let (bubble, newly_bubbled) = match decision {
            FabBubble::Welcome => {
                let lines = &spec.welcome;
                let pick = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|now| now.subsec_nanos() as usize)
                    .unwrap_or_default()
                    % lines.len().max(1);
                let text = lines.get(pick).map(|line| line.en.clone()).unwrap_or_default();
                let bubble = SpeechBubble {
                    key: "welcome".into(),
                    message_id: None,
                    kicker: None,
                    text,
                    stay: Duration::from_millis(spec.stay_ms.welcome),
                };
                (bubble, Vec::new())
            }
            FabBubble::NeedsYou { message_id } => {
                let Some((message, _)) = messages.iter().find(|(message, _)| message.id == message_id) else {
                    return;
                };
                let at = |iso: &str| domain::session_tree::iso_ms(iso).unwrap_or(0);
                let shown_at = at(message.created_at.as_deref().unwrap_or_default());
                let older: Vec<String> = cards
                    .iter()
                    .filter(|card| at(card.created_at) <= shown_at)
                    .map(|card| card.message_id.to_string())
                    .collect();
                let (plan, text) = waiting_card_view(message);
                let bubble = SpeechBubble {
                    key: message_id.clone().into(),
                    message_id: Some(message_id),
                    kicker: Some(if plan { spec.copy.plan.clone() } else { spec.copy.question.clone() }),
                    text,
                    stay: Duration::from_millis(spec.stay_ms.needs_you),
                };
                (bubble, older)
            }
        };
        // Any bubble shown spends the welcome; a burst bubbles once.
        self.remember(cx, |memory| {
            memory.welcomed = true;
            for id in newly_bubbled {
                if !memory.bubbled.contains(&id) {
                    memory.bubbled.push(id);
                }
            }
            let over = memory.bubbled.len().saturating_sub(BUBBLED_KEPT);
            memory.bubbled.drain(..over);
        });
        self.bubble = Some(bubble);
        self.hovered = false;
        self.arm_stay(cx);
    }

    /// The bubble stays `stayMs`; a hover pauses it, leaving re-arms the full time.
    fn arm_stay(&mut self, cx: &mut gpui::Context<Self>) {
        self.stay_gen += 1;
        let generation = self.stay_gen;
        let Some(stay) = self.bubble.as_ref().map(|bubble| bubble.stay) else {
            self._stay = None;
            return;
        };
        self._stay = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(stay).await;
            let _ = this.update(cx, |this, cx| {
                if this.stay_gen == generation && !this.hovered {
                    this.bubble = None;
                    cx.notify();
                }
            });
        }));
    }

    fn set_hovered(&mut self, hovered: bool, cx: &mut gpui::Context<Self>) {
        if self.hovered == hovered {
            return;
        }
        self.hovered = hovered;
        if hovered {
            // Paused: the armed timer goes stale.
            self.stay_gen += 1;
            self._stay = None;
        } else {
            self.arm_stay(cx);
        }
    }

    #[cfg(test)]
    pub(crate) fn set_focused_for_test(&mut self, focused: bool, cx: &mut gpui::Context<Self>) {
        self.focused = focused;
        self.refresh(cx);
    }

    /// The welcome unseen again, nothing showing.
    #[cfg(test)]
    pub(crate) fn forget_bubble_for_test(&mut self, cx: &mut gpui::Context<Self>) {
        self.bubble = None;
        if let Some((_, memory)) = self.memory.as_mut() {
            memory.welcomed = false;
        }
        self.refresh(cx);
    }

    #[cfg(test)]
    pub(crate) fn set_hovered_for_test(&mut self, hovered: bool, cx: &mut gpui::Context<Self>) {
        self.set_hovered(hovered, cx);
        cx.notify();
    }

    /// #EXP-1876: how far the bottom-right toasts rise while the FAB shows:
    /// `toastLift`, plus the bubble's height and gap while it shows.
    pub(crate) fn toast_lift(&self) -> f32 {
        let bubble = if self.bubble.is_some() && !self.open && self.bubble_height > 0. {
            self.bubble_height + ASSISTANT_FAB.bubble.gap
        } else {
            0.
        };
        ASSISTANT_FAB.toast_lift + bubble
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
            if open {
                self.bubble = None;
            }
            self.opened_on = resolved_screen(&self.nav, cx);
            cx.notify();
        }
    }

    /// #EXP-1876 `assistantWidgetSize`: the size the widget paints at in
    /// the card as last measured (the floor before the first measure).
    fn widget_size(&self) -> AssistantWidgetSize {
        let spec = &ASSISTANT_FAB.widget;
        match self.card.get() {
            Some(card) => assistant_widget_size(card, self.stored),
            None => spec.min,
        }
    }

    fn start_resize(&mut self, event: &MouseDownEvent, width: bool, height: bool, cx: &mut gpui::Context<Self>) {
        cx.stop_propagation();
        self.drag = Some(WidgetDrag {
            x: f32::from(event.position.x),
            y: f32::from(event.position.y),
            start: self.widget_size(),
            width,
            height,
        });
        cx.notify();
    }

    fn track_resize(&mut self, x: f32, y: f32, cx: &mut gpui::Context<Self>) {
        let (Some(drag), Some(card)) = (self.drag, self.card.get()) else { return };
        let next = AssistantWidgetSize {
            width: if drag.width { drag.start.width + drag.x - x } else { drag.start.width },
            height: if drag.height { drag.start.height + drag.y - y } else { drag.start.height },
        };
        let next = assistant_widget_size(card, Some(next));
        if self.stored != Some(next) {
            self.stored = Some(next);
            cx.notify();
        }
    }

    fn end_resize(&mut self, cx: &mut gpui::Context<Self>) {
        if self.drag.take().is_some() {
            let key = size_key(crate::session_data::signed_in_user(cx).as_deref());
            crate::ui_prefs::set_assistant_widget_size(&key, self.stored.map(|s| (s.width, s.height)));
            cx.notify();
        }
    }

    /// The header's double-click: back to the responsive default.
    fn reset_size(&mut self, cx: &mut gpui::Context<Self>) {
        self.drag = None;
        self.stored = None;
        let key = size_key(crate::session_data::signed_in_user(cx).as_deref());
        crate::ui_prefs::set_assistant_widget_size(&key, None);
        cx.notify();
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

/// THE FAB circle: #EXP-1876 the `size` px brand mark IS the button (the
/// disc fills it, no padding, no glass chrome), THE count badge on its
/// corner (web `FabButton`, genUI Button variant `fab`). `entrance` =
/// wear THE entrance this paint.
#[derive(IntoElement)]
struct FabCircle {
    selected: bool,
    badge: CountBadgeValue,
    entrance: bool,
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
        let look = fab_look(cx);
        let mark = if self.entrance { entrance(look.clone()).into_any_element() } else { fab_mark(&look, 1.).into_any_element() };
        // #EXP-1876 look (b), Danny's pick: the mark carries the genUI
        // Button `fab` `lg` look (`fab_mark`), so the entrance scales it whole.
        let circle = div()
            .id(TEST_IDS.0)
            .debug_selector(|| TEST_IDS.0.into())
            .relative()
            .size(px(fab_px()))
            .flex_shrink_0()
            .rounded_full();
        circle
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .active(|style| style.opacity(0.85))
            .child(mark)
            .children(crate::sidebar::nav_count(self.badge, cx).map(|badge| {
                let badge = div().absolute().top(px(-2.)).right(px(-2.)).child(badge);
                // The badge rides the entrance's fade (web: it sits inside
                // the animated button), never ahead of the mark.
                if self.entrance { entrance_fade(badge).into_any_element() } else { badge.into_any_element() }
            }))
    }
}

/// #EXP-1876 THE mark button's look: the genUI Button `variant: fab`,
/// `size: lg` recipe of the built-in exponential theme in the app's mode
/// (web `.assistant-fab-mark`, the catalogue's specimen): the circle's
/// ground (under the mark's gaps), its shadow, the mark's flat ink and its
/// sheen (`backgroundClip: text` + `backgroundGradient` on the icon part).
#[derive(Clone)]
struct FabLook {
    ground: Option<gpui::Hsla>,
    shadows: Vec<gpui::BoxShadow>,
    ink: gpui::Hsla,
    sheen: Option<eui_gpui::paint::GradientPaint>,
}

fn fab_look(cx: &App) -> FabLook {
    fab_look_in(if cx.theme().is_dark() { eui_core::theme::Mode::Dark } else { eui_core::theme::Mode::Light }, cx.theme().foreground)
}

fn fab_look_in(mode: eui_core::theme::Mode, foreground: gpui::Hsla) -> FabLook {
    let theme = eui_core::themes::builtin_theme("exponential");
    let props = serde_json::Map::from_iter([("variant".to_string(), serde_json::json!("fab")), ("size".to_string(), serde_json::json!("lg"))]);
    let part = |name: &str| eui_gpui::paint::parts::part_props(theme.as_deref(), mode, "Button", name, &props, &[]);
    let style = |props: &serde_json::Map<String, serde_json::Value>| eui_gpui::paint::PaintStyle::from_visual(&eui_core::style::visual(props, eui_core::style::BoxKind::Leaf));
    let root = style(&part("root"));
    let icon_props = part("icon");
    let sheen = (icon_props.get("backgroundClip").and_then(serde_json::Value::as_str) == Some("text")).then(|| style(&icon_props).gradient).flatten();
    FabLook { ground: root.bg, shadows: root.shadows, ink: root.color.unwrap_or(foreground), sheen }
}

/// The brand mark filling the circle, at `scale` of it (centred): the
/// look's ground disc under its gaps, lifted by the look's shadow, the mark
/// wearing the sheen (else its flat ink).
fn fab_mark(look: &FabLook, scale: f32) -> gpui::Div {
    let size = fab_px();
    let glyph = size * scale;
    let paint = |ink: gpui::Hsla| Icon::from(ExpIcon::Logo).with_size(px(glyph)).text_color(ink).into_any_element();
    let mut disc = div().size(px(glyph)).flex_none().rounded_full().shadow(look.shadows.clone());
    if let Some(ground) = look.ground {
        disc = disc.bg(ground);
    }
    div()
        .absolute()
        .top_0()
        .left_0()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .child(disc.child(match &look.sheen {
            Some(sheen) => eui_gpui::paint::icons::sheen_bands(glyph, sheen, paint),
            None => paint(look.ink),
        }))
}

/// A piecewise scale over `stops` at `times` (0..1), each leg eased.
fn entrance_scale(t: f32, stops: [f32; 3], times: [f32; 3]) -> f32 {
    let ease = theme::motion::decelerate();
    if t <= times[0] {
        return stops[0];
    }
    for leg in 0..2 {
        let (from, to) = (times[leg], times[leg + 1]);
        if t <= to {
            let span = (to - from).max(f32::EPSILON);
            return stops[leg] + (stops[leg + 1] - stops[leg]) * ease((t - from) / span);
        }
    }
    stops[2]
}

/// DEV-ONLY (the EXP_DEV_* family): `EXP_DEV_FAB_ENTRANCE_AT_MS=<ms>`
/// freezes THE entrance at that millisecond (a capture's mid-entrance
/// frame); unset = the live animation.
fn frozen_entrance_t() -> Option<f32> {
    let ms: f32 = std::env::var("EXP_DEV_FAB_ENTRANCE_AT_MS").ok()?.parse().ok()?;
    let spec = &ASSISTANT_FAB.entrance;
    let total = if theme::motion::reduce_motion() { spec.reduced_motion.fade_ms } else { spec.duration_ms };
    Some((ms / total.max(1) as f32).clamp(0., 1.))
}

/// The badge's opacity at entrance progress `t`: the mark's fade curve,
/// or the reduced-motion fade.
fn badge_fade_at(t: f32) -> f32 {
    if theme::motion::reduce_motion() {
        return t;
    }
    let lead_at = ASSISTANT_FAB.entrance.scale_times[1].max(f32::EPSILON);
    theme::motion::decelerate()((t / lead_at).min(1.))
}

/// The entrance's fade alone (the badge's share of it): the mark's opacity
/// curve, or the reduced-motion fade.
fn entrance_fade(element: gpui::Div) -> impl IntoElement {
    let spec = &ASSISTANT_FAB.entrance;
    if let Some(t) = frozen_entrance_t() {
        return element.opacity(badge_fade_at(t)).into_any_element();
    }
    let lasts = if theme::motion::reduce_motion() { spec.reduced_motion.fade_ms } else { spec.duration_ms };
    element
        .with_animation(
            "assistant-fab-entrance-badge",
            Animation::new(Duration::from_millis(lasts)),
            |badge, t| badge.opacity(badge_fade_at(t)),
        )
        .into_any_element()
}

/// #EXP-1876 THE entrance (`assistant.fab.entrance`, web `useFabEntrance`):
/// the mark rises `riseY` px and scales 0.6 → 1.06 → 1 with a fade, then
/// one soft ring ripple spreads `ripple.spreadPx` from the disc's edge.
/// Reduce motion: a `reducedMotion.fadeMs` fade only. gpui has no element
/// scale, so the mark's own size carries it.
fn entrance(look: FabLook) -> impl IntoElement {
    let spec = &ASSISTANT_FAB.entrance;
    let frozen = frozen_entrance_t();
    if theme::motion::reduce_motion() {
        if let Some(t) = frozen {
            return fab_mark(&look, 1.).opacity(t).into_any_element();
        }
        return fab_mark(&look, 1.)
            .with_animation(
                "assistant-fab-entrance",
                Animation::new(Duration::from_millis(spec.reduced_motion.fade_ms)),
                |mark, t| mark.opacity(t),
            )
            .into_any_element();
    }
    let layer = div().absolute().top_0().left_0().size(px(fab_px()));
    if let Some(t) = frozen {
        return entrance_frame(layer, &look, t).into_any_element();
    }
    layer
        .with_animation(
            "assistant-fab-entrance",
            Animation::new(Duration::from_millis(spec.duration_ms)),
            move |layer, t| entrance_frame(layer, &look, t),
        )
        .into_any_element()
}

/// THE entrance's frame at progress `t` (0..1) on `layer`.
fn entrance_frame(layer: gpui::Div, look: &FabLook, t: f32) -> gpui::Div {
    let spec = &ASSISTANT_FAB.entrance;
    let total = spec.duration_ms.max(1) as f32;
    let ripple = spec.ripple;
    let (stops, times) = (spec.scale, spec.scale_times);
    let ease = theme::motion::decelerate();
    // The rise + fade ride the first leg (to the overshoot).
    let lead = ease((t / times[1].max(f32::EPSILON)).min(1.));
    let scale = entrance_scale(t, stops, times);
    let ms = t * total;
    let ring = ((ms - ripple.start_ms as f32) / ripple.duration_ms.max(1) as f32).clamp(0., 1.);
    let ring_on = ripple.count > 0 && ms >= ripple.start_ms as f32 && ring < 1.;
    let size = fab_px();
    let spread = ripple.spread_px * ease(ring);
    layer
        .children(ring_on.then(|| {
            div()
                .absolute()
                .top(px(-spread))
                .left(px(-spread))
                .size(px(size + spread * 2.))
                .rounded_full()
                .border_2()
                .border_color(look.ink.opacity(0.45 * (1. - ring)))
        }))
        .child(fab_mark(look, scale).top(px(spec.rise_y * (1. - lead))).opacity(lead))
}

/// The Assistant conversation tree's waiting cards (web `useWaitingCards`):
/// the unanswered plan / question / ask digest messages of its sessions
/// that wait on the viewer, each with whether it is a plan.
fn waiting_cards(agent_id: &str, cx: &App) -> Vec<(domain::rows::MessageRow, bool)> {
    let Some(conversation) = crate::session_data::my_conversation(agent_id, cx) else {
        return Vec::new();
    };
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let sessions = store.collections().sessions.read(cx);
    let waiting: Vec<&domain::rows::SessionRow> = sessions
        .iter()
        .filter(|row| row.root_id_or_self() == conversation.id && row.needs_input)
        .collect();
    let digests = crate::session_data::digest_messages_of(&waiting, cx);
    digests
        .iter()
        .flat_map(|rows| rows.iter())
        .filter(|message| message.answered_at.is_none())
        .filter_map(|message| {
            let kind = message.card.as_ref()?.get("kind")?.as_str()?;
            WAITING_CARD_KINDS.contains(&kind).then(|| (message.clone(), kind == "plan"))
        })
        .collect()
}

/// The bubble's inputs synced: the sessions shape (the waiting cards ride
/// its digests) and the read marks reached their first up-to-date.
fn bubble_inputs_ready(cx: &App) -> bool {
    let Some(store) = Store::try_global(cx) else {
        return false;
    };
    store.collections().sessions.read(cx).is_ready() && store.collections().session_reads.read(cx).is_ready()
}

/// The synced read marks of the waiting cards' sessions: (session, readAt).
fn session_reads(cards: &[(domain::rows::MessageRow, bool)], cx: &App) -> Vec<(String, String)> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let reads = store.collections().session_reads.read(cx);
    reads
        .iter()
        .filter(|read| cards.iter().any(|(message, _)| message.session_id == read.session_id))
        .filter_map(|read| Some((read.session_id.clone(), read.read_at.clone()?)))
        .collect()
}

/// The bubble's kicker kind + text of a waiting card (web
/// `waitingCardView`): the chat list's preview of it without its `Plan: ` /
/// `Question: ` lead; a sealed one that does not open here has no text.
fn waiting_card_view(message: &domain::rows::MessageRow) -> (bool, String) {
    use domain::chats::{ask_preview, ChatPreviewKind, CHATS};
    let card_plan = message.card.as_ref().and_then(|card| card.get("kind")).and_then(|kind| kind.as_str()) == Some("plan");
    if message.locked().is_some() {
        return (card_plan, String::new());
    }
    let preview = ask_preview(message, None);
    let plan = preview.kind == ChatPreviewKind::Plan;
    let template = if plan { &CHATS.copy.preview.plan } else { &CHATS.copy.preview.question };
    let lead = template.split("{text}").next().unwrap_or_default();
    let text = preview.text.strip_prefix(lead).unwrap_or(&preview.text).to_string();
    (plan, text)
}

/// The bubble's width: its widest line (the text on one line, the kicker)
/// plus the card's chrome, capped at `maxWidth` (the text wraps to
/// `maxLines` there).
fn bubble_width(bubble: &SpeechBubble, window: &mut Window) -> f32 {
    let spec = &ASSISTANT_FAB.bubble;
    let font = window.text_style().font();
    let measure = |text: &str, size: f32, font: gpui::Font| -> f32 {
        if text.is_empty() {
            return 0.;
        }
        let run = gpui::TextRun { len: text.len(), font, color: gpui::black(), background_color: None, underline: None, strikethrough: None };
        f32::from(window.text_system().shape_line(SharedString::from(text.to_string()), px(size), &[run], None).width)
    };
    let text = measure(&bubble.text, theme::tokens::transcript::BODY_SIZE, font.clone());
    let kicker = bubble.kicker.as_deref().map_or(0., |kicker| {
        let bold = gpui::Font { weight: gpui::FontWeight::MEDIUM, ..font.clone() };
        KICKER_GLYPH_PX + 4. + measure(kicker, 12., bold)
    });
    (text.max(kicker) + BUBBLE_CHROME_X).ceil().min(spec.max_width)
}

/// #EXP-1876 THE SPEECH BUBBLE: THE message bubble (side in, the card
/// fill) above the FAB, right-aligned with it, popping in over `popInMs`
/// (gpui has no element scale: the fade alone, as every overlay enter); a
/// press opens the widget (the waiting card sits in its Dock), a hover
/// pauses its stay.
fn speech_bubble(bubble: &SpeechBubble, fab: gpui::WeakEntity<AssistantFab>, window: &mut Window, cx: &App) -> gpui::AnyElement {
    let spec = &ASSISTANT_FAB.bubble;
    let width = bubble_width(bubble, window);
    let yellow = theme::tokens::YELLOW.to_hsla();
    let kicker = bubble.kicker.clone().map(|kicker| {
        h_flex()
            .gap_1()
            .items_center()
            .text_xs()
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(yellow)
            .child(Icon::new(registry::NAV_NEEDS_YOU).with_size(px(KICKER_GLYPH_PX)).text_color(yellow))
            .child(SharedString::from(kicker))
    });
    let body = v_flex()
        .min_w_0()
        .children(kicker)
        .children((!bubble.text.is_empty()).then(|| div().text_ellipsis().line_clamp(spec.max_lines).child(SharedString::from(bubble.text.clone()))));
    // #EXP-1876: THE bubble's `down` tail at the bottom, pointing at the FAB.
    let card = crate::session_chat::message_bubble(
        crate::session_chat::MessageBubble {
            id: "assistant-fab-bubble-card".into(),
            body: body.into_any_element(),
            time: String::new(),
            own: false,
            name: None,
            link: None,
            status: None,
            menu: None,
            width: Some(px(width)),
            tail: crate::session_chat::BubbleTail::Down,
        },
        cx,
    );
    let hover_fab = fab.clone();
    let press_fab = fab.clone();
    let measure = canvas(
        move |bounds, _, cx| {
            let height = f32::from(bounds.size.height);
            let Some(entity) = fab.upgrade() else {
                return;
            };
            if (entity.read(cx).bubble_height - height).abs() > 0.5 {
                cx.defer(move |cx| {
                    entity.update(cx, |fab, cx| {
                        fab.bubble_height = height;
                        cx.notify();
                    })
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full();
    let name = SharedString::from(match &bubble.kicker {
        Some(kicker) if !bubble.text.is_empty() => format!("{kicker}: {}", bubble.text),
        Some(kicker) => kicker.clone(),
        None => bubble.text.clone(),
    });
    div()
        .id(BUBBLE_TEST_ID)
        .role(gpui::Role::Button)
        .aria_label(name)
        .debug_selector(|| BUBBLE_TEST_ID.into())
        .absolute()
        .right_0()
        .bottom(px(fab_px() + spec.gap))
        .w(px(width))
        .cursor_pointer()
        .on_hover(move |hovered, _, cx| {
            let hovered = *hovered;
            let _ = hover_fab.update(cx, |fab, cx| fab.set_hovered(hovered, cx));
        })
        .on_click(move |_, _, cx| {
            let _ = press_fab.update(cx, |fab, cx| fab.set_open(true, cx));
        })
        .child(card)
        .child(measure)
        .with_animation(
            bubble.key.clone(),
            Animation::new(Duration::from_millis(spec.pop_in_ms)).with_easing(theme::motion::decelerate()),
            |bubble, t| bubble.opacity(t),
        )
        .into_any_element()
}

impl Render for AssistantFab {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AssistantFab").entered();
        // #EXP-1876: a needs-you bubble waits for the window's focus.
        if self.activation.is_none() {
            self.focused = window.is_window_active();
            self.activation = Some(cx.observe_window_activation(window, |this, window, cx| {
                this.focused = window.is_window_active();
                this.queue_refresh(cx);
            }));
        }
        let Some(derived) = self.derived.clone().filter(|_| self.shows(cx)) else {
            self.open = false;
            self.bubble = None;
            set_corner(window, None, cx);
            return div().into_any_element();
        };
        // #EXP-1876 THE entrance: the first show on this device, once.
        if self.entrance == Entrance::Pending {
            self.entrance = Entrance::Playing;
            self.remember(cx, |memory| memory.entered = true);
            let spec = &ASSISTANT_FAB.entrance;
            let lasts = if theme::motion::reduce_motion() { spec.reduced_motion.fade_ms } else { spec.duration_ms };
            // A frozen capture frame (`EXP_DEV_FAB_ENTRANCE_AT_MS`) never ends.
            self._entrance_done = frozen_entrance_t().is_none().then(|| cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(lasts + 50)).await;
                let _ = this.update(cx, |this, cx| {
                    this.entrance = Entrance::Done;
                    cx.notify();
                });
            }));
        }
        let entrance = self.entrance == Entrance::Playing;
        let bubble = self
            .bubble
            .clone()
            .filter(|_| !self.open)
            .map(|bubble| speech_bubble(&bubble, cx.entity().downgrade(), window, cx));
        let open = self.open;
        let chat = open.then(|| self.chat_for(&derived.assistant_id, window, cx));
        let fab = cx.entity().downgrade();
        let copy = &ASSISTANT_FAB.copy;
        let name: SharedString = derived.name.clone().into();
        let assistant_id = derived.assistant_id.clone();
        let header_fab = fab.clone();
        let drag = self.drag;
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
            let reset_fab = header_fab.clone();
            let header = h_flex()
                .id(HEADER_TEST_ID)
                .debug_selector(|| HEADER_TEST_ID.into())
                .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
                    if event.click_count >= 2 {
                        let _ = reset_fab.update(cx, |fab, cx| fab.reset_size(cx));
                    }
                })
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
            let band = px(ASSISTANT_FAB.widget.handle);
            let edge = |id: &'static str, width: bool, height: bool, cursor: CursorStyle| {
                let fab = header_fab.clone();
                div()
                    .id(id)
                    .debug_selector(|| id.into())
                    .absolute()
                    .top_0()
                    .left_0()
                    .when(width && height, |d| d.size(band * 2.))
                    .when(width && !height, |d| d.h_full().w(band))
                    .when(height && !width, |d| d.w_full().h(band))
                    .cursor(cursor)
                    .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
                        let _ = fab.update(cx, |fab, cx| fab.start_resize(event, width, height, cx));
                    })
            };
            v_flex()
                .debug_selector(|| WIDGET_TEST_ID.into())
                .relative()
                .size_full()
                .min_h_0()
                .overflow_hidden()
                .child(header)
                .child(div().flex_1().min_h_0().children(chat.clone()))
                .child(edge(RESIZE_TEST_IDS.0, false, true, CursorStyle::ResizeUpDown))
                .child(edge(RESIZE_TEST_IDS.1, true, false, CursorStyle::ResizeLeftRight))
                .child(edge(RESIZE_TEST_IDS.2, true, true, CursorStyle::ResizeUpLeftDownRight))
                .children(drag_capture(drag, header_fab.clone()))
        };
        let size = self.widget_size();
        // The ground samples the page gradient at the widget's own rows:
        // its bottom sits WIDGET_OFFSET above the circle.
        let window_h = f32::from(window.viewport_size().height).max(1.);
        let bottom = corner(window, cx).map_or(window_h, |circle| f32::from(circle.top()) - WIDGET_OFFSET);
        let ground = card_ground((bottom - size.height) / window_h, bottom / window_h);
        let card_cell = self.card.clone();
        let measure = cx.entity().downgrade();
        div()
            .absolute()
            .inset_0()
            // #EXP-1876: the content card's size (this layer fills it).
            .child(
                canvas(
                    move |bounds, window, _| {
                        let card = AssistantWidgetSize { width: f32::from(bounds.size.width), height: f32::from(bounds.size.height) };
                        if card_cell.get() != Some(card) {
                            card_cell.set(Some(card));
                            let measure = measure.clone();
                            window.on_next_frame(move |_, cx| {
                                let _ = measure.update(cx, |_, cx| cx.notify());
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(div()
            .absolute()
            .right(px(ASSISTANT_FAB.inset))
            .bottom(px(ASSISTANT_FAB.inset))
            .child(
                Popover::new("assistant-widget")
                    .open(open)
                    .anchor(gpui::Anchor::BottomRight)
                    .offset(px(WIDGET_OFFSET))
                    .w(px(size.width))
                    .h(px(size.height))
                    .p_0()
                    .bg(ground)
                    .rounded(px(ASSISTANT_FAB.widget.radius))
                    .shadow(widget_shadow())
                    .overflow_hidden()
                    .trigger(FabCircle { selected: open, badge: derived.badge, entrance })
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
                    .size(px(fab_px())),
            )
            .children(bubble))
            .into_any_element()
    }
}

/// #EXP-1876: while an edge drag is live, a window-level capture (the
/// pointer leaves the 6px band at once) pins the resize cursor and tracks
/// the size; the up (or a move with the button released) ends it and
/// remembers the size (resize_edge.rs `drag_capture`'s pattern).
fn drag_capture(drag: Option<WidgetDrag>, fab: gpui::WeakEntity<AssistantFab>) -> Option<gpui::AnyElement> {
    let drag = drag?;
    let cursor = match (drag.width, drag.height) {
        (true, true) => CursorStyle::ResizeUpLeftDownRight,
        (true, false) => CursorStyle::ResizeLeftRight,
        _ => CursorStyle::ResizeUpDown,
    };
    Some(
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.set_window_cursor_style(cursor);
                let move_fab = fab.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if !phase.bubble() {
                        return;
                    }
                    let _ = move_fab.update(cx, |fab, cx| {
                        if event.pressed_button != Some(MouseButton::Left) {
                            fab.end_resize(cx);
                        } else {
                            fab.track_resize(f32::from(event.position.x), f32::from(event.position.y), cx);
                        }
                    });
                });
                let up_fab = fab.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase.bubble() {
                        let _ = up_fab.update(cx, |fab, cx| fab.end_resize(cx));
                    }
                });
            },
        )
        .absolute()
        .size_full()
        .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entrance_scale_runs_through_its_stops() {
        let spec = &ASSISTANT_FAB.entrance;
        let at = |t| entrance_scale(t, spec.scale, spec.scale_times);
        assert_eq!(at(0.), spec.scale[0]);
        assert!((at(spec.scale_times[1]) - spec.scale[1]).abs() < 1e-4, "the overshoot");
        assert!((at(1.) - spec.scale[2]).abs() < 1e-4, "rests at 1");
        assert!(at(spec.scale_times[1] / 2.) > spec.scale[0]);
    }

    #[test]
    fn a_waiting_card_reads_without_its_lead() {
        let plan = domain::rows::MessageRow {
            id: "m1".into(),
            card: Some(serde_json::json!({"kind": "plan", "plan": "Assistant widget: bigger window"})),
            ..Default::default()
        };
        assert_eq!(waiting_card_view(&plan), (true, "Assistant widget: bigger window".to_string()));
        let question = domain::rows::MessageRow {
            id: "m2".into(),
            card: Some(serde_json::json!({"kind": "question", "question": "Which base?"})),
            ..Default::default()
        };
        assert_eq!(waiting_card_view(&question), (false, "Which base?".to_string()));
    }

    /// #EXP-1876 look (b): the genUI Button `fab` `lg` recipe of the
    /// exponential theme: the sheen white → zinc-300 in dark, zinc-600 →
    /// the foreground in light, over the ground, lifted by `shadow.lg`.
    #[test]
    fn the_mark_wears_the_genui_fab_lg_look() {
        use eui_core::theme::Mode;
        let hex = |h: &str| eui_gpui::paint::color::parse_hex(h).unwrap();
        let dark = fab_look_in(Mode::Dark, gpui::black());
        let stops: Vec<gpui::Hsla> = dark.sheen.as_ref().expect("a sheen").stops.iter().map(|s| s.0).collect();
        assert_eq!(stops, vec![hex("#ffffff"), hex("#c4c4cc")]);
        assert_eq!(dark.sheen.as_ref().unwrap().angle, 180.0);
        assert_eq!(dark.ground, Some(hex("#0a0a0a")));
        assert!(!dark.shadows.is_empty(), "lifted");
        let light = fab_look_in(Mode::Light, gpui::black());
        let stops: Vec<gpui::Hsla> = light.sheen.expect("a sheen").stops.iter().map(|s| s.0).collect();
        assert_eq!(stops, vec![hex("#52525b"), hex("#0a0a0a")]);
        assert_eq!(light.ground, Some(hex("#ffffff")));
    }
}
