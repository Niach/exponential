//! EXP-698 — the ONE composer card.
//!
//! Three surfaces type into a message box: the issue timeline's comment
//! composer (`comments::composer_row`), the steer viewer's reply composer
//! (`steer_viewer::render_composer`) and the helpdesk thread's reply/note
//! composer (`support_thread`). They used to be three hand-built cards with
//! three different radii, paddings and submit affordances (the support one
//! was a FILLED primary button). They are one recipe now:
//!
//! ```text
//! ┌───────────────────────────────────────────┐  radius XL, FILL_CARD
//! │ [leading row]                             │  (support: Reply / Note)
//! │ [strip]                                   │  attachment chips
//! │ the field                                 │  borderless, auto-grow
//! │ ⧉ ⌗ ☺                              ( ↑ )  │  tool row
//! └───────────────────────────────────────────┘
//! ```
//!
//! Every slot is optional except the field; the caller owns all state and
//! handlers, this only lays the card out.

use gpui::{div, px, AnyElement, App, Div, ParentElement as _, Styled};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _};
use theme::tokens as t;

use crate::controls::WebControl as _;

/// The composer card's slots. Build it with [`GlassComposer::new`] and fill
/// what the surface has.
pub(crate) struct GlassComposer {
    field: AnyElement,
    leading: Option<AnyElement>,
    strip: Option<AnyElement>,
    tools: Vec<AnyElement>,
    submit: Option<AnyElement>,
}

impl GlassComposer {
    pub(crate) fn new(field: impl Into<AnyElement>) -> Self {
        Self {
            field: field.into(),
            leading: None,
            strip: None,
            tools: Vec::new(),
            submit: None,
        }
    }

    /// A row INSIDE the card, above everything else — the helpdesk composer's
    /// Reply / Internal note mode pills.
    pub(crate) fn leading(mut self, leading: impl Into<AnyElement>) -> Self {
        self.leading = Some(leading.into());
        self
    }

    /// The attachment/pending strip between the leading row and the field.
    pub(crate) fn strip(mut self, strip: Option<AnyElement>) -> Self {
        self.strip = strip;
        self
    }

    /// One leading tool of the bottom row — a 24px ghost glyph button
    /// ([`composer_tool`]).
    pub(crate) fn tool(mut self, tool: impl Into<AnyElement>) -> Self {
        self.tools.push(tool.into());
        self
    }

    /// The trailing round ghost submit ([`composer_submit`]).
    pub(crate) fn submit(mut self, submit: impl Into<AnyElement>) -> Self {
        self.submit = Some(submit.into());
        self
    }
}

/// Lay a [`GlassComposer`] out as the card. Returns a bare `Div` so the
/// caller can still hang a `capture_action` (the steer composer's paste
/// handler) or a mode tint on it.
pub(crate) fn glass_composer(composer: GlassComposer) -> Div {
    let GlassComposer {
        field,
        leading,
        strip,
        tools,
        submit,
    } = composer;
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1p5()
        .p_2()
        .rounded(px(t::radius::XL))
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .bg(t::glass::FILL_CARD.to_hsla())
        .children(leading)
        .children(strip)
        // EXP-525: a flex-COLUMN slot, not a nested row — the view child's
        // percent width resolved against unclamped avail in a row hop
        // (EXP-436 class) and the composer collapsed to placeholder width at
        // some window sizes; a column stretches its child to the definite
        // slot width instead.
        .child(v_flex().w_full().min_w_0().child(field))
        .child(
            h_flex()
                .w_full()
                .gap_1()
                .px_1()
                .pb_1()
                .items_center()
                .children(tools)
                .child(div().flex_1())
                .children(submit),
        )
}

/// One leading tool of the composer's bottom row: a muted 24px ghost glyph
/// button. The caller adds the tooltip, the disabled state and the handler.
pub(crate) fn composer_tool(
    id: impl Into<gpui::ElementId>,
    icon: crate::icons::ExpIcon,
    cx: &App,
) -> Button {
    Button::new(id)
        .ghost()
        .web_icon_xs()
        .icon(Icon::new(icon).text_color(cx.theme().muted_foreground))
}

/// EXP-599/EXP-698: the composer's send — a ROUND GHOST capsule around the
/// glyph, primary-tinted, never a filled button (the `ui-submit` glyph is
/// itself a circled arrow, and a filled box would draw a second ring around
/// it). `icon` differs by surface: `UI_SUBMIT` for comments, `UI_SEND` for
/// steer and helpdesk replies — same shape, same tint, same 32px hit box.
pub(crate) fn composer_submit(
    id: impl Into<gpui::ElementId>,
    icon: crate::icons::ExpIcon,
    disabled: bool,
    cx: &App,
) -> Button {
    // Web `disabled:opacity-40` — the explicit icon tint would otherwise
    // override the ghost variant's own disabled treatment.
    let tint = if disabled {
        cx.theme().primary.opacity(0.4)
    } else {
        cx.theme().primary
    };
    Button::new(id)
        .ghost()
        .with_size(px(t::size::CONTROL_MD))
        .rounded_full()
        .cursor_pointer()
        .icon(Icon::new(icon).text_color(tint))
        .disabled(disabled)
}
