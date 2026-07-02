//! macOS menubar (masterplan-v3 §3.6: `cx.set_menus(vec![Menu { … }])`).
//!
//! Phase-1 scope: app + window lifecycle. Per EXP-1 #11 the menubar is the
//! *secondary* affordance — Settings et al. live in the sidebar footer
//! dropdown; menu entries for them may be added alongside their Phase-3
//! handlers.

#![cfg(target_os = "macos")]

use gpui::{App, Menu, MenuItem};

use crate::actions::{NewWindow, Quit};

pub fn install_menubar(cx: &mut App) {
    cx.set_menus(vec![
        Menu::new("Exponential").items([MenuItem::action("Quit Exponential", Quit)]),
        Menu::new("File").items([MenuItem::action("New Window", NewWindow)]),
    ]);
}
