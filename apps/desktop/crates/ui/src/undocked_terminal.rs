//! A native window hosting ONE undocked terminal tab (EXP-65).
//!
//! The tab is still owned by the origin window's `TerminalManager` — this
//! view only renders its `TerminalView` (the dock hides the tab while the
//! [`crate::undock`] registry maps it here, so exactly one window paints it).
//! Exit hooks, OSC titles, run-bar stop and steer wiring all keep flowing
//! through the manager untouched.
//!
//! Closing the window (titlebar X) is equivalent to Reattach: the release
//! hook unregisters the tab, which reshows it in the dock — a running
//! session must never become unreachable. Closing the tab from the manager
//! side (its close button in this window, run-bar stop, `close_tab`) closes
//! this window via the manager's `TabClosed` event.

use gpui::{
    div, px, AnyWindowHandle, App, ClickEvent, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, IconName, Root, Sizable as _,
};
use terminal::{TabId, TerminalManager, TerminalManagerEvent};

use crate::undock;

pub(crate) struct UndockedTerminalWindow {
    manager: Entity<TerminalManager>,
    tab_id: TabId,
    /// The workspace window whose dock owns the tab (reattach target).
    origin: AnyWindowHandle,
    focus_handle: FocusHandle,
    /// Last title pushed to the OS window (OSC titles sync live).
    window_title: SharedString,
    _subscriptions: Vec<Subscription>,
    _steer_subscription: Option<Subscription>,
}

impl UndockedTerminalWindow {
    pub(crate) fn new(
        manager: Entity<TerminalManager>,
        tab_id: TabId,
        origin: AnyWindowHandle,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let own_handle = window.window_handle();
        undock::register_terminal_tab(tab_id, own_handle, origin.window_id(), cx);

        let mut subscriptions = Vec::new();
        // The manager closing the tab (close button here, run-bar stop, …)
        // closes this window; exits/titles repaint via the observe below.
        subscriptions.push(cx.subscribe(
            &manager,
            move |this, _, event: &TerminalManagerEvent, cx| {
                if let TerminalManagerEvent::TabClosed(id) = event {
                    if *id == this.tab_id {
                        // Deferred — we are inside this window's update.
                        cx.defer(move |cx| {
                            let _ = own_handle.update(cx, |_, window, _| window.remove_window());
                        });
                    }
                }
                cx.notify();
            },
        ));
        subscriptions.push(cx.observe(&manager, |_, _, cx| cx.notify()));

        // §8.5 parity with the dock: repaint when a presence frame flips the
        // remote steerer so the banner shows/hides live.
        let steer_subscription =
            crate::steer_wiring::observe_steer_presence(cx, |_, cx| cx.notify());

        // Titlebar-X ≡ Reattach: unregister (dock reshows the tab) and bring
        // it back into view in the owner window — without raising it, so a
        // plain close doesn't yank focus.
        cx.on_release(move |this, cx| {
            undock::unregister_terminal_tab(this.tab_id, cx);
            if this.manager.read(cx).tab(this.tab_id).is_some() {
                undock::restore_tab_in_owner(
                    this.origin,
                    this.manager.clone(),
                    this.tab_id,
                    false,
                    cx,
                );
            }
        })
        .detach();

        // The session keeps running — put the caret where the user expects.
        if let Some(tab) = manager.read(cx).tab(tab_id) {
            let handle = tab.view.focus_handle(cx);
            window.focus(&handle, cx);
        }

        Self {
            manager,
            tab_id,
            origin,
            focus_handle: cx.focus_handle(),
            window_title: SharedString::default(),
            _subscriptions: subscriptions,
            _steer_subscription: steer_subscription,
        }
    }

    /// Explicit reattach: raise the owner window with the tab active, then
    /// close this window (the release hook unregisters).
    fn reattach(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        undock::restore_tab_in_owner(self.origin, self.manager.clone(), self.tab_id, true, cx);
        let this_window = window.window_handle();
        cx.defer(move |cx| {
            let _ = this_window.update(cx, |_, window, _| window.remove_window());
        });
    }

    /// The dock's §8.5 "Remote steering" banner, replicated for the undocked
    /// surface so a steered coding tab never loses the signal.
    fn render_steer_banner(
        &self,
        session_id: String,
        steerer: String,
        cx: &gpui::Context<Self>,
    ) -> impl IntoElement {
        let accent = cx.theme().warning;
        h_flex()
            .gap_2()
            .px_3()
            .py_1()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(accent.opacity(0.12))
            .text_xs()
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(Icon::new(IconName::Eye).xsmall().text_color(accent))
                    .child(
                        div()
                            .text_color(cx.theme().foreground)
                            .child(SharedString::from(format!("Remote steering — {steerer}"))),
                    ),
            )
            .child(
                Button::new("undocked-steer-take-over")
                    .outline()
                    .xsmall()
                    .label("Take over")
                    .tooltip("Revoke the remote steerer — your typing is never blocked.")
                    .on_click(cx.listener(move |_, _: &ClickEvent, _window, cx| {
                        crate::steer_wiring::take_over(&session_id, cx);
                    })),
            )
    }
}

impl Focusable for UndockedTerminalWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for UndockedTerminalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let (title, view, exit_code) = match self.manager.read(cx).tab(self.tab_id) {
            Some(tab) => (tab.title().clone(), Some(tab.view.clone()), tab.exit_code()),
            // Tab already closed — the TabClosed handler is tearing this
            // window down; render nothing for the last frame.
            None => ("Terminal".into(), None, None),
        };
        if self.window_title != title {
            self.window_title = title.clone();
            window.set_window_title(&title);
        }

        let steer_banner = crate::steer_wiring::remote_steerer_for_tab(self.tab_id, cx);

        let header = h_flex()
            .h(px(34.))
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().title_bar)
            .child(
                Icon::new(IconName::SquareTerminal)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(div().text_sm().child(title))
            .child(div().flex_1())
            .child(
                Button::new("reattach-terminal-tab")
                    .ghost()
                    .xsmall()
                    .label("Reattach")
                    .tooltip("Move back into the terminal dock")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.reattach(window, cx);
                    })),
            );

        // Root layers for parity with every other window (notifications).
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        let mut body = v_flex().size_full().child(header);
        if let Some((session_id, steerer)) = steer_banner {
            body = body.child(self.render_steer_banner(session_id, steerer, cx));
        }
        if let Some(view) = view {
            body = body.child(div().flex_1().min_h_0().child(view));
        } else {
            body = body.child(div().flex_1());
        }
        if let Some(code) = exit_code {
            body = body.child(crate::terminal_dock::exit_strip(code, cx));
        }

        div()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .child(body)
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}
