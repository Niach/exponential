//! EXP-885 — the ONE issue chip of the desktop app.
//!
//! An "issue chip" is the small badge that names ONE issue inline: its status
//! glyph, its `EXP-123` identifier and its title. The canonical style is the
//! issue-detail markdown editor's `#EXP-123` chip (EXP-322/EXP-381/EXP-423),
//! which the four clients already agree on:
//!
//! * a small rounded RECT — radius [`ISSUE_CHIP_RADIUS`], never a capsule;
//! * a 1px `border` hairline over an `accent` fill (the fill barely clears the
//!   surface behind it, so the border is what makes the chip legible);
//! * padding [`ISSUE_CHIP_PAD_X`] × [`ISSUE_CHIP_PAD_Y`];
//! * the status glyph, capped at [`ISSUE_CHIP_ICON_MAX`] and tinted by
//!   [`crate::icons::status_tint_color`];
//! * the identifier in the MONO font, muted (`muted_foreground`);
//! * the title in the UI font, `foreground`, MEDIUM, truncated.
//!
//! The markdown surfaces paint that chip by hand out of a text layout — the
//! WYSIWYG editor from the vendored theme's `issue_chip_radius` /
//! `reference_pad_*`, the read-only renderer from [`ISSUE_CHIP_RADIUS`] &
//! friends here (`markdown::editor`'s `PillText` imports them, so this module
//! is the single source of truth on the host side; [`vendored_geometry_matches`]
//! + its test pin the two sets together).
//!
//! EVERY OTHER surface — composer subject chips, tool-result previews — builds
//! the chip as an ELEMENT through [`issue_chip`], so there is one shape, one
//! set of colours and one set of numbers instead of a capsule here and a
//! rounded rect there.
//!
//! What this is NOT: a list row. `run_rows`, `issue_relations`, the issue
//! lists and the tab strip name an issue too, but they are rows/tabs with
//! their own chrome (EXP-818: a filled group band over FLAT rows) and they
//! deliberately do not wear this badge.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, ClickEvent, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, RenderOnce, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window,
};
use gpui_component::{h_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::statuses::ResolvedStatus;

use crate::icons::registry;

/// The chip's corner radius. Mirrors the vendored WYSIWYG theme's
/// `issue_chip_radius` default (`gpui-markdown-editor/src/theme.rs`).
pub(crate) const ISSUE_CHIP_RADIUS: f32 = 4.0;
/// Horizontal padding — the vendored theme's `reference_pad_x`.
pub(crate) const ISSUE_CHIP_PAD_X: f32 = 3.0;
/// Vertical padding — the vendored theme's `reference_pad_y`.
pub(crate) const ISSUE_CHIP_PAD_Y: f32 = 1.0;
/// The status glyph's cap (EXP-469: bigger than this and it painted flush
/// against the chip's edge in the markdown gutter).
pub(crate) const ISSUE_CHIP_ICON_MAX: f32 = 14.0;
/// The gap between glyph, identifier and title. In the painted chips this is
/// a literal space glyph; here it is the nearest fixed number.
const ISSUE_CHIP_GAP: f32 = 4.0;
/// The removable variant's trailing ✕ box. [`crate::composer::composer_tool`]
/// is a 24px circle by default — the size a COMPOSER's toolbar wants, and
/// half again the height of a chip whose own content is 14px. Same glyph,
/// same ghost hover, a box that fits.
const ISSUE_CHIP_REMOVE_SIZE: f32 = 16.0;

/// The vendored editor crate carries its OWN copy of the geometry (it is a
/// standalone crate with a host-supplied theme, so it cannot import ours).
/// This is the cross-check: the numbers the vendored theme paints from must
/// be the numbers above. [`crate::wysiwyg::theme_bridge`] — the one place
/// they could drift — `debug_assert!`s it on every theme build, and
/// [`tests::geometry_matches_the_vendored_theme`] pins it in CI.
pub(crate) fn vendored_geometry_matches(radius: f32, pad_x: f32, pad_y: f32) -> bool {
    radius == ISSUE_CHIP_RADIUS && pad_x == ISSUE_CHIP_PAD_X && pad_y == ISSUE_CHIP_PAD_Y
}

/// The status of a SYNCED issue, by id — the read a chip does when its
/// surface holds an id but no row (support's linked issue, the duplicate-of
/// banner, an attachments table). `None` when the issue is not synced here,
/// which is exactly when the chip must not claim a status.
pub(crate) fn synced_issue_status(issue_id: &str, cx: &App) -> Option<ResolvedStatus> {
    let store = sync::Store::try_global(cx)?;
    let issue = store.collections().issues.read(cx).get(issue_id).cloned()?;
    Some(crate::queries::resolve_issue_status(cx, &issue))
}

type ChipHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// One issue chip. Build it with [`issue_chip`].
#[derive(IntoElement)]
pub(crate) struct IssueChip {
    id: ElementId,
    identifier: SharedString,
    title: SharedString,
    /// `None` = the issue's status is not known here (an unsynced row a tool
    /// answer named). The chip then leads with the plain issues glyph rather
    /// than inventing a status.
    status: Option<ResolvedStatus>,
    max_title_width: Option<Pixels>,
    /// Take the row's leftover width and truncate the title into it, instead
    /// of the default "never shrink, the parent wraps me" a chip row wants.
    flexible: bool,
    on_click: Option<ChipHandler>,
    on_remove: Option<(ElementId, ChipHandler)>,
}

/// The ONE issue badge (EXP-885). `title` may be empty — the identifier alone
/// then labels the chip.
pub(crate) fn issue_chip(
    id: impl Into<ElementId>,
    identifier: impl Into<SharedString>,
    title: impl Into<SharedString>,
) -> IssueChip {
    IssueChip {
        id: id.into(),
        identifier: identifier.into(),
        title: title.into(),
        status: None,
        max_title_width: None,
        flexible: false,
        on_click: None,
        on_remove: None,
    }
}

impl IssueChip {
    /// The issue's RESOLVED status (`domain::statuses::resolve_status_sorted`
    /// / [`crate::queries::resolve_issue_status`]) — its glyph and tint lead
    /// the chip.
    pub(crate) fn status(mut self, status: ResolvedStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Cap the title's width (a composer chip row wraps; a chip in prose does
    /// not). Without it the title takes what the parent gives it.
    pub(crate) fn max_title_width(mut self, width: Pixels) -> Self {
        self.max_title_width = Some(width);
        self
    }

    /// Let the chip take a ROW's leftover width and truncate its title into
    /// it (the duplicate-of banner, a table cell) rather than sizing to its
    /// content and letting the parent wrap it (a composer chip row).
    pub(crate) fn flexible(mut self) -> Self {
        self.flexible = true;
        self
    }

    /// Open the issue (or whatever the surface means by "this chip").
    pub(crate) fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    /// The composer variant: a trailing ✕ INSIDE the chip, after the title.
    pub(crate) fn on_remove(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_remove = Some((id.into(), Box::new(handler)));
        self
    }
}

impl RenderOnce for IssueChip {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        // The vendored theme bridge's `reference_*` mapping, read back from
        // the same app theme so the element chip and the painted one are the
        // same colours: bg = accent, border = border, token = muted, text =
        // foreground.
        let background = theme.accent;
        let border = theme.border;
        let token = theme.muted_foreground;
        let text = theme.foreground;
        let ring = theme.ring;
        let clickable = self.on_click.is_some();

        let glyph = match &self.status {
            Some(status) => crate::icons::resolved_status_icon(status, cx),
            None => Icon::new(registry::NAV_ISSUES).text_color(token),
        }
        .with_size(px(ISSUE_CHIP_ICON_MAX));

        let mut chip = h_flex()
            .id(self.id)
            .map(|chip| {
                if self.flexible {
                    chip.min_w_0()
                } else {
                    chip.flex_shrink_0()
                }
            })
            .items_center()
            .gap(px(ISSUE_CHIP_GAP))
            .px(px(ISSUE_CHIP_PAD_X))
            .py(px(ISSUE_CHIP_PAD_Y))
            .rounded(px(ISSUE_CHIP_RADIUS))
            .border_1()
            .border_color(border)
            .bg(background)
            .text_xs()
            .child(div().flex_shrink_0().child(glyph));
        if !self.identifier.is_empty() {
            chip = chip.child(
                div()
                    .flex_shrink_0()
                    .font_family(theme.mono_font_family.clone())
                    .text_color(token)
                    .child(self.identifier),
            );
        }
        if !self.title.is_empty() {
            let mut title = div()
                .min_w_0()
                .truncate()
                .text_color(text)
                .font_weight(FontWeight::MEDIUM)
                .child(self.title);
            if let Some(width) = self.max_title_width {
                title = title.max_w(width);
            }
            if self.flexible {
                title = title.flex_1();
            }
            chip = chip.child(title);
        }
        if let Some((remove_id, handler)) = self.on_remove {
            chip = chip.child(
                gpui::Styled::size(
                    crate::composer::composer_tool(remove_id, registry::UI_CLOSE, cx),
                    px(ISSUE_CHIP_REMOVE_SIZE),
                )
                .flex_shrink_0()
                .tooltip("Remove")
                    .on_click(move |event, window, cx| handler(event, window, cx)),
            );
        }
        if let Some(handler) = self.on_click {
            chip = chip.on_click(move |event, window, cx| handler(event, window, cx));
        }
        chip.when(clickable, |chip| {
            // The only hover the painted chips could not have: the border
            // brightens to the focus ring, nothing moves.
            chip.cursor_pointer()
                .hover(|style| style.border_color(ring))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-885: the numbers this module owns ARE the vendored WYSIWYG theme's
    /// defaults. The vendored crate pins its own side in
    /// `gpui-markdown-editor/src/theme.rs`'s dimension test; this is the host
    /// half of the same lock, so a drift on either side fails a build.
    #[test]
    fn geometry_matches_the_vendored_theme() {
        let dimensions = gpui_markdown_editor::MarkdownEditorTheme::default_theme().dimensions;
        assert!(vendored_geometry_matches(
            dimensions.issue_chip_radius,
            dimensions.reference_pad_x,
            dimensions.reference_pad_y,
        ));
        assert_eq!(ISSUE_CHIP_RADIUS, 4.0);
        assert_eq!(ISSUE_CHIP_PAD_X, 3.0);
        assert_eq!(ISSUE_CHIP_PAD_Y, 1.0);
        assert_eq!(ISSUE_CHIP_ICON_MAX, 14.0);
    }

    #[test]
    fn geometry_mismatch_is_caught() {
        assert!(!vendored_geometry_matches(999.0, 3.0, 1.0));
        assert!(!vendored_geometry_matches(4.0, 6.0, 1.0));
        assert!(!vendored_geometry_matches(4.0, 3.0, 2.0));
    }
}
