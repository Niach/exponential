//! Web-parity control metrics (EXP-525) — the shadcn sizing layer.
//!
//! gpui-component sizes its controls in rem off the host's root font size
//! (14px in the IDE since EXP-723), which still lands every button/input
//! under the web's shadcn boxes (web Button default h-9/px-4, sm h-8/px-3,
//! xs h-6/px-2, all on the theme radius since EXP-1176 — a text button is a
//! rounded rectangle, the capsule is the pill recipe's; inputs h-9). These
//! helpers are pure `Styled` refinements — gpui-component applies caller
//! refinements after its own base styles (`refine_style` runs last and is
//! replayed inside the selected/disabled state closures), so they win
//! without forking the component. `cursor_pointer` rides the same
//! refinement: gpui-component buttons default to `cursor_default`, the web
//! (and every native toolkit convention we mirror) points on hover.

use gpui::{px, Styled};
use gpui_component::{Sizable, Size};

// EXP-698: the rung names match the TOKEN ladder (`size::CONTROL_*`), which
// is the same ladder on all four clients — LG 36 / MD 32 / SM 24. They used
// to be shifted one notch (36 was "MD"), which made every cross-file read a
// translation step. The values are the Exponential design tokens
// (`packages/design-tokens/tokens.json` `size.control*`); the IDE pins them
// against its generated `theme::tokens::size` in a test.
/// Web `h-9` (Button default / Input) — `size::INPUT_HEIGHT`.
pub const CTL_LG_H: f32 = 36.;
/// Web `h-8` (Button sm / small inputs) — `size::CONTROL_MD`.
pub const CTL_MD_H: f32 = 32.;
/// Web `h-6` (Button xs) — `size::CONTROL_SM`.
pub const CTL_SM_H: f32 = 24.;

/// One import per file: `use …::controls::WebControl as _;`
/// `with_size` keeps the component's own label/icon typography mapping; the
/// explicit height/padding overrides the too-small rem-derived boxes.
pub trait WebControl: Styled + Sizable + Sized {
    /// Web Button default: h-9 px-4, theme radius.
    fn web_md(self) -> Self {
        self.with_size(Size::Medium)
            .h(px(CTL_LG_H))
            .px(px(16.))
            .cursor_pointer()
    }

    /// Web Button `sm`: h-8 px-3, theme radius (EXP-1176).
    fn web_sm(self) -> Self {
        self.with_size(Size::Small)
            .h(px(CTL_MD_H))
            .px(px(12.))
            .cursor_pointer()
    }

    /// Web Button `xs`: h-6 px-2, theme radius (EXP-1176).
    fn web_xs(self) -> Self {
        self.with_size(Size::XSmall)
            .h(px(CTL_SM_H))
            .px(px(8.))
            .cursor_pointer()
    }

    /// Web Button icon-sm: size-8 circle.
    fn web_icon_sm(self) -> Self {
        self.with_size(Size::Small)
            .size(px(CTL_MD_H))
            .rounded_full()
            .cursor_pointer()
    }

    /// Web Button `icon-xs`: size-6 circle.
    fn web_icon_xs(self) -> Self {
        self.with_size(Size::XSmall)
            .size(px(CTL_SM_H))
            .rounded_full()
            .cursor_pointer()
    }

    /// Web Input: h-9 (radius stays the component's).
    fn web_input(self) -> Self {
        self.h(px(CTL_LG_H))
    }

    /// Web small input/select: h-8.
    fn web_input_sm(self) -> Self {
        self.h(px(CTL_MD_H))
    }
}

impl<T: Styled + Sizable> WebControl for T {}

/// EXP-698 — the web's 11px caption rung (`text-[11px]`), one step below
/// `text_xs` (12px). gpui has no rung there, and the steer feed needs two
/// caption levels: a tool row's mono argument, a permission's detail and
/// hint, a subagent's status line and the stepper counter all render at 11 on
/// the web, and rendering them at 12 flattens them into the labels above them.
///
/// Its own trait, not a [`WebControl`] method: that one is bounded on
/// `Sizable` (a gpui-component CONTROL), and these are plain `Div`s.
///
/// One import per file: `use …::controls::WebText as _;`
pub trait WebText: Styled + Sized {
    fn text_2xs(self) -> Self {
        self.text_size(gpui::rems(0.6875))
    }
}

impl<T: Styled> WebText for T {}
