//! #EXP-1493 THE kind badge ×4 (stories.json `badge`; web `StoryKindBadge`):
//! THE Pill (sm, readonly) with the kind's concept glyph leading and its
//! copy word, tinted per kind (plan primary, question yellow, result
//! emerald): the word + glyph in the tint, the tint at `fill` alpha behind,
//! at `stroke` alpha as its border. Leads a story's title line (viewer,
//! Stories tab) and the plan page's header.

use gpui::{App, Hsla, IntoElement as _, ParentElement as _, Styled as _};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _};

use domain::stories::{StoryKind, STORIES};
use crate::surface::{glass_pill, PillMode, PillSize};

/// Tailwind emerald-500 (web `text-emerald-500`).
const EMERALD: u32 = 0x10b981;

/// The kind's tint (`badge.tints`).
pub(crate) fn kind_tint(kind: StoryKind, cx: &App) -> Hsla {
    match STORIES.badge.tints.of(kind) {
        "primary" => cx.theme().primary,
        "yellow" => theme::tokens::YELLOW.to_hsla(),
        _ => gpui::rgb(EMERALD).into(),
    }
}

/// THE kind badge.
pub(crate) fn kind_badge(id: impl Into<gpui::ElementId>, kind: StoryKind, cx: &App) -> gpui::AnyElement {
    let tint = kind_tint(kind, cx);
    let glyph = crate::icons::registry::concept_by_name(STORIES.badge.icons.of(kind)).unwrap_or(crate::icons::registry::UI_CHECK);
    glass_pill(id, PillSize::Sm, PillMode::Readonly, cx)
        .bg(tint.opacity(STORIES.badge.fill))
        .border_color(tint.opacity(STORIES.badge.stroke))
        .text_color(tint)
        .child(Icon::new(glyph).xsmall().text_color(tint))
        .child(kind.label())
        .into_any_element()
}
