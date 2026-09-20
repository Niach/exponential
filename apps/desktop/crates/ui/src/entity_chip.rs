//! EXP-920 — the ENTITY chip under a settled Exponential tool row.
//!
//! One chip per [`domain::entity_preview::group_preview_refs`] group: the
//! SAME box as [`crate::issue_chip`] (its radius, padding, hairline, fill
//! and glyph cap — the consts are shared, so the two never drift), led by
//!
//! * a synced issue's RESOLVED status glyph, else
//! * the kind's registry glyph ([`crate::entity_preview::ref_icon`]),
//!
//! then the label ([`domain::entity_preview::entity_chip_label`]) — an
//! issue's identifier in the muted mono slot, exactly as `issue_chip` paints
//! it, with the title ([`entity_chip_detail`]) in the medium slot; a synced
//! issue prefers its ROW's identifier + title over what the wire carried.
//!
//! Hovering opens the entity's card through the window's
//! [`crate::issue_preview::IssuePreviewHost`] (`.hover_card(key)`); a click
//! goes wherever the caller says (`.on_click`), or nowhere — an unresolvable
//! target leaves the chip inert.

use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, Bounds, ClickEvent, ElementId, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window,
};
use gpui_component::{h_flex, ActiveTheme as _, ElementExt as _, Icon, Sizable as _};

use domain::entity_preview as rule;
use steer::EntityRef;

use crate::issue_chip::{
    synced_issue_status, ISSUE_CHIP_ICON_MAX, ISSUE_CHIP_PAD_X, ISSUE_CHIP_PAD_Y,
    ISSUE_CHIP_RADIUS,
};

/// The gap between glyph, identifier and title (`issue_chip`'s number).
const CHIP_GAP: f32 = 4.0;
/// A chip's title slot never runs wider than this: the label is already
/// clamped to 48 code points, this only keeps one chip from owning the row.
const CHIP_TITLE_MAX_W: f32 = 280.0;

type ChipHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// One entity chip. Build it with [`entity_chip`].
#[derive(IntoElement)]
pub(crate) struct EntityChip {
    id: ElementId,
    r#ref: EntityRef,
    members: Vec<EntityRef>,
    hover_key: Option<SharedString>,
    on_click: Option<ChipHandler>,
}

pub(crate) fn entity_chip(
    id: impl Into<ElementId>,
    r#ref: &EntityRef,
    members: &[EntityRef],
) -> EntityChip {
    EntityChip {
        id: id.into(),
        r#ref: r#ref.clone(),
        members: members.to_vec(),
        hover_key: None,
        on_click: None,
    }
}

impl EntityChip {
    /// Open the entity's hover card under this chip, keyed so a release from
    /// a chip the pointer already left cannot close the next chip's card.
    pub(crate) fn hover_card(mut self, key: impl Into<SharedString>) -> Self {
        self.hover_key = Some(key.into());
        self
    }

    pub(crate) fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

/// What the chip paints: the leading glyph, the mono slot, the medium slot.
struct ChipParts {
    glyph: Icon,
    mono: Option<String>,
    title: Option<String>,
}

fn chip_parts(r#ref: &EntityRef, cx: &App) -> ChipParts {
    let token = cx.theme().muted_foreground;
    let view = crate::entity_preview::view(r#ref);
    if r#ref.kind == "issue" {
        // A synced row lends its status glyph and its own identifier +
        // title; the wire's strings only stand in while it has not synced.
        if let Some(issue) = crate::entity_preview::find_issue(&r#ref.id, cx) {
            let status = synced_issue_status(&issue.id, cx);
            let glyph = match &status {
                Some(status) => crate::icons::resolved_status_icon(status, cx),
                None => Icon::new(crate::entity_preview::ref_icon(r#ref)).text_color(token),
            };
            let title = issue.title.trim();
            return ChipParts {
                glyph,
                mono: Some(rule::clamp_chip_label(&issue.identifier)),
                title: (!title.is_empty()).then(|| rule::clamp_chip_label(title)),
            };
        }
        let label = rule::entity_chip_label(&view);
        let detail = rule::entity_chip_detail(&view);
        // The label IS the identifier only when the ref carried one; a
        // title-only issue reads its title in the medium slot.
        let has_identifier = r#ref
            .identifier
            .as_deref()
            .is_some_and(|identifier| !identifier.trim().is_empty());
        return ChipParts {
            glyph: Icon::new(crate::entity_preview::ref_icon(r#ref)).text_color(token),
            mono: has_identifier.then_some(label.clone()),
            title: if has_identifier { detail } else { Some(label) },
        };
    }
    ChipParts {
        glyph: Icon::new(crate::entity_preview::ref_icon(r#ref)).text_color(token),
        mono: None,
        title: Some(rule::entity_chip_label(&view)),
    }
}

impl RenderOnce for EntityChip {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let background = theme.accent;
        let border = theme.border;
        let token = theme.muted_foreground;
        let text = theme.foreground;
        let ring = theme.ring;
        let mono_font = theme.mono_font_family.clone();
        let clickable = self.on_click.is_some();
        let parts = chip_parts(&self.r#ref, cx);

        let mut chip = h_flex()
            .id(self.id)
            .flex_shrink_0()
            .items_center()
            .gap(px(CHIP_GAP))
            .px(px(ISSUE_CHIP_PAD_X))
            .py(px(ISSUE_CHIP_PAD_Y))
            .rounded(px(ISSUE_CHIP_RADIUS))
            .border_1()
            .border_color(border)
            .bg(background)
            .text_xs()
            .child(
                div()
                    .flex_shrink_0()
                    .child(parts.glyph.with_size(px(ISSUE_CHIP_ICON_MAX))),
            );
        if let Some(mono) = parts.mono.filter(|mono| !mono.is_empty()) {
            chip = chip.child(
                div()
                    .flex_shrink_0()
                    .font_family(mono_font)
                    .text_color(token)
                    .child(SharedString::from(mono)),
            );
        }
        if let Some(title) = parts.title.filter(|title| !title.is_empty()) {
            chip = chip.child(
                div()
                    .min_w_0()
                    .max_w(px(CHIP_TITLE_MAX_W))
                    .truncate()
                    .text_color(text)
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(title)),
            );
        }
        if let Some(key) = self.hover_key {
            // The chip's painted rectangle is the card's anchor, captured at
            // prepaint (the `issue_relations` row recipe): a hover listener
            // is handed the pointer, not the element.
            let anchor: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
            let anchor_write = anchor.clone();
            let r#ref = self.r#ref.clone();
            let members = self.members.clone();
            chip = chip
                .on_prepaint(move |bounds, _window, _cx| anchor_write.set(bounds))
                .on_hover(move |hovered, window, cx| {
                    let host = crate::issue_preview::host_for_window(window, cx);
                    if *hovered {
                        let bounds = anchor.get();
                        let r#ref = r#ref.clone();
                        let members = members.clone();
                        host.update(cx, |host, cx| {
                            host.request_entity(key.clone(), r#ref, members, bounds, cx)
                        });
                    } else {
                        host.update(cx, |host, cx| host.release(key.clone(), cx));
                    }
                });
        }
        if let Some(handler) = self.on_click {
            chip = chip.on_click(move |event, window, cx| {
                // The card must not outlive the surface the click leaves.
                let host = crate::issue_preview::host_for_window(window, cx);
                host.update(cx, |host, cx| host.dismiss(cx));
                handler(event, window, cx)
            });
        }
        chip.when(clickable, |chip| {
            chip.cursor_pointer()
                .hover(|style| style.border_color(ring))
        })
    }
}
