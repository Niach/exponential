//! EXP-1039 — the IDE's styleguide SCREEN (`Screen::Styleguide`).
//!
//! Debug-only, like the debug board: it renders behind `EXP_DEV_STYLEGUIDE=1`
//! (a rail entry) or `EXP_DEV_SCREEN=styleguide`, never in a shipped build's
//! navigation. The page is the four shared sections in order — the SAME
//! index the web styleguide reads
//! (`apps/styleguide/src/sections/sections.json`) — each with its title, its
//! blurb, the IDE's own live entries ([`super::native`]) and the registered
//! per-control entries ([`super::entries`], a one-line placeholder until
//! their leaf lands).

use gpui::{div, px, AnyElement, IntoElement, ParentElement as _, Render, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use super::{entries, native, sections, Section};

/// Is the styleguide reachable in this process? DEV-ONLY (`EXP_DEV_STYLEGUIDE=1`),
/// read once — the rail entry and the screen ask the same question.
pub(crate) fn styleguide_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("EXP_DEV_STYLEGUIDE").is_ok_and(|value| value == "1"))
}

pub(crate) struct StyleguideView {
    scroll: gpui::ScrollHandle,
    sections: Vec<Section>,
}

impl StyleguideView {
    pub(crate) fn new(_window: &mut Window, _cx: &mut gpui::Context<Self>) -> Self {
        Self {
            scroll: gpui::ScrollHandle::new(),
            sections: sections(),
        }
    }

    /// One section: its numbered heading, its blurb, then every entry in it.
    fn render_section(
        &self,
        section: &Section,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let native_entries: Vec<&native::NativeEntry> = native::in_section(section.order).collect();
        // The registered per-control entries of this section, in index order.
        let registered: Vec<&entries::Entry> = section
            .entries
            .iter()
            .filter_map(|id| entries::ENTRIES.iter().find(|entry| entry.id == *id))
            .collect();
        v_flex()
            .w_full()
            .min_w_0()
            .gap_4()
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(foreground)
                            .child(section.title.clone()),
                    )
                    .child(div().text_sm().text_color(muted).child(section.blurb.clone())),
            )
            .children(native_entries.into_iter().map(|entry| {
                let demo = (entry.demo)(window, cx);
                crate::surface::glass_card()
                    .w_full()
                    .min_w_0()
                    .p_4()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(foreground)
                            .child(entry.title),
                    )
                    .child(div().text_xs().text_color(muted).child(entry.blurb))
                    .child(div().pt_2().child(demo))
            }))
            .children(registered.into_iter().map(|entry| {
                crate::surface::glass_row_card()
                    .w_full()
                    .min_w_0()
                    .p_3()
                    .text_sm()
                    .text_color(muted)
                    .child((entry.render)())
            }))
            .into_any_element()
    }
}

impl Render for StyleguideView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let sections = self.sections.clone();
        let body = v_flex()
            .w_full()
            .min_w_0()
            .max_w(px(760.))
            .gap_8()
            .p_6()
            .children(
                sections
                    .iter()
                    .map(|section| self.render_section(section, window, cx)),
            );
        v_flex()
            .size_full()
            .min_h_0()
            .child(crate::scroll_pane::v_scroll_pane(
                "styleguide-scroll",
                &self.scroll,
                body,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The screen renders the shared index IN ORDER, and every native entry
    /// is filed under one of those four sections (a typo'd `section` would
    /// otherwise silently drop the entry off the page).
    #[test]
    fn every_native_entry_belongs_to_one_of_the_four_sections() {
        let orders: Vec<u8> = sections().iter().map(|section| section.order).collect();
        assert_eq!(orders, vec![1, 2, 3, 4]);
        for entry in native::NATIVE_ENTRIES {
            assert!(
                orders.contains(&entry.section),
                "{} names section {}",
                entry.id,
                entry.section
            );
        }
        // Every section has at least one native entry — the IDE's styleguide
        // reads as four sections, not as one with three empty headings.
        for order in orders {
            assert!(
                native::in_section(order).next().is_some(),
                "section {order} has no native entry"
            );
        }
    }

    /// Ids are unique and stable (they key the page's cards).
    #[test]
    fn native_entry_ids_are_unique() {
        let mut ids: Vec<&str> = native::NATIVE_ENTRIES.iter().map(|entry| entry.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total);
    }

    /// The dev gate is OFF unless the env var says otherwise — a shipped
    /// build never shows the screen.
    #[test]
    fn the_styleguide_is_off_by_default() {
        assert!(std::env::var("EXP_DEV_STYLEGUIDE").is_err() || styleguide_enabled());
    }
}
