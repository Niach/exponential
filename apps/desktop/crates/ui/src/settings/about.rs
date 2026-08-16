//! Settings → About (EXP-262): version, project links, and the third-party
//! licence notice every distributed build must reproduce.
//!
//! The notice is [`crate::licenses::NOTICES`] — the same `include_str!` blob
//! whose embedding EXP-375/EXP-376 gated. EXP-416: it is split into ~18.8k
//! pre-wrapped rows ONCE per process and rendered through `v_virtual_list`.
//! A single `TextView` shaped the whole 822 KB blob as one text run, which
//! froze the window for seconds on every open. Virtual rows cannot be
//! drag-selected, so the card header carries a Copy button — the escape
//! hatch for "reproduce the licence".
//!
//! The list sits in a FIXED-height box on purpose: `SettingsView::render`
//! wraps every pane in `overflow_y_scroll`, so the inner list needs a bounded
//! viewport to virtualize against.

use std::rc::Rc;
use std::sync::OnceLock;

use gpui::{
    div, px, size, App, ClipboardItem, InteractiveElement as _, IntoElement, ParentElement, Pixels,
    Render, SharedString, Size, Styled, Window,
};
use gpui_component::{
    button::Button,
    h_flex,
    scroll::{ScrollableElement as _, ScrollbarAxis},
    v_flex, v_virtual_list, ActiveTheme as _, Icon, VirtualListScrollHandle,
};

use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::licenses;

use super::{card_header, open_url, row_stroke, section};

const SOURCE_URL: &str = "https://github.com/Niach/exponential";
const LICENSE_URL: &str = "https://github.com/Niach/exponential/blob/master/LICENSE";

/// The locked cross-client blurb — same wording on web, iOS and Android.
const LICENSES_BLURB: &str = "Exponential is built with open-source software. \
     These licenses cover the components bundled in this build.";

/// Uniform virtual-row height for the notice (the `diff.rs` code-row rhythm).
const NOTICE_ROW_H: f32 = 18.;
/// The licence-text convention, and what the 480px box fits without clipping.
const NOTICE_WRAP_COLS: usize = 80;
/// Mono size of a notice row (`diff.rs` `CODE_TEXT_SIZE` neighbourhood).
const NOTICE_TEXT_SIZE: f32 = 12.;

/// [`licenses::NOTICES`] split into display rows ONCE per process: one row per
/// line, long lines word-wrapped at [`NOTICE_WRAP_COLS`]. Rows are subslices
/// of the `include_str!` blob, so the cache costs one pointer pair per row and
/// rendering one never allocates.
fn notice_rows() -> &'static [&'static str] {
    static ROWS: OnceLock<Vec<&'static str>> = OnceLock::new();
    ROWS.get_or_init(|| wrap_notice_lines(licenses::NOTICES, NOTICE_WRAP_COLS))
}

/// Greedy word wrap at `cols` characters: break at the last space that fits,
/// hard-break runs with no space in them (URLs, rule lines). The break space
/// itself is dropped; every other byte survives in order.
fn wrap_notice_lines(text: &'static str, cols: usize) -> Vec<&'static str> {
    // A zero width would never advance — the loop below must always consume.
    let cols = cols.max(1);
    let mut rows = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while rest.chars().count() > cols {
            // Byte index of the first char PAST the budget (char boundary, so
            // non-ASCII licence bodies can't panic the slice).
            let limit = rest
                .char_indices()
                .nth(cols)
                .map(|(ix, _)| ix)
                .unwrap_or(rest.len());
            let split = rest[..limit].rfind(' ').filter(|ix| *ix > 0).unwrap_or(limit);
            rows.push(&rest[..split]);
            rest = rest[split..].trim_start_matches(' ');
        }
        rows.push(rest);
    }
    rows
}

/// The About pane (`SettingsSection::About`) — everything it shows is compiled
/// in; the state is the notice list's scroll position and row geometry.
pub struct AboutPane {
    notices_scroll: VirtualListScrollHandle,
    notice_sizes: Rc<Vec<Size<Pixels>>>,
}

