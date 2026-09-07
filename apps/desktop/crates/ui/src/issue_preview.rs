//! EXP-760 — the issue-chip HOVER PREVIEW (web parity target:
//! `apps/web/src/components/issue-preview-card.tsx`).
//!
//! Linear's behaviour: rest the pointer on a `#IDENT` pill and a small card
//! fades in under it with everything the pill cannot fit — the assignee, the
//! full title, the status, the priority and the labels. It is DISPLAY-ONLY:
//! nothing in it is clickable, so a pointer that wanders into it simply
//! closes it again.
//!
//! ## Why a host entity and not `.tooltip`
//!
//! gpui's `InteractiveText::tooltip` anchors at the POINTER and keeps the
//! same tooltip alive while the pointer slides from one pill to the next on
//! the same line — both wrong here (a preview has to hang off the pill it
//! describes, and switching pills has to switch cards). So the surfaces
//! report hover transitions to ONE host per window instead, and the host owns
//! the open/close delays, the anchor rectangle and the overlay:
//!
//! - read-only prose (`markdown::MarkdownView`) pairs `on_hover`'s character
//!   index with the line's pill ranges ([`pill_at_index`]);
//! - the WYSIWYG description forwards `MarkdownEditorEvent::ReferenceHover`;
//! - relation rows report their own hover with the row's bounds.
//!
//! The host is registered per `WindowId` in a Global (the `NavRegistry`
//! precedent) and mounted ONCE by [`crate::shell::Shell`], under the dialog
//! layers.

use std::ops::Range;
use std::time::Duration;

use gpui::{
    canvas, deferred, div, px, AnyElement, App, AppContext as _, Bounds, Entity, Global,
    IntoElement,
    MouseDownEvent, MouseMoveEvent, ParentElement as _, Pixels, Render, ScrollWheelEvent,
    SharedString, Styled, Task, Window, WindowId,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Sizable as _};
use std::collections::HashMap;

use domain::options::get_issue_priority_config;
use domain::rows::Issue;
use sync::Store;

use crate::icons::option_icon;

/// Card width. Wide enough for a two-line title at `text_sm`, narrow enough
/// to sit beside a pill without covering the paragraph it came from.
const CARD_W: f32 = 320.;

/// How long the pointer must rest on a pill before the card opens. Linear's
/// dwell; short enough to feel deliberate, long enough that dragging the
/// pointer across a paragraph of chips opens nothing.
const OPEN_DELAY: Duration = Duration::from_millis(400);

/// Grace period after the pointer leaves, so crossing a 1px gap between two
/// segments of the SAME pill (a wrapped chip) does not flicker the card.
const CLOSE_DELAY: Duration = Duration::from_millis(120);

/// Gap between the anchor's bottom edge and the card.
const CARD_OFFSET: f32 = 6.;

/// How far outside the anchor the pointer may stray before the card is
/// dropped by the window-level backstop.
const ANCHOR_SLACK: f32 = 4.;

// ---------------------------------------------------------------------------
// The card
// ---------------------------------------------------------------------------

