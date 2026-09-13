//! The Devices center screen (EXP-686): the web `t/$teamSlug/devices` page —
//! the signed-in user's devices and nothing else. It was the top section of
//! the old Actions page; the split gave it its own rail entry so devices,
//! actions and automations each stand on their own.
//!
//! The page owns nothing but the scaffold: every row, poll and mutation lives
//! in [`crate::machines::MachinesSection`], which reads the synced `devices`
//! shape directly (EXP-485), and — EXP-818 — in
//! [`crate::accounts_section::AccountsSection`], the Usage page folded in
//! under it: "My devices" (the SETUP surface: which device, which logins it
//! holds, what it may run), then Accounts (the DECISION surface: which login a
//! run should spend).

use gpui::{
    AppContext as _, Entity, IntoElement, ParentElement, Render, ScrollHandle, Subscription,
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
    /// EXP-818: the agent accounts across those devices (the old Usage page).
    accounts: Entity<crate::accounts_section::AccountsSection>,
    _subscriptions: Vec<Subscription>,
}

impl DevicesView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let machines = cx.new(|cx| crate::machines::MachinesSection::new(window, cx));
        let accounts = cx.new(|cx| crate::accounts_section::AccountsSection::new(window, cx));
        // A team switch re-scopes the section's reads.
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        Self {
            nav,
            scroll: ScrollHandle::new(),
            machines,
            accounts,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for DevicesView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _ = cx;
        page_scaffold(
            "devices-screen-scroll",
            &self.scroll,
            // EXP-818: the Running / Past run sections moved to the Agent
            // page's sessions list (`sidebar::SidebarPanel::render_sessions_tool`).
            gpui_component::v_flex()
                .child(self.machines.clone())
                .child(self.accounts.clone()),
        )
    }
}
