//! In-app titlebar strip with window controls (EXP-269).
//!
//! Vendored adaptation of gpui-component's `title_bar.rs` (rev a9a7341,
//! Apache-2.0). This file is Apache-2.0, NOT the repository licence — see the
//! `NOTICE` and `LICENSE-APACHE-2.0` files at the root of this crate.
//!
//! The deliberate deltas — the first of which is why we carry a copy at all:
//!
//! 1. **Rounded window controls** (below).
//! 2. **EXP-287 [`TitleBar::window_controls`]**: minimize/maximize are
//!    optional. Upstream always draws both, but a non-resizable window (every
//!    fixed-size native dialog) has no maximize affordance at the OS level, so
//!    the button would be dead chrome. Upstream has no hook for this either —
//!    `WindowControls` and `ControlIcon` are private.
//!
//! On (1): the close button's hover/active fill is the
//! last thing painted in the window's TOP-RIGHT corner, and upstream paints
//! it as a plain square. gpui's content mask is rectangular
//! ([`crate::window_frame::frame_radii`]), so that square fill lands in the
//! 12px notch outside the frame's arc and the red hover flashes a square
//! corner back over the rounded window. Upstream has no hook for this:
//! `ControlIcon` and `WindowControls` are private and `TitleBar` always
//! renders them, so styling them means owning them.
//!
//! Everything else — drag/move, the Linux right-click window menu,
//! double-click zoom, the Windows `WindowControlArea` hitboxes that drive
//! Snap Layouts, the macOS traffic-light gutter, and the Linux-only gate on
//! [`TitleBar::on_close_window`] — is upstream's behavior, carried over
//! as-is.
//!
//! One consequence worth knowing (EXP-294): the Linux window-menu overlay
//! fires for EVERY right press over the bar, including one that landed on
//! bar content, and the WM menu takes a pointer grab that buries anything we
//! popped ourselves. Content with its own right-click menu — the EXP-277 tab
//! strip — must therefore swallow the press, which
//! [`crate::app_title_bar::interactive`] does for everything it wraps.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, ClickEvent, Context, Corners,
    Decorations, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Render,
    RenderOnce, StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
    WindowControlArea,
};
use gpui_component::{
    h_flex, ActiveTheme, Icon, IconName, InteractiveElementExt as _, Sizable as _, StyledExt as _,
};
pub(crate) const TITLE_BAR_HEIGHT: Pixels = px(34.);
#[cfg(target_os = "macos")]
const TITLE_BAR_LEFT_PADDING: Pixels = px(80.);
#[cfg(not(target_os = "macos"))]
const TITLE_BAR_LEFT_PADDING: Pixels = px(12.);

/// TitleBar used to customize the appearance of the title bar.
///
/// We can put some elements inside the title bar.
#[derive(IntoElement)]
pub struct TitleBar {
    style: StyleRefinement,
    children: Vec<AnyElement>,
    on_close_window: Option<Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>>,
    /// EXP-287 delta: which of the min/max controls the strip draws. Upstream
    /// always draws both — but a window opened with `is_resizable: false`
    /// (every fixed-size native dialog) has no `WS_MAXIMIZEBOX` on Windows and
    /// no `NSResizableWindowMask` on macOS, so its maximize button is dead
    /// chrome. Close is never optional.
    show_minimize: bool,
    show_maximize: bool,
}

impl TitleBar {
    /// Create a new TitleBar.
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            children: Vec::new(),
            on_close_window: None,
            show_minimize: true,
            show_maximize: true,
        }
    }

    /// EXP-287: pick which window controls the strip draws (Windows/Linux —
    /// macOS renders none of them; the native traffic lights are the chrome).
    /// Pass `maximize: false` for a non-resizable window.
    pub(crate) fn window_controls(mut self, minimize: bool, maximize: bool) -> Self {
        self.show_minimize = minimize;
        self.show_maximize = maximize;
        self
    }

    // `title_bar_options()` is deliberately NOT copied: it returns plain
    // `TitlebarOptions` with no styling of its own, so the public
    // `gpui_component::TitleBar::title_bar_options()` stays the single source
    // of truth for the window options every `WindowOptions` site passes.

    /// Add custom for close window event, default is None, then click X button will call `window.remove_window()`.
    /// Linux only, this will do nothing on other platforms.
    ///
    /// EXP-287: the Linux-only gate is upstream's and is kept deliberately —
    /// it is exactly where it is needed. `Window::remove_window` never
    /// consults `on_window_should_close`, so on Linux this callback is the
    /// only way a busy dialog can refuse its own ✕. Windows routes the
    /// control's `WindowControlArea::Close` hitbox through `WM_CLOSE` and
    /// macOS through `windowShouldClose:` — both land on
    /// `on_window_should_close`, which native dialogs already register with
    /// the same `can_close` gate, and installing a second client-side path
    /// there would close the window twice.
    pub fn on_close_window(
        mut self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if cfg!(target_os = "linux") {
            self.on_close_window = Some(Rc::new(Box::new(f)));
        }
        self
    }
}

