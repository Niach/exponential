//! The Devices center screen (EXP-686): the web `t/$teamSlug/devices` page —
//! the signed-in user's machines and nothing else. It was the top section of
//! the old Actions page; the split gave it its own rail entry so devices,
//! actions and automations each stand on their own.
//!
//! The page owns nothing but the scaffold: every row, poll and mutation lives
//! in [`crate::machines::MachinesSection`], which reads the synced `devices`
//! shape directly (EXP-485).

use gpui::{
    AppContext as _, Entity, IntoElement, ParentElement, Render, ScrollHandle, Styled, Subscription,
    Window,
};

use crate::actions_view::page_scaffold;
use crate::navigation::{nav_for_window, Navigation};

pub struct DevicesView {
    #[allow(dead_code)] // held for the team-switch re-render subscription
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// EXP-403: the same per-user device registry web/iOS/Android show. Its
    /// rows come straight off the synced `devices` shape (EXP-485), so it
    /// holds no poll of its own.
    machines: Entity<crate::machines::MachinesSection>,
    _subscriptions: Vec<Subscription>,
}

impl DevicesView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let machines = cx.new(|cx| crate::machines::MachinesSection::new(window, cx));
        // A team switch re-scopes the section's reads.
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        Self {
            nav,
            scroll: ScrollHandle::new(),
            machines,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for DevicesView {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        page_scaffold(
            "devices-screen-scroll",
            &self.scroll,
            gpui_component::v_flex().gap_6().child(self.machines.clone()),
        )
    }
}
