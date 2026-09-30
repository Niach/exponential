//! EXP-1031 — THE toast, the IDE twin of `@exp/ui` `toast.tsx` (sonner), iOS
//! `Toast.swift` and Android `Toast.kt`, all drawn from ONE contract:
//! `packages/domain-contract/fixtures/toast-stack.json`.
//!
//! The IDE's stack is gpui-component's `Notification` list: its engine
//! (`gpui_base::ToastMotion::sonner()`) already stacks like sonner — newest
//! in front, three visible collapsed, older ones peeking by 14 and shrinking
//! by 5% per rank, hover expands them 14 apart and pauses the clock. This
//! module owns only the FACE: the kind colour on the ICON alone (registry
//! concepts `ui-success` / `ui-error` / `ui-info` / `ui-warning`), the text in
//! the foreground, the opaque glass card fill under the card hairline at
//! radius lg and NO shadow. Placement (bottom-right, 24 from the edges) and
//! width (356) are `theme::apply_exponential_dark`'s `theme.notification`.
//!
//! [`show`] pushes a live toast; [`card`] draws the same face as a plain
//! element for the styleguide. Both go through [`chrome`] + [`body`], so the
//! two cannot drift.
//!
//! Accepted leftovers (the vendored crate's): the 5 s auto-dismiss (the
//! contract says 4 s), the close glyph that only shows on hover, and the
//! crate's own enter/exit easing.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, Div, Hsla, IntoElement as _, ParentElement as _, SharedString, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    notification::Notification,
    v_flex, ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
};
use theme::tokens as t;

use crate::icons::{registry, ExpIcon};

/// The contract's numbers (`toast-stack.json` `constants`), locked by the
/// tests below. `durationMs` is the contract's; the IDE's crate keeps 5 s.
pub(crate) mod constants {
    pub(crate) const WIDTH: f32 = theme::TOAST_WIDTH;
    pub(crate) const GAP: f32 = 14.;
    pub(crate) const PEEK: f32 = 14.;
    pub(crate) const SCALE_STEP: f32 = 0.05;
    pub(crate) const VISIBLE: usize = 3;
    #[cfg_attr(not(test), allow(dead_code))] // placement is theme.notification
    pub(crate) const VIEWPORT_OFFSET: f32 = theme::TOAST_VIEWPORT_OFFSET;
}

/// A toast's kind: it colours the icon, nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Success,
    Error,
    Info,
    Warning,
}

impl ToastKind {
    /// Every kind, in the contract's `kinds` order.
    pub(crate) const ALL: [ToastKind; 4] = [Self::Success, Self::Error, Self::Info, Self::Warning];

    /// The contract spelling (`constants.kinds`).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
            Self::Info => "info",
            Self::Warning => "warning",
        }
    }

    /// The registry concept the kind draws.
    pub(crate) fn icon(self) -> ExpIcon {
        match self {
            Self::Success => registry::UI_SUCCESS,
            Self::Error => registry::UI_ERROR,
            Self::Info => registry::UI_INFO,
            Self::Warning => registry::UI_WARNING,
        }
    }

    /// The icon's colour — the ONLY place a toast wears its kind.
    pub(crate) fn color(self, cx: &App) -> Hsla {
        let theme = cx.theme();
        match self {
            Self::Success => theme.success,
            Self::Error => theme.danger,
            Self::Info => theme.info,
            Self::Warning => theme.warning,
        }
    }
}

/// The toast's one button: it runs `on_click` and dismisses the toast.
#[derive(Clone)]
pub(crate) struct ToastAction {
    pub label: SharedString,
    pub on_click: Rc<dyn Fn(&mut Window, &mut App)>,
}

/// One toast: a sentence (`title`), an optional `description`, an optional
/// `action`, and its `kind`.
#[derive(Clone)]
pub(crate) struct Toast {
    pub kind: ToastKind,
    pub title: SharedString,
    pub description: Option<SharedString>,
    pub action: Option<ToastAction>,
}

impl Toast {
    pub(crate) fn new(kind: ToastKind, title: impl Into<SharedString>) -> Self {
        Self { kind, title: title.into(), description: None, action: None }
    }

    pub(crate) fn success(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Success, title)
    }

    pub(crate) fn error(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Error, title)
    }

    pub(crate) fn info(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Info, title)
    }

    pub(crate) fn warning(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Warning, title)
    }

    #[allow(dead_code)] // part of the shared toast API; the styleguide uses it
    pub(crate) fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    #[allow(dead_code)] // part of the shared toast API; the styleguide uses it
    pub(crate) fn action(
        mut self,
        label: impl Into<SharedString>,
        on_click: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some(ToastAction { label: label.into(), on_click: Rc::new(on_click) });
        self
    }
}

/// Push `toast` onto `window`'s stack (the window's `Root` owns it).
pub(crate) fn show(toast: Toast, window: &mut Window, cx: &mut App) {
    window.push_notification(notification(toast, cx), cx);
}

/// Push onto the active (else primary) window — for callers without one.
pub(crate) fn show_in_active_window(toast: Toast, cx: &mut App) {
    crate::navigation::on_active_window(cx, move |window, cx| show(toast, window, cx));
}