// The Windows control buttons have a fixed width of 35px.
//
// We don't need implementation the click event for the control buttons.
// If user clicked in the bounds, the window event will be triggered.
#[derive(IntoElement, Clone)]
enum ControlIcon {
    Minimize,
    Restore,
    Maximize,
    Close {
        on_close_window: Option<Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>>,
    },
}

impl ControlIcon {
    fn minimize() -> Self {
        Self::Minimize
    }

    fn restore() -> Self {
        Self::Restore
    }

    fn maximize() -> Self {
        Self::Maximize
    }

    fn close(on_close_window: Option<Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>>) -> Self {
        Self::Close { on_close_window }
    }

    fn id(&self) -> &'static str {
        match self {
            Self::Minimize => "minimize",
            Self::Restore => "restore",
            Self::Maximize => "maximize",
            Self::Close { .. } => "close",
        }
    }

    fn icon(&self) -> IconName {
        match self {
            Self::Minimize => IconName::WindowMinimize,
            Self::Restore => IconName::WindowRestore,
            Self::Maximize => IconName::WindowMaximize,
            Self::Close { .. } => IconName::WindowClose,
        }
    }

    fn window_control_area(&self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Restore | Self::Maximize => WindowControlArea::Max,
            Self::Close { .. } => WindowControlArea::Close,
        }
    }

    fn is_close(&self) -> bool {
        matches!(self, Self::Close { .. })
    }

    #[inline]
    fn hover_fg(&self, cx: &App) -> Hsla {
        if self.is_close() {
            cx.theme().danger_foreground
        } else {
            cx.theme().secondary_foreground
        }
    }

    #[inline]
    fn hover_bg(&self, cx: &App) -> Hsla {
        if self.is_close() {
            cx.theme().danger
        } else {
            cx.theme().secondary_hover
        }
    }

    #[inline]
    fn active_bg(&self, cx: &mut App) -> Hsla {
        if self.is_close() {
            cx.theme().danger_active
        } else {
            cx.theme().secondary_active
        }
    }
}

impl RenderOnce for ControlIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_linux = cfg!(target_os = "linux");
        let is_windows = cfg!(target_os = "windows");
        let hover_fg = self.hover_fg(cx);
        let hover_bg = self.hover_bg(cx);
        let active_bg = self.active_bg(cx);
        // EXP-269 delta: the close button is flush with the window's TOP-RIGHT
        // corner, so its hover/active fill must carry the frame's radius there
        // — a square fill would paint the corner notch back in (bright red, on
        // the one control users hover most).
        let corner = if self.is_close() {
            crate::window_frame::frame_radii(window).top_right
        } else {
            px(0.0)
        };
        let icon = self.clone();
        let on_close_window = match &self {
            ControlIcon::Close { on_close_window } => on_close_window.clone(),
            _ => None,
        };

        div()
            .id(self.id())
            .flex()
            .w(TITLE_BAR_HEIGHT)
            .h_full()
            .flex_shrink_0()
            .justify_center()
            .content_center()
            .items_center()
            .rounded_tr(corner)
            .text_color(cx.theme().foreground)
            .hover(|style| style.bg(hover_bg).text_color(hover_fg))
            .active(|style| style.bg(active_bg).text_color(hover_fg))
            .when(is_windows, |this| {
                this.window_control_area(self.window_control_area())
            })
            .when(is_linux, |this| {
                this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    match icon {
                        Self::Minimize => window.minimize_window(),
                        Self::Restore | Self::Maximize => window.zoom_window(),
                        Self::Close { .. } => {
                            if let Some(f) = on_close_window.clone() {
                                f(&ClickEvent::default(), window, cx);
                            } else {
                                window.remove_window();
                            }
                        }
                    }
                })
            })
            .child(Icon::new(self.icon()).small())
    }
}