/// The preview body for one issue, or `None` when the row has not synced (a
/// pill can outlive its issue: another team's board, a trashed board). The
/// host renders nothing at all in that case rather than an empty card.
pub(crate) fn card(issue_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = Store::global(cx).collections();
    let issue: Issue = collections.issues.read(cx).get(issue_id).cloned()?;

    let status = crate::queries::resolve_issue_status(cx, &issue);
    let priority = get_issue_priority_config(issue.priority);

    // Assignee, when the user row has synced.
    let assignee = issue.assignee_id.as_deref().and_then(|user_id| {
        collections
            .users
            .read(cx)
            .get(user_id)
            .map(|user| (user.id.clone(), user.name.clone(), user.email.clone(), user.image.clone()))
    });

    let label_ids: Vec<String> = collections
        .issue_labels
        .read(cx)
        .iter()
        .filter(|link| link.issue_id == issue.id)
        .map(|link| link.label_id.clone())
        .collect();
    let labels: Vec<(String, Option<String>)> = collections
        .labels
        .read(cx)
        .iter()
        .filter(|label| label_ids.contains(&label.id))
        .map(|label| (label.name.clone(), label.color.clone()))
        .collect();

    let muted = cx.theme().muted_foreground;
    let mut header = h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .font_family(theme::terminal::FONT_FAMILY)
                .child(SharedString::from(issue.identifier.clone())),
        )
        .child(div().flex_1().min_w_0());
    if let Some((user_id, name, email, image)) = assignee {
        let label = name
            .clone()
            .or_else(|| email.clone())
            .unwrap_or_else(|| user_id.clone());
        header = header.child(
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap_1p5()
                .child(crate::user_avatar::avatar_element(
                    &user_id,
                    &label,
                    crate::user_avatar::cached_avatar_image(cx, image.as_deref()),
                    gpui_component::Size::XSmall,
                ))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(label)),
                ),
        );
    }

    let mut chips = h_flex().w_full().flex_wrap().gap_1().items_center().child(
        crate::surface::glass_pill(
            "issue-preview-status",
            crate::surface::PillSize::Sm,
            crate::surface::PillMode::Readonly,
            cx,
        )
        .child(crate::icons::resolved_status_icon(&status, cx).xsmall())
        .child(
            div()
                .text_xs()
                .child(SharedString::from(status.name.clone())),
        ),
    );
    // `None` priority is the absence of one — a "No priority" chip would be
    // the widest thing on the card and say nothing (web hides it too).
    if issue.priority != domain::IssuePriority::None {
        chips = chips.child(
            crate::surface::glass_pill(
                "issue-preview-priority",
                crate::surface::PillSize::Sm,
                crate::surface::PillMode::Readonly,
                cx,
            )
            .child(option_icon(priority, cx).xsmall())
            .child(div().text_xs().child(priority.label)),
        );
    }
    for (index, (name, color)) in labels.into_iter().enumerate() {
        let tint = color
            .as_deref()
            .and_then(crate::settings::parse_hex_color)
            .unwrap_or(muted);
        chips = chips.child(
            crate::surface::glass_pill(
                ("issue-preview-label", index),
                crate::surface::PillSize::Sm,
                crate::surface::PillMode::Readonly,
                cx,
            )
            .child(div().flex_shrink_0().size_1p5().rounded_full().bg(tint))
            .child(div().text_xs().child(SharedString::from(name))),
        );
    }

    Some(
        v_flex()
            .w(px(CARD_W))
            .gap_1p5()
            .px_3()
            .py_2p5()
            .rounded(px(theme::tokens::radius::LG))
            .border_1()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            // OPAQUE: the card floats over prose, and a translucent fill
            // would show the very text it is meant to explain through it
            // (`surface::glass_bar`'s rule).
            .bg(cx.theme().popover)
            .shadow_md()
            .child(header)
            .child(
                div()
                    .w_full()
                    .text_sm()
                    .line_clamp(2)
                    .child(SharedString::from(issue.title.clone())),
            )
            .child(chips)
            .into_any_element(),
    )
}

/// The pill a hovered CHARACTER index falls inside, if any.
///
/// `InteractiveText::on_hover` reports a character offset into the rendered
/// line rather than the index of a registered range, so a pill is found by
/// containment. `None` (the pointer left the text) resolves to no pill.
pub(crate) fn pill_at_index(
    pills: &[(Range<usize>, String)],
    index: Option<usize>,
) -> Option<(Range<usize>, String)> {
    let index = index?;
    pills
        .iter()
        .find(|(range, _)| range.contains(&index))
        .cloned()
}

// ---------------------------------------------------------------------------
// The host
// ---------------------------------------------------------------------------

struct Pending {
    /// Identifies the requesting pill, so a `release` from a pill the pointer
    /// has already left cannot close the card the NEXT pill just opened.
    key: SharedString,
    issue_id: String,
    anchor: Bounds<Pixels>,
    open: bool,
}

/// One per window: the open/close state machine plus the overlay element.
pub(crate) struct IssuePreviewHost {
    pending: Option<Pending>,
    timer: Option<Task<()>>,
}

impl IssuePreviewHost {
    fn new() -> Self {
        Self {
            pending: None,
            timer: None,
        }
    }

    /// The pointer entered (or is still inside) the pill `key`. Re-requesting
    /// the SAME key only refreshes the anchor — a card already up never
    /// re-runs its delay while the pointer slides along its own pill.
    pub(crate) fn request(
        &mut self,
        key: impl Into<SharedString>,
        issue_id: impl Into<String>,
        anchor: Bounds<Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = key.into();
        if let Some(pending) = &mut self.pending {
            if pending.key == key {
                if pending.anchor != anchor {
                    pending.anchor = anchor;
                    if pending.open {
                        cx.notify();
                    }
                }
                return;
            }
        }
        // A different pill: the old card goes immediately (no cross-fade —
        // two previews on screen at once would be worse than a blink).
        let was_open = self.pending.as_ref().is_some_and(|p| p.open);
        self.pending = Some(Pending {
            key: key.clone(),
            issue_id: issue_id.into(),
            anchor,
            open: false,
        });
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(OPEN_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                if let Some(pending) = &mut this.pending {
                    if pending.key == key && !pending.open {
                        pending.open = true;
                        cx.notify();
                    }
                }
            });
        }));
        if was_open {
            cx.notify();
        }
    }

    /// The pointer left the pill `key`. A stale key is ignored.
    pub(crate) fn release(&mut self, key: impl Into<SharedString>, cx: &mut gpui::Context<Self>) {
        let key = key.into();
        if self.pending.as_ref().is_none_or(|p| p.key != key) {
            return;
        }
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CLOSE_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                if this.pending.as_ref().is_some_and(|p| p.key == key) {
                    this.dismiss(cx);
                }
            });
        }));
    }

    /// Close now, whatever is up (a scroll, a click, a lost window focus).
    pub(crate) fn dismiss(&mut self, cx: &mut gpui::Context<Self>) {
        let was_open = self.pending.as_ref().is_some_and(|p| p.open);
        self.pending = None;
        self.timer = None;
        if was_open {
            cx.notify();
        }
    }
}