impl AboutPane {
    pub fn new() -> Self {
        Self {
            notices_scroll: VirtualListScrollHandle::new(),
            notice_sizes: Rc::new(vec![
                size(px(0.), px(NOTICE_ROW_H));
                notice_rows().len()
            ]),
        }
    }

    fn link_button(id: &'static str, label: &'static str, url: &'static str) -> Button {
        Button::new(id)
            .outline().cursor_pointer()
            .web_xs()
            .icon(Icon::new(registry::UI_EXTERNAL_LINK))
            .label(label)
            .on_click(|_, _, cx| open_url(cx, url.to_string()))
    }

    fn render_about(&self, cx: &App) -> impl IntoElement {
        section(cx)
            .child(card_header(
                "About",
                format!(
                    "Exponential · Version {}",
                    domain::client_version::current_version()
                ),
                cx,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .child(Self::link_button("about-source", "Source code", SOURCE_URL))
                    .child(Self::link_button(
                        "about-license",
                        "License (Apache-2.0)",
                        LICENSE_URL,
                    )),
            )
    }

    fn render_licenses(&mut self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        section(cx)
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .justify_between()
                    .gap_2()
                    .child(card_header("Third-party licenses", LICENSES_BLURB, cx))
                    .child(
                        Button::new("about-notices-copy")
                            .outline().cursor_pointer()
                            .web_xs()
                            .label("Copy")
                            .tooltip("Copy the full notice")
                            .on_click(|_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    licenses::NOTICES.to_string(),
                                ))
                            }),
                    ),
            )
            .child(
                div()
                    .id("about-notices")
                    .h(px(480.))
                    .w_full()
                    .overflow_hidden()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(row_stroke(cx))
                    .px_3()
                    .py_2()
                    .child(
                        v_flex()
                            .id("about-notices-list")
                            .relative()
                            .size_full()
                            .child(
                                v_virtual_list(
                                    entity,
                                    "about-notices-rows",
                                    self.notice_sizes.clone(),
                                    |_, visible_range, _window, cx| {
                                        let rows = notice_rows();
                                        let mono = cx.theme().mono_font_family.clone();
                                        visible_range
                                            .map(|ix| {
                                                div()
                                                    .h(px(NOTICE_ROW_H))
                                                    .whitespace_nowrap()
                                                    .text_size(px(NOTICE_TEXT_SIZE))
                                                    .font_family(mono.clone())
                                                    .child(SharedString::from(
                                                        *rows.get(ix).unwrap_or(&""),
                                                    ))
                                                    .into_any_element()
                                            })
                                            .collect()
                                    },
                                )
                                .track_scroll(&self.notices_scroll),
                            )
                            .scrollbar(&self.notices_scroll, ScrollbarAxis::Vertical),
                    ),
            )
    }
}

impl Render for AboutPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_6()
            .child(self.render_about(cx))
            .child(self.render_licenses(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_lines_pass_through_untouched() {
        let rows = wrap_notice_lines("MIT License\n\nCopyright (c) 2026\n", 80);
        assert_eq!(rows, vec!["MIT License", "", "Copyright (c) 2026"]);
    }

    #[test]
    fn long_lines_break_at_a_word_boundary() {
        let text = "aaaa bbbb cccc dddd eeee";
        let rows = wrap_notice_lines(text, 10);
        assert_eq!(rows, vec!["aaaa bbbb", "cccc dddd", "eeee"]);
        // Only the break spaces are lost — rejoining restores the line.
        assert_eq!(rows.join(" "), text);
        assert!(rows.iter().all(|row| !row.starts_with(' ')));
    }

    #[test]
    fn unbroken_runs_hard_break_at_the_column() {
        static RUN: &str = "xxxxxxxxxxxxxxxxxxxxxxxxx";
        let rows = wrap_notice_lines(RUN, 10);
        assert_eq!(rows, vec!["xxxxxxxxxx", "xxxxxxxxxx", "xxxxx"]);
        assert_eq!(rows.concat(), RUN);
    }

    #[test]
    fn every_row_fits_the_budget() {
        // The real blob: the wrap is what makes the 480px box readable without
        // horizontal scrolling.
        assert!(notice_rows()
            .iter()
            .all(|row| row.chars().count() <= NOTICE_WRAP_COLS));
    }
}