#[derive(IntoElement)]
struct WindowControls {
    on_close_window: Option<Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>>,
    /// EXP-287: see [`TitleBar::window_controls`].
    show_minimize: bool,
    show_maximize: bool,
}

impl RenderOnce for WindowControls {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        if cfg!(target_os = "macos") || cfg!(target_family = "wasm") {
            return div().id("window-controls");
        }

        h_flex()
            .id("window-controls")
            .items_center()
            .flex_shrink_0()
            .h_full()
            .when(self.show_minimize, |this| {
                this.child(ControlIcon::minimize())
            })
            .when(self.show_maximize, |this| {
                this.child(if window.is_maximized() {
                    ControlIcon::restore()
                } else {
                    ControlIcon::maximize()
                })
            })
            .child(ControlIcon::close(self.on_close_window))
    }
}

impl Styled for TitleBar {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for TitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

struct TitleBarState {
    should_move: bool,
}

// TODO: Remove this when GPUI has released v0.2.3
impl Render for TitleBarState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl RenderOnce for TitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        let is_web = cfg!(target_family = "wasm");
        let is_linux = cfg!(target_os = "linux");
        let is_macos = cfg!(target_os = "macos");

        // EXP-269 delta: the strip spans the window's top edge, so its own
        // fill (and border) round with the frame — the token is transparent
        // glass today, but a tinted one would square the corners right back.
        let Corners {
            top_left,
            top_right,
            ..
        } = crate::window_frame::frame_radii(window);

        let state = window.use_state(cx, |_, _| TitleBarState { should_move: false });

        div().flex_shrink_0().child(
            div()
                .id("title-bar")
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .h(TITLE_BAR_HEIGHT)
                .pl(TITLE_BAR_LEFT_PADDING)
                .border_b_1()
                .border_color(cx.theme().title_bar_border)
                .bg(cx.theme().tokens.title_bar)
                .rounded_tl(top_left)
                .rounded_tr(top_right)
                .refine_style(&self.style)
                .when(is_linux, |this| {
                    this.on_double_click(|_, window, _| window.zoom_window())
                })
                .when(is_macos, |this| {
                    this.on_double_click(|_, window, _| window.titlebar_double_click())
                })
                .on_mouse_down_out(window.listener_for(&state, |state, _, _, _| {
                    state.should_move = false;
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    window.listener_for(&state, |state, _, _, _| {
                        state.should_move = true;
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    window.listener_for(&state, |state, _, _, _| {
                        state.should_move = false;
                    }),
                )
                .on_mouse_move(window.listener_for(&state, |state, _, window, _| {
                    if state.should_move {
                        state.should_move = false;
                        window.start_window_move();
                    }
                }))
                .child(
                    h_flex()
                        .id("bar")
                        .h_full()
                        .justify_between()
                        .flex_shrink_0()
                        .flex_1()
                        .when(!is_web, |this| {
                            this.window_control_area(WindowControlArea::Drag)
                                .when(window.is_fullscreen(), |this| this.pl_3())
                                .when(is_linux && is_client_decorated, |this| {
                                    // Painted BEFORE `self.children`, so gpui's
                                    // reverse-bubble order runs it LAST: bar
                                    // content that stops right-button
                                    // propagation (EXP-294 — the `interactive`
                                    // wrapper) keeps its own menu, and the
                                    // remaining dead space keeps the WM one.
                                    this.child(
                                        div()
                                            .top_0()
                                            .left_0()
                                            .absolute()
                                            .size_full()
                                            .h_full()
                                            .on_mouse_down(
                                                MouseButton::Right,
                                                move |ev, window, _| {
                                                    window.show_window_menu(ev.position)
                                                },
                                            ),
                                    )
                                })
                        })
                        .children(self.children),
                )
                .child(WindowControls {
                    on_close_window: self.on_close_window,
                    show_minimize: self.show_minimize,
                    show_maximize: self.show_maximize,
                }),
        )
    }
}