impl Render for IssuePreviewHost {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // The window lost focus while a card was up: it would otherwise hang
        // over a window the user is no longer pointing at.
        if !window.is_window_active() && self.pending.is_some() {
            self.dismiss(cx);
        }
        let Some(pending) = self.pending.as_ref().filter(|pending| pending.open) else {
            return div();
        };
        let anchor = pending.anchor;
        let Some(card) = card(&pending.issue_id, cx) else {
            return div();
        };

        // Window-level backstop (the image-resize precedent above in
        // `markdown::editor`): the surfaces report leaving their own pill,
        // but a pointer that jumps straight into a DIFFERENT element — or a
        // scroll that moves the pill out from under it — never sends one.
        let entity = cx.entity();
        let backstop = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let moved = entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if !phase.bubble() {
                        return;
                    }
                    if !anchor
                        .dilate(px(ANCHOR_SLACK))
                        .contains(&event.position)
                    {
                        moved.update(cx, |this, cx| this.dismiss(cx));
                    }
                });
                let pressed = entity.clone();
                window.on_mouse_event(move |_: &MouseDownEvent, phase, _, cx| {
                    if phase.bubble() {
                        pressed.update(cx, |this, cx| this.dismiss(cx));
                    }
                });
                let scrolled = entity.clone();
                window.on_mouse_event(move |_: &ScrollWheelEvent, phase, _, cx| {
                    if phase.bubble() {
                        scrolled.update(cx, |this, cx| this.dismiss(cx));
                    }
                });
            },
        )
        .absolute()
        .size_full();

        div()
            .child(backstop)
            .child(
                deferred(
                    gpui::anchored()
                        .position(gpui::point(
                            anchor.origin.x,
                            anchor.origin.y + anchor.size.height + px(CARD_OFFSET),
                        ))
                        .snap_to_window_with_margin(px(8.))
                        .child(card),
                )
                // Above the panel chrome, below the modal dialog layers.
                .with_priority(3),
            )
    }
}

// ---------------------------------------------------------------------------
// Per-window registry (the `NavRegistry` precedent)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct PreviewRegistry {
    by_window: HashMap<WindowId, Entity<IssuePreviewHost>>,
}

impl Global for PreviewRegistry {}

/// This window's preview host, created on first access.
pub(crate) fn host_for_window(window: &Window, cx: &mut App) -> Entity<IssuePreviewHost> {
    let window_id = window.window_handle().window_id();
    if let Some(existing) = cx
        .try_global::<PreviewRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
    {
        return existing;
    }
    let host = cx.new(|_| IssuePreviewHost::new());
    cx.default_global::<PreviewRegistry>()
        .by_window
        .insert(window_id, host.clone());
    host
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hover index is a CHARACTER offset, not a range index: a pill is
    /// found by containment, and everything between pills (and `None`, the
    /// pointer leaving the text) resolves to no preview.
    #[test]
    fn pills_resolve_by_containment() {
        let pills = vec![(3..9, "i-1".to_string()), (20..30, "i-2".to_string())];
        assert_eq!(
            pill_at_index(&pills, Some(3)).map(|(_, id)| id),
            Some("i-1".to_string())
        );
        assert_eq!(
            pill_at_index(&pills, Some(8)).map(|(_, id)| id),
            Some("i-1".to_string())
        );
        // Exclusive end.
        assert!(pill_at_index(&pills, Some(9)).is_none());
        assert_eq!(
            pill_at_index(&pills, Some(25)).map(|(_, id)| id),
            Some("i-2".to_string())
        );
        assert!(pill_at_index(&pills, Some(0)).is_none());
        assert!(pill_at_index(&pills, Some(200)).is_none());
        assert!(pill_at_index(&pills, None).is_none());
        assert!(pill_at_index(&[], Some(3)).is_none());
    }
}