pub(crate) fn success(title: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    show(Toast::success(title), window, cx);
}

pub(crate) fn error(title: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    show(Toast::error(title), window, cx);
}

#[allow(dead_code)] // the shared toast API: no IDE caller raises an info toast today
pub(crate) fn info(title: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    show(Toast::info(title), window, cx);
}

pub(crate) fn warning(title: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    show(Toast::warning(title), window, cx);
}

/// The crate `Notification` carrying our face. NO `with_type` — that would
/// force the crate's icon and tint; with none the crate draws our `.icon`.
fn notification(toast: Toast, cx: &App) -> Notification {
    let icon = kind_icon(toast.kind, cx);
    let action = toast.action.clone();
    let content = toast.clone();
    let note = Notification::new()
        .icon(icon)
        .content(move |_, _, cx| body(&content, cx).into_any_element());
    let note = match action {
        // `.action` turns autohide off; the contract keeps the clock running.
        Some(action) => note
            .action(move |_, _, cx| {
                let on_click = action.on_click.clone();
                action_button(action.label.clone()).on_click(cx.listener(
                    move |this, _, window, cx| {
                        this.dismiss(window, cx);
                        on_click(window, cx);
                    },
                ))
            })
            .autohide(true),
        None => note,
    };
    chrome(note, cx)
}

/// The card: opaque glass card fill, card hairline, radius lg, no shadow,
/// 16 in from every edge. Applied AFTER the crate's own chrome, so it wins.
fn chrome<T: Styled>(el: T, cx: &App) -> T {
    el.bg(cx.theme().popover)
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .rounded(px(t::radius::LG))
        .shadow_none()
        .p_4()
}

fn kind_icon(kind: ToastKind, cx: &App) -> Icon {
    Icon::new(kind.icon()).size(px(16.)).text_color(kind.color(cx))
}

/// Title (foreground, medium) over the description (muted).
fn body(toast: &Toast, cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .gap_0p5()
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(toast.title.clone()),
        )
        .when_some(toast.description.clone(), |this, description| {
            this.child(div().text_sm().text_color(theme.muted_foreground).child(description))
        })
}

fn action_button(label: SharedString) -> Button {
    Button::new("toast-action").label(label).primary()
}

/// The toast AT REST as a plain element (the styleguide's specimen): the
/// crate's frame (icon absolute at 18 / 16, content 24 in, action last,
/// close top-right) wearing the same [`chrome`] and [`body`] as [`show`].
pub(crate) fn card(toast: &Toast, cx: &App) -> Div {
    let frame = h_flex().relative().w(px(constants::WIDTH)).gap_3();
    chrome(frame, cx)
        .child(div().absolute().top(px(18.)).left_4().child(kind_icon(toast.kind, cx)))
        .child(v_flex().flex_1().overflow_hidden().pl_6().child(body(toast, cx)))
        .when_some(toast.action.clone(), |this, action| {
            this.child(action_button(action.label).small().mr_3p5())
        })
        .child(
            div().absolute().top_1().right_1().child(
                Button::new("toast-close")
                    .icon(Icon::new(registry::UI_CLOSE))
                    .ghost()
                    .xsmall(),
            ),
        )
}

/// A card BEHIND the front one in a collapsed stack: the chrome alone (web
/// `data-[front=false]` hides a back card's contents the same way).
pub(crate) fn card_back(cx: &App) -> Div {
    chrome(div().w(px(constants::WIDTH)), cx)
}

/// One item of a laid-out stack: its top inside the stack box, its width
/// factor, whether it draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StackItem {
    pub offset: f32,
    pub scale: f32,
    pub visible: bool,
}

