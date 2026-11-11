//! #EXP-1629 (`fixtures/interactions-round1.json` `menuPreview`, the TS
//! reference `src/menu-preview.ts`): a Menu item's `preview`, the IDE
//! composer menu's side hover card. WHICH row's preview shows, given the
//! row under the pointer, the keyboard-highlighted row and whether the
//! pointer is on the shown preview. Keys are item paths (`"2"`, a
//! submenu's row `"1.0"`); a row without a preview is `None`, like no row.
//!
//! THE RULE: the wanted row = the highlighted one ?? the shown one while the
//! pointer is on it ?? the hovered one. A highlight shows its row at once; a
//! hovered row shows [`crate::layout::MENU_PREVIEW_OPEN_MS`] after the
//! pointer entered it (another row restarts the wait); with nothing wanted
//! the shown one hides [`crate::layout::MENU_PREVIEW_CLOSE_MS`] later unless
//! wanted again in time. The surface runs the waits through
//! [`crate::surface::OutEvent::HoverTimer`] (owner `<menu>.preview`).

use crate::layout::{MENU_PREVIEW_CLOSE_MS, MENU_PREVIEW_OPEN_MS};

/// The `HoverTimer` owner suffix of a menu's preview wait.
pub const TIMER_SUFFIX: &str = ".preview";

/// One menu's preview state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuPreview {
    /// The row under the pointer.
    pub hover: Option<String>,
    /// The pointer is on the shown preview.
    pub over: bool,
    /// The keyboard-highlighted row.
    pub highlight: Option<String>,
    /// The row whose preview shows.
    pub shown: Option<String>,
    /// A change waiting for its timer: show this row (`None` = hide).
    pub pending: Option<Option<String>>,
}

/// One input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuPreviewInput {
    Hover(Option<String>),
    Over(bool),
    Highlight(Option<String>),
}

impl MenuPreview {
    /// The row the state wants shown.
    pub fn wanted(&self) -> Option<String> {
        if self.highlight.is_some() {
            return self.highlight.clone();
        }
        if self.over && self.shown.is_some() {
            return self.shown.clone();
        }
        self.hover.clone()
    }

    /// Settle after a change: `Some(ms)` = (re)start the wait.
    fn settle(&mut self) -> Option<u32> {
        let want = self.wanted();
        if want == self.shown {
            self.pending = None;
            return None;
        }
        if want.is_some() && want == self.highlight {
            self.shown = want;
            self.pending = None;
            return None;
        }
        if self.pending.as_ref() == Some(&want) {
            return None;
        }
        let delay = if want.is_none() { MENU_PREVIEW_CLOSE_MS } else { MENU_PREVIEW_OPEN_MS };
        self.pending = Some(want);
        Some(delay as u32)
    }

    /// One input: `Some(ms)` = (re)start the menu's wait.
    pub fn input(&mut self, input: MenuPreviewInput) -> Option<u32> {
        match input {
            MenuPreviewInput::Hover(k) => self.hover = k,
            MenuPreviewInput::Over(o) => self.over = o,
            MenuPreviewInput::Highlight(k) => self.highlight = k,
        }
        self.settle()
    }

    /// The wait ran out: the pending change applies (`Some(ms)` = wait again).
    pub fn timeout(&mut self) -> Option<u32> {
        let Some(next) = self.pending.take() else { return None };
        if next.is_none() {
            self.over = false;
        }
        self.shown = next;
        self.settle()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_waits_highlight_does_not() {
        let mut m = MenuPreview::default();
        assert_eq!(m.input(MenuPreviewInput::Hover(Some("0".into()))), Some(150));
        assert_eq!(m.shown, None);
        assert_eq!(m.timeout(), None);
        assert_eq!(m.shown.as_deref(), Some("0"));
        assert_eq!(m.input(MenuPreviewInput::Hover(None)), Some(80));
        assert_eq!(m.input(MenuPreviewInput::Highlight(Some("1".into()))), None);
        assert_eq!(m.shown.as_deref(), Some("1"));
        assert_eq!(m.pending, None);
    }
}
