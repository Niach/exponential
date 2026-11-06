//! The numbers the shell documents use (`packages/uicore/fixtures/shell-layout.json`),
//! so eDE's documents and later the IDE agree on one geometry.

use serde::{Deserialize, Serialize};

const SOURCE: &str = include_str!("../../../../../packages/uicore/fixtures/shell-layout.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellLayout {
    pub version: u32,
    pub sidebar_width: u32,
    pub rail_width: u32,
    pub taskbar_height: u32,
    pub panel_margin: u32,
    pub panel_radius: u32,
    pub sidebar_row_height: u32,
    pub section_label_height: u32,
    pub taskbar_button_size: u32,
    pub applet_button_size: u32,
    pub start_menu: Size,
    pub previews_tile: Size,
}

/// The fixture's numbers, parsed once.
pub fn shell_layout() -> &'static ShellLayout {
    static LAYOUT: std::sync::OnceLock<ShellLayout> = std::sync::OnceLock::new();
    LAYOUT.get_or_init(|| serde_json::from_str(SOURCE).expect("shell-layout.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_numbers_are_the_fixtures() {
        let l = shell_layout();
        assert_eq!((l.sidebar_width, l.rail_width, l.taskbar_height), (272, 56, 48));
        assert_eq!((l.panel_margin, l.panel_radius), (10, 12));
        assert_eq!(l.start_menu, Size { width: 760, height: 620 });
    }
}