/// The contract's stack geometry (`toast-stack.json` `geometry`, locked by
/// the tests): `heights` oldest first, returns the stack box height and one
/// item per toast in the same order. Rank 0 = the newest (front). Collapsed:
/// every older toast's edge moves `PEEK` per rank away from the anchor and
/// its width shrinks `SCALE_STEP` per rank (capped at the last visible
/// rank); only `VISIBLE` draw. Expanded: full size, `GAP` apart, all drawn.
pub(crate) fn stack_layout(
    heights: &[f32],
    expanded: bool,
    anchored_bottom: bool,
) -> (f32, Vec<StackItem>) {
    use constants::{GAP, PEEK, SCALE_STEP, VISIBLE};
    let n = heights.len();
    let rank = |i: usize| n - 1 - i;
    // Distance of each item's anchor-side edge from the box's anchor edge.
    let lead: Vec<f32> = if expanded {
        (0..n)
            .map(|i| heights[i + 1..].iter().map(|h| h + GAP).sum::<f32>())
            .collect()
    } else {
        (0..n).map(|i| PEEK * rank(i) as f32).collect()
    };
    let height = (0..n).map(|i| lead[i] + heights[i]).fold(0f32, f32::max);
    let items = (0..n)
        .map(|i| {
            let r = rank(i);
            let offset =
                if anchored_bottom { height - lead[i] - heights[i] } else { lead[i] };
            let (scale, visible) = if expanded {
                (1.0, true)
            } else {
                (1.0 - SCALE_STEP * r.min(VISIBLE - 1) as f32, r < VISIBLE)
            };
            StackItem { offset, scale, visible }
        })
        .collect();
    (height, items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/toast-stack.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("toast-stack.json parses")
    }

    fn num(v: &Value, key: &str) -> f64 {
        v["constants"][key].as_f64().unwrap_or_else(|| panic!("constants.{key}"))
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-4
    }

    /// The crate's stack engine IS the contract's numbers.
    #[test]
    fn sonner_motion_matches_the_fixture() {
        let f = fixture();
        let m = gpui_base::ToastMotion::sonner();
        assert!(close(f64::from(f32::from(m.collapsed_peek)), num(&f, "peek")));
        assert!(close(f64::from(f32::from(m.expanded_gap)), num(&f, "gap")));
        assert!(close(f64::from(m.collapsed_scale_step), num(&f, "scaleStep")));
        assert_eq!(m.collapsed_visible as f64, num(&f, "visible"));
    }

    #[test]
    fn constants_match_the_fixture() {
        let f = fixture();
        assert!(close(constants::WIDTH.into(), num(&f, "width")));
        assert!(close(constants::GAP.into(), num(&f, "gap")));
        assert!(close(constants::PEEK.into(), num(&f, "peek")));
        assert!(close(constants::SCALE_STEP.into(), num(&f, "scaleStep")));
        assert_eq!(constants::VISIBLE as f64, num(&f, "visible"));
        assert!(close(constants::VIEWPORT_OFFSET.into(), num(&f, "viewportOffset")));
        let kinds: Vec<&str> = f["constants"]["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        let ours: Vec<&str> = ToastKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(ours, kinds);
    }

    #[test]
    fn every_kind_draws_its_concept_icon() {
        use gpui_component::IconNamed as _;
        let pairs = [
            (ToastKind::Success, registry::UI_SUCCESS),
            (ToastKind::Error, registry::UI_ERROR),
            (ToastKind::Info, registry::UI_INFO),
            (ToastKind::Warning, registry::UI_WARNING),
        ];
        for (kind, concept) in pairs {
            assert_eq!(kind.icon().path(), concept.path(), "{}", kind.as_str());
        }
    }

    #[test]
    fn stack_layout_matches_every_fixture_case() {
        let f = fixture();
        for case in f["geometry"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let heights: Vec<f32> = case["heights"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h.as_f64().unwrap() as f32)
                .collect();
            let (height, items) = stack_layout(
                &heights,
                case["expanded"].as_bool().unwrap(),
                case["anchoredBottom"].as_bool().unwrap(),
            );
            assert!(close(height.into(), case["height"].as_f64().unwrap()), "{name}: height");
            let want = case["items"].as_array().unwrap();
            assert_eq!(items.len(), want.len(), "{name}: items");
            for (i, (got, want)) in items.iter().zip(want).enumerate() {
                assert!(close(got.offset.into(), want["offset"].as_f64().unwrap()), "{name}[{i}] offset");
                assert!(close(got.scale.into(), want["scale"].as_f64().unwrap()), "{name}[{i}] scale");
                assert_eq!(got.visible, want["visible"].as_bool().unwrap(), "{name}[{i}] visible");
            }
        }
    }

    /// `theme::init` puts the stack bottom-right, 24 in, 356 wide.
    #[gpui::test]
    fn theme_places_the_stack_bottom_right(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            let n = &cx.theme().notification;
            assert_eq!(n.placement, gpui::Anchor::BottomRight);
            assert_eq!(n.width, px(356.));
            assert_eq!(n.max_items, 10);
            for edge in [n.margins.top, n.margins.right, n.margins.bottom, n.margins.left] {
                assert_eq!(edge, px(24.));
            }
        });
    }

    /// A pushed toast lands as exactly one notification and paints (with and
    /// without an action). A `Root` cannot be built in a macOS test window
    /// (see `markdown/editor.rs`), so this drives the crate's list directly —
    /// the one `Root::push_notification` forwards to.
    #[gpui::test]
    fn a_toast_lands_as_one_notification(cx: &mut gpui::TestAppContext) {
        use gpui::{AppContext as _, Context, Entity, IntoElement, Render};
        use gpui_component::notification::NotificationList;

        struct Host {
            list: Entity<NotificationList>,
        }
        impl Render for Host {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().size_full().child(self.list.clone())
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (host, cx) = cx.add_window_view(|window, cx| Host {
            list: cx.new(|cx| NotificationList::new(window, cx)),
        });
        let list = host.read_with(cx, |host, _| host.list.clone());
        list.update_in(cx, |list, window, cx| {
            let note = notification(Toast::error("Could not merge the pull request"), cx);
            list.push(note, window, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(list.read_with(cx, |list, _| list.notifications().len()), 1);

        list.update_in(cx, |list, window, cx| {
            let toast = Toast::success("Issue created")
                .description("EXP-1031 is on the board")
                .action("Open", |_, _| {});
            list.push(notification(toast, cx), window, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(list.read_with(cx, |list, _| list.notifications().len()), 2);
    }
}
